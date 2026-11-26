"""Pure-stdlib PCM16 & 24 kHz mono WAV encoding helpers for BanterVoice."""

from __future__ import annotations

import io
import struct
import wave
from typing import Iterable, Sequence

DEFAULT_SAMPLE_RATE = 24000
DEFAULT_CHANNELS = 1
DEFAULT_SAMPLE_WIDTH_BYTES = 2  # 16-bit signed little-endian PCM


def float_samples_to_pcm16_bytes(samples: Iterable[float]) -> bytes:
    """Convert normalized float samples in [-1.0, 1.0] to signed 16-bit LE PCM bytes."""
    clamped = []
    for sample in samples:
        value = float(sample)
        if value > 1.0:
            value = 1.0
        elif value < -1.0:
            value = -1.0
        clamped.append(int(round(value * 32767.0)))
    if not clamped:
        return b""
    return struct.pack(f"<{len(clamped)}h", *clamped)


def coerce_audio_to_pcm16_bytes(audio: object) -> bytes:
    """Accept raw PCM16 bytes or numeric sample iterables/arrays and return PCM16 bytes."""
    if isinstance(audio, (bytes, bytearray, memoryview)):
        raw = bytes(audio)
        if len(raw) % DEFAULT_SAMPLE_WIDTH_BYTES != 0:
            raise ValueError("PCM16 byte buffer length must be a multiple of 2 bytes")
        return raw

    # Support numpy arrays / torch tensors without importing them at module level.
    if hasattr(audio, "detach"):
        audio = audio.detach()
    if hasattr(audio, "cpu"):
        audio = audio.cpu()
    if hasattr(audio, "numpy"):
        audio = audio.numpy()
    if hasattr(audio, "squeeze"):
        audio = audio.squeeze()
    if hasattr(audio, "tolist"):
        values = audio.tolist()
        if isinstance(values, Sequence) and values and isinstance(values[0], Sequence):
            values = values[0]
        return float_samples_to_pcm16_bytes(values)

    if isinstance(audio, Iterable):
        return float_samples_to_pcm16_bytes(audio)

    raise TypeError(f"Unsupported audio payload type: {type(audio).__name__}")


def compute_duration_ms(
    pcm16_byte_length: int,
    *,
    sample_rate: int = DEFAULT_SAMPLE_RATE,
    channels: int = DEFAULT_CHANNELS,
) -> int:
    """Return audio duration in integer milliseconds for signed 16-bit PCM bytes."""
    if sample_rate <= 0:
        raise ValueError("sample_rate must be positive")
    if channels <= 0:
        raise ValueError("channels must be positive")
    bytes_per_frame = DEFAULT_SAMPLE_WIDTH_BYTES * channels
    if pcm16_byte_length < 0 or pcm16_byte_length % bytes_per_frame != 0:
        raise ValueError(
            f"pcm16_byte_length ({pcm16_byte_length}) must be a non-negative multiple of {bytes_per_frame}"
        )
    frames = pcm16_byte_length // bytes_per_frame
    return int(round((frames * 1000.0) / float(sample_rate)))


def encode_wav_pcm16(
    pcm16_bytes: bytes,
    *,
    sample_rate: int = DEFAULT_SAMPLE_RATE,
    channels: int = DEFAULT_CHANNELS,
) -> bytes:
    """Wrap raw signed 16-bit LE PCM bytes in a standard RIFF/WAVE container."""
    bytes_per_frame = DEFAULT_SAMPLE_WIDTH_BYTES * channels
    if len(pcm16_bytes) % bytes_per_frame != 0:
        raise ValueError(
            f"PCM16 byte length ({len(pcm16_bytes)}) must be aligned to {bytes_per_frame}-byte frames"
        )
    buf = io.BytesIO()
    with wave.open(buf, "wb") as wf:
        wf.setnchannels(channels)
        wf.setsampwidth(DEFAULT_SAMPLE_WIDTH_BYTES)
        wf.setframerate(sample_rate)
        wf.writeframes(pcm16_bytes)
    return buf.getvalue()
