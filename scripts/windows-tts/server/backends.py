"""Pluggable Windows LAN emotional TTS backend adapters.

Supports switching between:
  1. Primary:  qwen3-custom-voice (Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice, BF16 + faster-qwen3-tts + SDPA)
  2. RunnerUp: cosyvoice3         (FunAudioLLM/Fun-CosyVoice3-0.5B-2512, FP16 + cached 5-persona embeddings)
  3. Fallback: qwen3-base-clone   (Qwen/Qwen3-TTS-12Hz-0.6B-Base, 5x6 pre-cached voice clone prompt matrix)
without changing any HTTP contract, mahjong-server, or frontend code.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass
import pathlib
from typing import Any, Callable, Dict, Iterator, Optional, Sequence, Tuple

from audio import DEFAULT_SAMPLE_RATE, coerce_audio_to_pcm16_bytes
from personas import CANONICAL_MOODS, PersonaRegistry, SynthesisVoicePlan
from vram_guard import (
    DEFAULT_MAX_NEW_TOKENS,
    DEFAULT_VRAM_FRACTION,
    VramStats,
    apply_vram_fraction_guard,
    clamp_max_new_tokens,
    collect_vram_stats,
)

BACKEND_QWEN3_CUSTOM_VOICE = "qwen3-custom-voice"
BACKEND_COSYVOICE3 = "cosyvoice3"
BACKEND_QWEN3_BASE_CLONE = "qwen3-base-clone"

DEFAULT_MODEL_PATHS: Dict[str, str] = {
    BACKEND_QWEN3_CUSTOM_VOICE: "Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
    BACKEND_COSYVOICE3: "FunAudioLLM/Fun-CosyVoice3-0.5B-2512",
    BACKEND_QWEN3_BASE_CLONE: "Qwen/Qwen3-TTS-12Hz-0.6B-Base",
}

SUPPORTED_BACKENDS = tuple(DEFAULT_MODEL_PATHS.keys())


@dataclass(frozen=True)
class SynthesisRawAudio:
    pcm16_bytes: bytes
    sample_rate: int = DEFAULT_SAMPLE_RATE


class TtsBackend(ABC):
    """Abstract interface implemented by all pluggable TTS inference backends."""

    backend_name: str
    model_id: str
    sample_rate: int = DEFAULT_SAMPLE_RATE

    def __init__(
        self,
        *,
        model_path: str,
        registry: PersonaRegistry,
        vram_fraction: float = DEFAULT_VRAM_FRACTION,
        max_new_tokens: int = DEFAULT_MAX_NEW_TOKENS,
        torch_module: Any = None,
    ) -> None:
        self.model_path = model_path
        self.model_id = pathlib.Path(model_path).name or model_path
        self.registry = registry
        self.vram_fraction = float(vram_fraction)
        self.max_new_tokens = clamp_max_new_tokens(max_new_tokens)
        self._torch = torch_module
        self._ready = False

    @property
    def ready(self) -> bool:
        return self._ready

    @property
    def personas_cached(self) -> int:
        return self.registry.persona_count if self._ready else 0

    def _resolve_torch(self) -> Any:
        if self._torch is None:
            import torch  # type: ignore[import-not-found]

            self._torch = torch
        return self._torch

    def get_vram_stats(self) -> VramStats:
        return collect_vram_stats(self._torch, vram_limit_fraction=self.vram_fraction)

    @abstractmethod
    def load(self) -> None:
        """Apply VRAM guardrails and load model weights into GPU memory."""

    @abstractmethod
    def synthesize(
        self,
        plan: SynthesisVoicePlan,
        text: str,
        *,
        max_new_tokens: int = DEFAULT_MAX_NEW_TOKENS,
    ) -> SynthesisRawAudio:
        """Synthesize a single short banter utterance into 24 kHz mono PCM16 bytes."""

    def synthesize_stream(
        self,
        plan: SynthesisVoicePlan,
        text: str,
        *,
        max_new_tokens: int = DEFAULT_MAX_NEW_TOKENS,
        chunk_bytes: int = 9600,  # 200 ms @ 24 kHz 16-bit mono
    ) -> Iterator[bytes]:
        """Yield raw 16-bit LE PCM chunks (v1.5 streaming extension hook)."""
        audio = self.synthesize(plan, text, max_new_tokens=max_new_tokens)
        data = audio.pcm16_bytes
        step = max(2, chunk_bytes - (chunk_bytes % 2))
        for offset in range(0, len(data), step):
            yield data[offset : offset + step]


def _default_load_qwen3_model(model_path: str, **kwargs: Any) -> Any:
    try:
        from faster_qwen3_tts import FasterQwen3TTS  # type: ignore[import-not-found]

        return FasterQwen3TTS.from_pretrained(model_path, **kwargs)
    except ImportError:
        from qwen_tts import Qwen3TTSModel  # type: ignore[import-not-found]

        return Qwen3TTSModel.from_pretrained(model_path, **kwargs)


def _resolve_reference_clip(
    voices_dir: pathlib.Path,
    candidate_stems: Sequence[str],
    fallback_text: str,
) -> Tuple[pathlib.Path, str]:
    """Resolve the first existing `<stem>.wav` and matching `<stem>.txt` in priority order."""
    chosen_stem = candidate_stems[0]
    for stem in candidate_stems:
        if (voices_dir / f"{stem}.wav").is_file():
            chosen_stem = stem
            break
    wav_path = voices_dir / f"{chosen_stem}.wav"
    txt_path = voices_dir / f"{chosen_stem}.txt"
    ref_text = (
        txt_path.read_text(encoding="utf-8").strip()
        if txt_path.is_file()
        else fallback_text
    )
    return wav_path, ref_text


class Qwen3CustomVoiceBackend(TtsBackend):
    """Primary backend: Qwen3-TTS-12Hz-0.6B-CustomVoice (BF16 + faster-qwen3-tts + SDPA)."""

    backend_name = BACKEND_QWEN3_CUSTOM_VOICE

    def __init__(
        self,
        *,
        model_path: str,
        registry: PersonaRegistry,
        vram_fraction: float = DEFAULT_VRAM_FRACTION,
        max_new_tokens: int = DEFAULT_MAX_NEW_TOKENS,
        torch_module: Any = None,
        model_loader: Optional[Callable[..., Any]] = None,
    ) -> None:
        super().__init__(
            model_path=model_path,
            registry=registry,
            vram_fraction=vram_fraction,
            max_new_tokens=max_new_tokens,
            torch_module=torch_module,
        )
        self._model_loader = model_loader
        self._model: Any = None

    def load(self) -> None:
        torch_mod = self._resolve_torch()
        apply_vram_fraction_guard(torch_mod, fraction=self.vram_fraction, device=0)
        loader = self._model_loader or _default_load_qwen3_model
        # Windows RTX 3060 (Ampere SM 8.6): native PyTorch SDPA + BF16, zero flash-attn build.
        self._model = loader(
            self.model_path,
            device="cuda:0" if torch_mod.cuda.is_available() else "cpu",
            dtype=torch_mod.bfloat16,
            attn_implementation="sdpa",
        )
        self._ready = True

    def synthesize(
        self,
        plan: SynthesisVoicePlan,
        text: str,
        *,
        max_new_tokens: int = DEFAULT_MAX_NEW_TOKENS,
    ) -> SynthesisRawAudio:
        if not self._ready or self._model is None:
            raise RuntimeError("Qwen3CustomVoiceBackend is not loaded")
        capped_tokens = clamp_max_new_tokens(max_new_tokens, hard_cap=self.max_new_tokens)
        wavs, sr = self._model.generate_custom_voice(
            text=text,
            language=plan.language,
            speaker=plan.speaker,
            instruct=plan.instruct,
            max_new_tokens=capped_tokens,
        )
        waveform = wavs[0] if isinstance(wavs, (list, tuple)) and wavs else wavs
        pcm16 = coerce_audio_to_pcm16_bytes(waveform)
        return SynthesisRawAudio(pcm16_bytes=pcm16, sample_rate=int(sr or self.sample_rate))


class CosyVoice3Backend(TtsBackend):
    """Runner-up backend: Fun-CosyVoice3-0.5B-2512 (FP16 + cached 5-persona embeddings)."""

    backend_name = BACKEND_COSYVOICE3

    def __init__(
        self,
        *,
        model_path: str,
        registry: PersonaRegistry,
        voices_dir: Optional[pathlib.Path | str] = None,
        vram_fraction: float = DEFAULT_VRAM_FRACTION,
        max_new_tokens: int = DEFAULT_MAX_NEW_TOKENS,
        torch_module: Any = None,
        model_loader: Optional[Callable[..., Any]] = None,
    ) -> None:
        super().__init__(
            model_path=model_path,
            registry=registry,
            vram_fraction=vram_fraction,
            max_new_tokens=max_new_tokens,
            torch_module=torch_module,
        )
        self.voices_dir = pathlib.Path(voices_dir) if voices_dir else pathlib.Path("voices")
        self._model_loader = model_loader
        self._model: Any = None
        self._speaker_cache: Dict[str, Any] = {}

    @property
    def personas_cached(self) -> int:
        return len(self._speaker_cache)

    def _default_load_model(self, model_path: str, **kwargs: Any) -> Any:
        from cosyvoice.cli.cosyvoice import AutoModel  # type: ignore[import-not-found]

        return AutoModel(model_dir=model_path, **kwargs)

    def _resolve_ref_clip(self, voice_pack_id: str) -> Tuple[str, str]:
        ref_wav, ref_text = _resolve_reference_clip(
            self.voices_dir,
            (f"{voice_pack_id}_neutral", f"{voice_pack_id}_ref"),
            fallback_text="",
        )
        return str(ref_wav), ref_text

    def load(self) -> None:
        torch_mod = self._resolve_torch()
        apply_vram_fraction_guard(torch_mod, fraction=self.vram_fraction, device=0)
        loader = self._model_loader or self._default_load_model
        # Windows native: FP16 without Linux-only vLLM.
        self._model = loader(self.model_path, fp16=True, load_vllm=False)

        # Pre-extract 5 persona embeddings in memory so inference_instruct2 never hits
        # upstream zero_shot_spk_id prompt_text overwrite issues.
        self._speaker_cache.clear()
        for profile in self.registry.list_personas():
            ref_path, ref_text = self._resolve_ref_clip(profile.voice_pack_id)
            if hasattr(self._model, "extract_speaker_features"):
                self._speaker_cache[profile.persona_id] = self._model.extract_speaker_features(
                    ref_path
                )
            else:
                self._speaker_cache[profile.persona_id] = {
                    "ref_audio_path": ref_path,
                    "ref_text": ref_text,
                }
        self._ready = True

    def synthesize(
        self,
        plan: SynthesisVoicePlan,
        text: str,
        *,
        max_new_tokens: int = DEFAULT_MAX_NEW_TOKENS,
    ) -> SynthesisRawAudio:
        if not self._ready or self._model is None:
            raise RuntimeError("CosyVoice3Backend is not loaded")
        _ = clamp_max_new_tokens(max_new_tokens, hard_cap=self.max_new_tokens)
        spk_features = self._speaker_cache[plan.persona_id]
        if hasattr(self._model, "synthesize_with_cached_features"):
            waveform, sr = self._model.synthesize_with_cached_features(
                text=text,
                instruct_prompt=plan.cosyvoice_instruct,
                spk_features=spk_features,
            )
            return SynthesisRawAudio(
                pcm16_bytes=coerce_audio_to_pcm16_bytes(waveform),
                sample_rate=int(sr or getattr(self._model, "sample_rate", self.sample_rate)),
            )

        # Direct CosyVoice3 fallback path using explicit neutral reference WAV + <|endofprompt|>
        chunks = []
        for out in self._model.inference_instruct2(
            text,
            plan.cosyvoice_instruct,
            spk_features["ref_audio_path"],
            stream=False,
        ):
            chunks.append(coerce_audio_to_pcm16_bytes(out["tts_speech"]))
        return SynthesisRawAudio(
            pcm16_bytes=b"".join(chunks),
            sample_rate=int(getattr(self._model, "sample_rate", self.sample_rate)),
        )


class Qwen3BaseCloneBackend(TtsBackend):
    """Fallback backend: Qwen3-TTS-12Hz-0.6B-Base + pre-cached 5x6 reference prompt matrix."""

    backend_name = BACKEND_QWEN3_BASE_CLONE

    def __init__(
        self,
        *,
        model_path: str,
        registry: PersonaRegistry,
        voices_dir: Optional[pathlib.Path | str] = None,
        vram_fraction: float = DEFAULT_VRAM_FRACTION,
        max_new_tokens: int = DEFAULT_MAX_NEW_TOKENS,
        torch_module: Any = None,
        model_loader: Optional[Callable[..., Any]] = None,
    ) -> None:
        super().__init__(
            model_path=model_path,
            registry=registry,
            vram_fraction=vram_fraction,
            max_new_tokens=max_new_tokens,
            torch_module=torch_module,
        )
        self.voices_dir = pathlib.Path(voices_dir) if voices_dir else pathlib.Path("voices")
        self._model_loader = model_loader
        self._model: Any = None
        self._prompt_matrix: Dict[str, Any] = {}

    @property
    def personas_cached(self) -> int:
        return self.registry.persona_count if self._ready and self._prompt_matrix else 0

    def load(self) -> None:
        torch_mod = self._resolve_torch()
        apply_vram_fraction_guard(torch_mod, fraction=self.vram_fraction, device=0)
        loader = self._model_loader or _default_load_qwen3_model
        self._model = loader(
            self.model_path,
            device="cuda:0" if torch_mod.cuda.is_available() else "cpu",
            dtype=torch_mod.bfloat16,
            attn_implementation="sdpa",
        )
        self._prompt_matrix.clear()
        for profile in self.registry.list_personas():
            for mood in CANONICAL_MOODS:
                plan = self.registry.resolve(persona_id=profile.persona_id, mood=mood)
                ref_wav, ref_text = _resolve_reference_clip(
                    self.voices_dir,
                    (
                        plan.reference_key,
                        f"{profile.voice_pack_id}_neutral",
                        f"{profile.voice_pack_id}_ref",
                    ),
                    fallback_text=plan.instruct,
                )
                self._prompt_matrix[plan.reference_key] = self._model.create_voice_clone_prompt(
                    ref_audio=str(ref_wav),
                    ref_text=ref_text,
                    x_vector_only_mode=False,
                )
        self._ready = True

    def synthesize(
        self,
        plan: SynthesisVoicePlan,
        text: str,
        *,
        max_new_tokens: int = DEFAULT_MAX_NEW_TOKENS,
    ) -> SynthesisRawAudio:
        if not self._ready or self._model is None:
            raise RuntimeError("Qwen3BaseCloneBackend is not loaded")
        capped_tokens = clamp_max_new_tokens(max_new_tokens, hard_cap=self.max_new_tokens)
        cached_prompt = self._prompt_matrix[plan.reference_key]
        wavs, sr = self._model.generate_voice_clone(
            text=text,
            language=plan.language,
            voice_clone_prompt=cached_prompt,
            max_new_tokens=capped_tokens,
        )
        waveform = wavs[0] if isinstance(wavs, (list, tuple)) and wavs else wavs
        pcm16 = coerce_audio_to_pcm16_bytes(waveform)
        return SynthesisRawAudio(pcm16_bytes=pcm16, sample_rate=int(sr or self.sample_rate))


def create_backend(
    backend_name: str,
    *,
    model_path: Optional[str] = None,
    registry: PersonaRegistry,
    voices_dir: Optional[pathlib.Path | str] = None,
    vram_fraction: float = DEFAULT_VRAM_FRACTION,
    max_new_tokens: int = DEFAULT_MAX_NEW_TOKENS,
    torch_module: Any = None,
    model_loader: Optional[Callable[..., Any]] = None,
) -> TtsBackend:
    """Instantiate a pluggable TTS backend by canonical backend name."""
    normalized = backend_name.strip().lower()
    if normalized not in DEFAULT_MODEL_PATHS:
        raise ValueError(
            f"Unsupported TTS backend '{backend_name}'; expected one of {list(SUPPORTED_BACKENDS)}"
        )
    resolved_model_path = (
        model_path.strip()
        if model_path and model_path.strip()
        else DEFAULT_MODEL_PATHS[normalized]
    )

    if normalized == BACKEND_QWEN3_CUSTOM_VOICE:
        return Qwen3CustomVoiceBackend(
            model_path=resolved_model_path,
            registry=registry,
            vram_fraction=vram_fraction,
            max_new_tokens=max_new_tokens,
            torch_module=torch_module,
            model_loader=model_loader,
        )
    if normalized == BACKEND_COSYVOICE3:
        return CosyVoice3Backend(
            model_path=resolved_model_path,
            registry=registry,
            voices_dir=voices_dir,
            vram_fraction=vram_fraction,
            max_new_tokens=max_new_tokens,
            torch_module=torch_module,
            model_loader=model_loader,
        )
    return Qwen3BaseCloneBackend(
        model_path=resolved_model_path,
        registry=registry,
        voices_dir=voices_dir,
        vram_fraction=vram_fraction,
        max_new_tokens=max_new_tokens,
        torch_module=torch_module,
        model_loader=model_loader,
    )
