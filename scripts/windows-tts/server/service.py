"""Core serial-queue TTS service with pre-enqueue/pre-GPU deadline guardrails."""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
import threading
import time
from typing import AsyncIterator, Callable, Dict, Optional, Tuple, TypeVar

from audio import DEFAULT_CHANNELS, compute_duration_ms, encode_wav_pcm16
from backends import TtsBackend
from personas import PersonaRegistry, PersonaValidationError, SynthesisVoicePlan
from vram_guard import (
    DEFAULT_MAX_NEW_TOKENS,
    DEFAULT_STREAM_CHUNK_SIZE,
    clamp_max_new_tokens,
    validate_stream_chunk_size,
)

DEFAULT_DEADLINE_MS = 2500
SUPPORTED_FORMATS = ("wav", "pcm")

_T = TypeVar("_T")
_STREAM_END = object()
_STREAM_ERROR = object()


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


class _TtsStreamIterator(AsyncIterator[bytes]):
    """Async iterator bridging a background GPU synthesis worker via asyncio.Queue."""

    def __init__(
        self,
        *,
        queue: asyncio.Queue[object],
        cancel_event: threading.Event,
        done_event: asyncio.Event,
    ) -> None:
        self._queue = queue
        self._cancel_event = cancel_event
        self._done_event = done_event
        self._closed = False

    def __aiter__(self) -> _TtsStreamIterator:
        return self

    async def __anext__(self) -> bytes:
        if self._closed:
            raise StopAsyncIteration
        try:
            item = await self._queue.get()
        except BaseException:
            self._cancel_event.set()
            self._closed = True
            raise

        if item is _STREAM_END:
            self._closed = True
            raise StopAsyncIteration

        if isinstance(item, tuple) and len(item) == 2 and item[0] is _STREAM_ERROR:
            self._closed = True
            exc = item[1]
            if isinstance(exc, BaseException):
                raise exc
            raise RuntimeError(str(exc))

        return item  # type: ignore[return-value]

    async def aclose(self) -> None:
        self._closed = True
        self._cancel_event.set()
        await self._done_event.wait()


@dataclass(frozen=True)
class SynthesizeStreamResponse:
    chunks: _TtsStreamIterator
    content_type: str
    headers: Dict[str, str] = field(default_factory=dict)
    cancel: Callable[[], None] = field(default=lambda: None)


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
        stream_chunk_size: int = DEFAULT_STREAM_CHUNK_SIZE,
        clock_ms: Callable[[], float] = _default_clock_ms,
    ) -> None:
        self.backend = backend
        self.registry = registry
        self.max_new_tokens = clamp_max_new_tokens(max_new_tokens)
        self.stream_chunk_size = validate_stream_chunk_size(stream_chunk_size)
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
            "stream_chunk_size": self.stream_chunk_size,
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

    def _build_stream_headers(
        self,
        *,
        request_id: str,
        queue_wait_ms: int,
        sample_rate: int,
    ) -> Dict[str, str]:
        return {
            "X-TTS-Request-Id": request_id,
            "X-TTS-Queue-Wait-Ms": str(queue_wait_ms),
            "X-TTS-Backend": self.backend.backend_name,
            "X-TTS-Model": self.backend.model_id,
            "X-Audio-Format": "pcm_s16le",
            "X-Audio-Sample-Rate": str(sample_rate),
            "X-Audio-Channels": str(DEFAULT_CHANNELS),
        }

    async def synthesize_stream(self, req: SynthesizeRequest) -> SynthesizeStreamResponse:
        """True incremental async streaming: yields 24 kHz 16-bit mono Raw PCM chunks."""
        plan, text, _fmt = self._validate_request(req)
        enqueued_at = self._clock_ms()

        self._queue_depth += 1
        acquired = False
        try:
            await self._gpu_semaphore.acquire()
            acquired = True
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
        except BaseException:
            if acquired:
                self._gpu_semaphore.release()
            self._queue_depth -= 1
            raise

        loop = asyncio.get_running_loop()
        queue: asyncio.Queue[object] = asyncio.Queue()
        cancel_event = threading.Event()
        done_event = asyncio.Event()
        released = False

        def _finish_stream(error: Optional[BaseException]) -> None:
            nonlocal released
            if not released:
                released = True
                self._gpu_semaphore.release()
                self._queue_depth -= 1
            if error is not None:
                queue.put_nowait((_STREAM_ERROR, error))
            else:
                queue.put_nowait(_STREAM_END)
            done_event.set()

        def _safe_schedule(fn: Callable[..., object], *args: object) -> None:
            try:
                loop.call_soon_threadsafe(fn, *args)
            except RuntimeError:
                pass

        def _worker() -> None:
            worker_error: Optional[BaseException] = None
            pending_odd_byte = b""
            stream_iter = None
            try:
                stream_iter = self.backend.synthesize_stream(
                    plan,
                    text,
                    max_new_tokens=self.max_new_tokens,
                    chunk_size=self.stream_chunk_size,
                    cancel_event=cancel_event,
                )
                for chunk in stream_iter:
                    if cancel_event.is_set():
                        break
                    raw_chunk = bytes(chunk)
                    if not raw_chunk:
                        continue
                    if pending_odd_byte:
                        raw_chunk = pending_odd_byte + raw_chunk
                        pending_odd_byte = b""
                    if len(raw_chunk) % 2 != 0:
                        pending_odd_byte = raw_chunk[-1:]
                        raw_chunk = raw_chunk[:-1]
                    if raw_chunk:
                        _safe_schedule(queue.put_nowait, raw_chunk)
                    if cancel_event.is_set():
                        break
            except Exception as exc:
                if not cancel_event.is_set():
                    worker_error = exc
            finally:
                close_fn = getattr(stream_iter, "close", None)
                if callable(close_fn):
                    try:
                        close_fn()
                    except Exception:
                        pass
                _safe_schedule(_finish_stream, worker_error)

        worker_thread = threading.Thread(
            target=_worker,
            name=f"tts-stream-{req.request_id}",
            daemon=True,
        )
        try:
            worker_thread.start()
        except BaseException:
            _finish_stream(None)
            raise

        headers = self._build_stream_headers(
            request_id=req.request_id,
            queue_wait_ms=queue_wait_ms,
            sample_rate=self.backend.sample_rate,
        )
        iterator = _TtsStreamIterator(
            queue=queue,
            cancel_event=cancel_event,
            done_event=done_event,
        )
        return SynthesizeStreamResponse(
            chunks=iterator,
            content_type="application/octet-stream",
            headers=headers,
            cancel=cancel_event.set,
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
