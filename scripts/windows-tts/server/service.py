"""Core serial-queue TTS service with pre-enqueue/pre-GPU deadline guardrails."""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
import time
from typing import Callable, Dict, List, Optional, Tuple, TypeVar

from audio import DEFAULT_CHANNELS, compute_duration_ms, encode_wav_pcm16
from backends import TtsBackend
from personas import PersonaRegistry, PersonaValidationError, SynthesisVoicePlan
from vram_guard import DEFAULT_MAX_NEW_TOKENS, clamp_max_new_tokens

DEFAULT_DEADLINE_MS = 2500
SUPPORTED_FORMATS = ("wav", "pcm")

_T = TypeVar("_T")


class TtsServiceError(Exception):
    """Domain HTTP error raised by TtsService."""

    def __init__(
        self,
        message: str,
        *,
        status_code: int,
        error_code: str,
        headers: Optional[Dict[str, str]] = None,
    ) -> None:
        super().__init__(message)
        self.message = message
        self.status_code = status_code
        self.error_code = error_code
        self.headers = dict(headers or {})

    def to_payload(self) -> Dict[str, object]:
        return {
            "error": self.error_code,
            "message": self.message,
        }


class InvalidRequestError(TtsServiceError):
    def __init__(self, message: str, *, error_code: str = "invalid_request") -> None:
        super().__init__(message, status_code=400, error_code=error_code)


class ServiceNotReadyError(TtsServiceError):
    def __init__(self, message: str = "TTS backend is not ready yet") -> None:
        super().__init__(message, status_code=503, error_code="service_not_ready")


class DeadlineExceededError(TtsServiceError):
    def __init__(
        self,
        *,
        request_id: str,
        deadline_ms: int,
        queue_wait_ms: int,
    ) -> None:
        super().__init__(
            f"Request '{request_id}' exceeded deadline_ms={deadline_ms} (queue_wait_ms={queue_wait_ms})",
            status_code=408,
            error_code="deadline_exceeded",
            headers={
                "X-TTS-Request-Id": request_id,
                "X-TTS-Queue-Wait-Ms": str(queue_wait_ms),
                "X-TTS-Dropped-Reason": "deadline_exceeded",
            },
        )
        self.request_id = request_id
        self.deadline_ms = deadline_ms
        self.queue_wait_ms = queue_wait_ms

    def to_payload(self) -> Dict[str, object]:
        return {
            "error": self.error_code,
            "message": self.message,
            "request_id": self.request_id,
            "deadline_ms": self.deadline_ms,
            "queue_wait_ms": self.queue_wait_ms,
        }


@dataclass(frozen=True)
class SynthesizeRequest:
    request_id: str
    persona_id: str
    mood: str
    text: str
    instruct_override: Optional[str] = None
    format: str = "wav"
    stream: bool = False
    deadline_ms: int = DEFAULT_DEADLINE_MS


@dataclass(frozen=True)
class SynthesizeResponse:
    body: bytes
    content_type: str
    headers: Dict[str, str] = field(default_factory=dict)


@dataclass(frozen=True)
class SynthesizeStreamResponse:
    chunks: List[bytes]
    content_type: str
    headers: Dict[str, str] = field(default_factory=dict)


def _default_clock_ms() -> float:
    return time.monotonic() * 1000.0


