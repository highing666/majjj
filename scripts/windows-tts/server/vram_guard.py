"""VRAM guardrails for co-residing LAN TTS with llama-server on RTX 3060 12GB."""

from __future__ import annotations

from dataclasses import dataclass
import os
from typing import Any, Mapping, MutableMapping, Optional

# Guardrail #2: 12,288 MiB * 0.30 = ~3,686 MiB hard per-process CUDA allocator cap.
DEFAULT_VRAM_FRACTION = 0.30
MAX_SAFE_VRAM_FRACTION = 0.45

# Guardrail #3: Windows WDDM-safe CUDA allocator config (never use expandable_segments).
DEFAULT_CUDA_ALLOC_CONF = "max_split_size_mb:128,garbage_collection_threshold:0.8"

# Guardrail #4: 12.5 Hz codec => 120 frames ~= 9.6 seconds hard cap for 10-26 char banter lines.
DEFAULT_MAX_NEW_TOKENS = 120

# Streaming codec chunk size: 6 frames ~= 500 ms @ 12 Hz codec (tunable via MAJJJ_TTS_STREAM_CHUNK_SIZE).
DEFAULT_STREAM_CHUNK_SIZE = 6
MAX_STREAM_CHUNK_SIZE = DEFAULT_MAX_NEW_TOKENS


class VramGuardrailError(ValueError):
    """Raised when a VRAM safety guardrail configuration is violated."""


@dataclass(frozen=True)
class VramStats:
    vram_allocated_mib: int
    vram_reserved_mib: int
    vram_limit_fraction: float


def validate_cuda_alloc_conf(raw_value: Optional[str]) -> str:
    """Validate and normalize PYTORCH_CUDA_ALLOC_CONF for Windows WDDM safety."""
    value = (raw_value or DEFAULT_CUDA_ALLOC_CONF).strip()
    if not value:
        value = DEFAULT_CUDA_ALLOC_CONF
    lowered = value.lower()
    if "expandable_segments" in lowered:
        raise VramGuardrailError(
            "PYTORCH_CUDA_ALLOC_CONF must not enable 'expandable_segments' on Windows WDDM; "
            f"use '{DEFAULT_CUDA_ALLOC_CONF}'."
        )
    return value


def configure_cuda_alloc_env(
    env: Optional[MutableMapping[str, str]] = None,
    *,
    alloc_conf: Optional[str] = None,
) -> str:
    """Set validated PYTORCH_CUDA_ALLOC_CONF before importing torch."""
    target_env = os.environ if env is None else env
    raw = alloc_conf if alloc_conf is not None else target_env.get("PYTORCH_CUDA_ALLOC_CONF")
    validated = validate_cuda_alloc_conf(raw)
    target_env["PYTORCH_CUDA_ALLOC_CONF"] = validated
    return validated


def validate_vram_fraction(fraction: float) -> float:
    """Ensure the per-process VRAM fraction keeps llama-server's ~7.24 GiB isolated."""
    if not (0.05 <= fraction <= MAX_SAFE_VRAM_FRACTION):
        raise VramGuardrailError(
            f"vram_fraction={fraction} is outside safe co-residency range [0.05, {MAX_SAFE_VRAM_FRACTION}]"
        )
    return float(fraction)


def apply_vram_fraction_guard(
    torch_module: Any,
    *,
    fraction: float = DEFAULT_VRAM_FRACTION,
    device: int = 0,
) -> bool:
    """Apply torch.cuda.set_per_process_memory_fraction before model weights load."""
    safe_fraction = validate_vram_fraction(fraction)
    if torch_module is None:
        return False
    cuda = getattr(torch_module, "cuda", None)
    if cuda is None or not cuda.is_available():
        return False
    cuda.set_per_process_memory_fraction(safe_fraction, device=device)
    return True


def clamp_max_new_tokens(requested: int, *, hard_cap: int = DEFAULT_MAX_NEW_TOKENS) -> int:
    """Clamp requested token/frame budget to the hard cap to prevent runaway KV cache growth."""
    if requested <= 0:
        raise ValueError("max_new_tokens must be positive")
    if hard_cap <= 0:
        raise ValueError("hard_cap must be positive")
    return min(int(requested), int(hard_cap))


def validate_stream_chunk_size(
    raw_value: Any,
    *,
    max_chunk_size: int = MAX_STREAM_CHUNK_SIZE,
) -> int:
    """Validate MAJJJ_TTS_STREAM_CHUNK_SIZE as a positive integer within [1, max_chunk_size]."""
    if isinstance(raw_value, bool):
        raise ValueError("stream_chunk_size must be a positive integer, not bool")
    if isinstance(raw_value, int):
        parsed = raw_value
    elif isinstance(raw_value, str):
        stripped = raw_value.strip()
        if not stripped:
            raise ValueError("stream_chunk_size must not be empty")
        try:
            parsed = int(stripped)
        except ValueError as exc:
            raise ValueError(f"stream_chunk_size must be an integer, got '{raw_value}'") from exc
    else:
        raise ValueError(
            f"stream_chunk_size must be a positive integer, got {type(raw_value).__name__}"
        )

    if parsed <= 0 or parsed > max_chunk_size:
        raise ValueError(
            f"stream_chunk_size={parsed} must be within [1, {max_chunk_size}]"
        )
    return parsed


def collect_vram_stats(
    torch_module: Any,
    *,
    vram_limit_fraction: float = DEFAULT_VRAM_FRACTION,
    device: int = 0,
) -> VramStats:
    """Read current PyTorch CUDA allocator usage in MiB (zeros when CPU/offline)."""
    if torch_module is None:
        return VramStats(0, 0, float(vram_limit_fraction))
    cuda = getattr(torch_module, "cuda", None)
    if cuda is None or not cuda.is_available():
        return VramStats(0, 0, float(vram_limit_fraction))
    allocated_bytes = int(cuda.memory_allocated(device))
    reserved_bytes = int(cuda.memory_reserved(device))
    mib = 1024 * 1024
    return VramStats(
        vram_allocated_mib=allocated_bytes // mib,
        vram_reserved_mib=reserved_bytes // mib,
        vram_limit_fraction=float(vram_limit_fraction),
    )


def verify_env_guardrails(env: Mapping[str, str]) -> str:
    """Check runtime environment variables for WDDM-safe CUDA allocator config."""
    return validate_cuda_alloc_conf(env.get("PYTORCH_CUDA_ALLOC_CONF"))