class TtsService:
    """Single-concurrency serial GPU synthesis service with deadline shedding."""

    def __init__(
        self,
        *,
        backend: TtsBackend,
        registry: PersonaRegistry,
        max_new_tokens: int = DEFAULT_MAX_NEW_TOKENS,
        clock_ms: Callable[[], float] = _default_clock_ms,
    ) -> None:
        self.backend = backend
        self.registry = registry
        self.max_new_tokens = clamp_max_new_tokens(max_new_tokens)
        self._clock_ms = clock_ms
        self._gpu_semaphore = asyncio.Semaphore(1)
        self._queue_depth = 0

    @property
    def queue_depth(self) -> int:
        return self._queue_depth

    def startup(self) -> None:
        if not self.backend.ready:
            self.backend.load()

    def health(self) -> Dict[str, object]:
        vram = self.backend.get_vram_stats()
        return {
            "status": "ok" if self.backend.ready else "loading",
            "ready": self.backend.ready,
            "backend": self.backend.backend_name,
            "model": self.backend.model_id,
            "queue_depth": self._queue_depth,
            "vram_allocated_mib": vram.vram_allocated_mib,
            "vram_reserved_mib": vram.vram_reserved_mib,
            "vram_limit_fraction": vram.vram_limit_fraction,
            "personas_cached": self.backend.personas_cached,
            "max_new_tokens": self.max_new_tokens,
            "sample_rate": self.backend.sample_rate,
        }

    def _validate_request(self, req: SynthesizeRequest) -> tuple[SynthesisVoicePlan, str, str]:
        if not self.backend.ready:
            raise ServiceNotReadyError()

        text = req.text.strip()
        if not text:
            raise InvalidRequestError("text must not be empty")

        fmt = (req.format or "wav").strip().lower()
        if fmt not in SUPPORTED_FORMATS:
            raise InvalidRequestError(
                f"Unsupported format '{req.format}'; expected one of {list(SUPPORTED_FORMATS)}"
            )

        try:
            plan = self.registry.resolve(
                persona_id=req.persona_id,
                mood=req.mood,
                instruct_override=req.instruct_override,
            )
        except PersonaValidationError as exc:
            raise InvalidRequestError(str(exc)) from exc

        # Guardrail #5a: discard already-expired requests BEFORE entering the queue.
        if req.deadline_ms <= 0:
            raise DeadlineExceededError(
                request_id=req.request_id,
                deadline_ms=req.deadline_ms,
                queue_wait_ms=0,
            )

        return plan, text, fmt

    async def _run_serialized_gpu_job(
        self,
        req: SynthesizeRequest,
        worker: Callable[[SynthesisVoicePlan, str], _T],
    ) -> Tuple[_T, str, int, int]:
        plan, text, fmt = self._validate_request(req)
        enqueued_at = self._clock_ms()

        self._queue_depth += 1
        try:
            async with self._gpu_semaphore:
                acquired_at = self._clock_ms()
                queue_wait_ms = max(0, int(round(acquired_at - enqueued_at)))

                # Guardrail #5b: discard requests whose queue wait already exceeded deadline_ms
                # BEFORE invoking GPU synthesis.
                if queue_wait_ms >= req.deadline_ms:
                    raise DeadlineExceededError(
                        request_id=req.request_id,
                        deadline_ms=req.deadline_ms,
                        queue_wait_ms=queue_wait_ms,
                    )

                synth_start = self._clock_ms()
                output = await asyncio.to_thread(worker, plan, text)
                synth_ms = max(0, int(round(self._clock_ms() - synth_start)))
        finally:
            self._queue_depth -= 1

        return output, fmt, queue_wait_ms, synth_ms

    def _build_audio_headers(
        self,
        *,
        request_id: str,
        duration_ms: int,
        synth_ms: int,
        queue_wait_ms: int,
        sample_rate: int,
        audio_format: Optional[str] = None,
    ) -> Dict[str, str]:
        headers = {
            "X-TTS-Request-Id": request_id,
            "X-TTS-Duration-Ms": str(duration_ms),
            "X-TTS-Synth-Ms": str(synth_ms),
            "X-TTS-Queue-Wait-Ms": str(queue_wait_ms),
            "X-TTS-Backend": self.backend.backend_name,
            "X-TTS-Model": self.backend.model_id,
            "X-Audio-Sample-Rate": str(sample_rate),
            "X-Audio-Channels": str(DEFAULT_CHANNELS),
        }
        if audio_format is not None:
            headers["X-Audio-Format"] = audio_format
        return headers

    async def synthesize(self, req: SynthesizeRequest) -> SynthesizeResponse:
        raw_audio, fmt, queue_wait_ms, synth_ms = await self._run_serialized_gpu_job(
            req,
            lambda plan, text: self.backend.synthesize(
                plan,
                text,
                max_new_tokens=self.max_new_tokens,
            ),
        )

        duration_ms = compute_duration_ms(
            len(raw_audio.pcm16_bytes),
            sample_rate=raw_audio.sample_rate,
            channels=DEFAULT_CHANNELS,
        )
        headers = self._build_audio_headers(
            request_id=req.request_id,
            duration_ms=duration_ms,
            synth_ms=synth_ms,
            queue_wait_ms=queue_wait_ms,
            sample_rate=raw_audio.sample_rate,
            audio_format="pcm_s16le" if fmt == "pcm" else None,
        )

        if fmt == "pcm":
            return SynthesizeResponse(
                body=raw_audio.pcm16_bytes,
                content_type="application/octet-stream",
                headers=headers,
            )

        wav_bytes = encode_wav_pcm16(
            raw_audio.pcm16_bytes,
            sample_rate=raw_audio.sample_rate,
            channels=DEFAULT_CHANNELS,
        )
        return SynthesizeResponse(
            body=wav_bytes,
            content_type="audio/wav",
            headers=headers,
        )

    async def synthesize_stream(self, req: SynthesizeRequest) -> SynthesizeStreamResponse:
        """v1.5 streaming extension: yields raw 24 kHz 16-bit mono PCM chunks."""
        chunks, _, queue_wait_ms, synth_ms = await self._run_serialized_gpu_job(
            req,
            lambda plan, text: list(
                self.backend.synthesize_stream(
                    plan,
                    text,
                    max_new_tokens=self.max_new_tokens,
                )
            ),
        )

        total_bytes = sum(len(chunk) for chunk in chunks)
        duration_ms = compute_duration_ms(
            total_bytes,
            sample_rate=self.backend.sample_rate,
            channels=DEFAULT_CHANNELS,
        )
        headers = self._build_audio_headers(
            request_id=req.request_id,
            duration_ms=duration_ms,
            synth_ms=synth_ms,
            queue_wait_ms=queue_wait_ms,
            sample_rate=self.backend.sample_rate,
            audio_format="pcm_s16le",
        )
        return SynthesizeStreamResponse(
            chunks=chunks,
            content_type="application/octet-stream",
            headers=headers,
        )

    async def synthesize_openai_speech(
        self,
        *,
        voice: str,
        input_text: str,
        instructions: Optional[str] = None,
        response_format: str = "wav",
        mood: str = "Deadpan",
        deadline_ms: int = DEFAULT_DEADLINE_MS,
    ) -> SynthesizeResponse:
        """OpenAI /v1/audio/speech compatibility helper for quick ops probing."""
        try:
            plan = self.registry.resolve_openai_voice(
                voice,
                mood=mood,
                instruct_override=instructions,
            )
        except PersonaValidationError as exc:
            raise InvalidRequestError(str(exc)) from exc

        req = SynthesizeRequest(
            request_id=f"openai-{int(self._clock_ms())}",
            persona_id=plan.persona_id,
            mood=plan.mood,
            text=input_text,
            instruct_override=instructions,
            format=response_format,
            stream=False,
            deadline_ms=deadline_ms,
        )
        return await self.synthesize(req)
