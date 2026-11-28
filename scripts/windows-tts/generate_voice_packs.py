#!/usr/bin/env python3
"""Offline 7-pack VoicePack (角色语音包) batch generator & AudioSprite packer.

Designed to run on the Windows GPU host (e.g., BENDER, RTX 3060 12GB) while
`llama-server` is paused so full VRAM is available for high-fidelity 1.7B models:
  - 5 AI VoicePacks   -> Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice
                         (Eric, Serena, Dylan, Vivian, Uncle_Fu)
  - 2 Human VoicePacks -> Qwen/Qwen3-TTS-12Hz-1.7B-VoiceDesign
                         (human_male, human_female)

Outputs per pack under `<output_dir>/<pack_id>/`:
  - `sprite.m4a`: mono AAC AudioSprite covering all 46 canonical slots
  - `manifest.json`: runtime manifest aligned with `web/src/lib/audio/voicePackSchema.ts`

Also exports clean 24 kHz mono reference clips (`<voices_dir>/<pack_id>_ref.wav`
and `<voices_dir>/<pack_id>_ref.txt`) for the 5 AI personas so online clone
fallback backends can reuse the exact same timbres.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
import argparse
from dataclasses import dataclass
import json
import math
import os
import pathlib
import shutil
import struct
import subprocess
import sys
import tempfile
from typing import Any, Callable, Dict, Iterable, List, Optional, Sequence, Tuple

THIS_DIR = pathlib.Path(__file__).resolve().parent
SERVER_DIR = THIS_DIR / "server"
REPO_ROOT = THIS_DIR.parent.parent
if str(SERVER_DIR) not in sys.path:
    sys.path.insert(0, str(SERVER_DIR))

from audio import DEFAULT_SAMPLE_RATE, coerce_audio_to_pcm16_bytes, encode_wav_pcm16  # noqa: E402

MODEL_17B_CUSTOM_VOICE = "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice"
MODEL_17B_VOICE_DESIGN = "Qwen/Qwen3-TTS-12Hz-1.7B-VoiceDesign"

DEFAULT_BITRATE_BPS = 24000
DEFAULT_TARGET_RMS_DBFS = -16.0
DEFAULT_PEAK_LIMIT_DBFS = -1.5
DEFAULT_CLIP_GAP_SEC = 0.08
DEFAULT_TAIL_PAD_SEC = 0.06
MAX_TOTAL_M4A_BYTES = 2 * 1024 * 1024  # 2.0 MiB single-binary budget ceiling

SLOT_MAX_NEW_TOKENS: Dict[str, int] = {
    "tile": 36,    # ~2.9s @ 12.5 Hz codec; prevents runaway open-ended babble
    "action": 40,  # ~3.2s
    "emote": 64,   # ~5.1s
}
SLOT_MAX_DURATION_SEC: Dict[str, float] = {
    "tile": 2.2,
    "action": 2.4,
    "emote": 3.5,
}
REFERENCE_MAX_NEW_TOKENS = 120
MAX_SLOT_RETRIES = 3


@dataclass(frozen=True)
class VoiceSlotSpec:
    slot_id: str
    category: str  # "tile" | "action" | "emote"
    text: str
    style_instruct: str


@dataclass(frozen=True)
class VoicePackSpec:
    pack_id: str
    kind: str  # "ai" | "human"
    display_name: str
    model_repo: str
    language: str
    speaker: Optional[str]
    base_instruct: str
    voice_design_instruct: str
    export_reference_clip: bool
    reference_text: str


TILE_STYLE_INSTRUCT = "清晰干脆、吐字利落、自然报牌，不要拖音。"
ACTION_STYLE_INSTRUCT = "响亮干脆、干劲十足、短促有力地喊出动作词。"
EMOTE_STYLE_INSTRUCT = "自然生动的中文口语牌桌互动语气，节奏紧凑不拖沓。"

CANONICAL_SLOTS: Tuple[VoiceSlotSpec, ...] = (
    # 34 standard tiles matching web/src/lib/audio/voicePackSchema.ts (0..33)
    VoiceSlotSpec("m1", "tile", "一万", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("m2", "tile", "二万", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("m3", "tile", "三万", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("m4", "tile", "四万", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("m5", "tile", "五万", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("m6", "tile", "六万", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("m7", "tile", "七万", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("m8", "tile", "八万", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("m9", "tile", "九万", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("s1", "tile", "一条", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("s2", "tile", "二条", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("s3", "tile", "三条", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("s4", "tile", "四条", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("s5", "tile", "五条", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("s6", "tile", "六条", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("s7", "tile", "七条", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("s8", "tile", "八条", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("s9", "tile", "九条", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("p1", "tile", "一筒", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("p2", "tile", "二筒", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("p3", "tile", "三筒", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("p4", "tile", "四筒", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("p5", "tile", "五筒", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("p6", "tile", "六筒", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("p7", "tile", "七筒", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("p8", "tile", "八筒", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("p9", "tile", "九筒", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("z1", "tile", "东风", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("z2", "tile", "南风", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("z3", "tile", "西风", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("z4", "tile", "北风", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("z5", "tile", "红中", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("z6", "tile", "发财", TILE_STYLE_INSTRUCT),
    VoiceSlotSpec("z7", "tile", "白板", TILE_STYLE_INSTRUCT),
    # 6 meld & win action calls
    VoiceSlotSpec("pong", "action", "碰！", ACTION_STYLE_INSTRUCT),
    VoiceSlotSpec("kong", "action", "杠！", ACTION_STYLE_INSTRUCT),
    VoiceSlotSpec("concealed_kong", "action", "暗杠！", ACTION_STYLE_INSTRUCT),
    VoiceSlotSpec("zimo", "action", "自摸！", ACTION_STYLE_INSTRUCT),
    VoiceSlotSpec("ron", "action", "胡了！", ACTION_STYLE_INSTRUCT),
    VoiceSlotSpec("rob_kong", "action", "抢杠胡！", ACTION_STYLE_INSTRUCT),
    # 6 human quick emotes (快捷互动)
    VoiceSlotSpec("emote_is_that_it", "emote", "就这？", EMOTE_STYLE_INSTRUCT),
    VoiceSlotSpec("emote_hurry_up", "emote", "搞快点", EMOTE_STYLE_INSTRUCT),
    VoiceSlotSpec("emote_spare_me", "emote", "别点我，求放过", EMOTE_STYLE_INSTRUCT),
    VoiceSlotSpec("emote_nice_play", "emote", "666，漂亮", EMOTE_STYLE_INSTRUCT),
    VoiceSlotSpec("emote_dealer_rigged", "emote", "什么破手气", EMOTE_STYLE_INSTRUCT),
    VoiceSlotSpec("emote_going_big", "emote", "这把我要胡大的", EMOTE_STYLE_INSTRUCT),
)

CANONICAL_PACK_SPECS: Tuple[VoicePackSpec, ...] = (
    VoicePackSpec(
        pack_id="quick_hothead",
        kind="ai",
        display_name="大川",
        model_repo=MODEL_17B_CUSTOM_VOICE,
        language="Chinese",
        speaker="Eric",
        base_instruct="活泼微哑川渝男声，语速偏快、利落有冲劲。",
        voice_design_instruct="",
        export_reference_clip=True,
        reference_text="抓紧出牌别磨蹭，三万红中我都看清楚了，这小局稳稳拿下。",
    ),
    VoicePackSpec(
        pack_id="tea_sarcasm",
        kind="ai",
        display_name="小晚",
        model_repo=MODEL_17B_CUSTOM_VOICE,
        language="Chinese",
        speaker="Serena",
        base_instruct="温婉年轻女声，语调轻柔清晰、带一点笑意。",
        voice_design_instruct="",
        export_reference_clip=True,
        reference_text="哎呀这手牌打得真有想法，我可不敢乱碰，慢慢看大家发挥吧。",
    ),
    VoicePackSpec(
        pack_id="stubborn_blamer",
        kind="ai",
        display_name="大鹏",
        model_repo=MODEL_17B_CUSTOM_VOICE,
        language="Chinese",
        speaker="Dylan",
        base_instruct="清亮京片子青年男声，语调爽朗带点贫嘴自信。",
        voice_design_instruct="",
        export_reference_clip=True,
        reference_text="这牌哪是我打错了，分明是牌墙针对我，下小局看我怎么扳回来。",
    ),
    VoicePackSpec(
        pack_id="chaos_instigator",
        kind="ai",
        display_name="小桃",
        model_repo=MODEL_17B_CUSTOM_VOICE,
        language="Chinese",
        speaker="Vivian",
        base_instruct="明亮带锋芒年轻女声，节奏轻快跳脱、清脆利落。",
        voice_design_instruct="",
        export_reference_clip=True,
        reference_text="哇这牌面可太热闹了，谁先出牌谁背锅，我就在旁边看戏鼓掌。",
    ),
    VoicePackSpec(
        pack_id="tilted_master",
        kind="ai",
        display_name="老陈",
        model_repo=MODEL_17B_CUSTOM_VOICE,
        language="Chinese",
        speaker="Uncle_Fu",
        base_instruct="沉稳低沉老成男声，中气十足、吐字厚重稳健。",
        voice_design_instruct="",
        export_reference_clip=True,
        reference_text="打麻将讲究章法和牌感，先看牌河再定进张，慌什么慌。",
    ),
    VoicePackSpec(
        pack_id="human_male",
        kind="human",
        display_name="清亮男声",
        model_repo=MODEL_17B_VOICE_DESIGN,
        language="Chinese",
        speaker=None,
        base_instruct="",
        voice_design_instruct=(
            "标准普通话清亮爽朗的二十多岁青年男声，音色干净阳光、吐字清晰自然，"
            "与川渝沙哑男声、京腔贫嘴男声和低沉老男声明显区分。"
        ),
        export_reference_clip=False,
        reference_text="",
    ),
    VoicePackSpec(
        pack_id="human_female",
        kind="human",
        display_name="利落女声",
        model_repo=MODEL_17B_VOICE_DESIGN,
        language="Chinese",
        speaker=None,
        base_instruct="",
        voice_design_instruct=(
            "标准普通话利落清脆的二十多岁青年女声，节奏干脆明快、自然大气，"
            "不嗲不尖锐，与温婉茶艺女声和跳脱女声明显区分。"
        ),
        export_reference_clip=False,
        reference_text="",
    ),
)

PACK_SPECS_BY_ID: Dict[str, VoicePackSpec] = {spec.pack_id: spec for spec in CANONICAL_PACK_SPECS}


@dataclass(frozen=True)
class GeneratedPackResult:
    pack_id: str
    manifest_path: pathlib.Path
    sprite_path: pathlib.Path
    sprite_bytes: int
    duration_sec: float
    slot_count: int
    reference_wav_path: Optional[pathlib.Path]


EncoderFn = Callable[[pathlib.Path, pathlib.Path, int], None]


class VoicePackSynthesizer(ABC):
    """Pluggable synthesizer seam for 1.7B offline generation and dry-run testing."""

    @abstractmethod
    def synthesize_slot(
        self, pack: VoicePackSpec, slot: VoiceSlotSpec
    ) -> Tuple[bytes, int]:
        """Return (pcm16_bytes, sample_rate) for one canonical slot."""

    def synthesize_reference(self, pack: VoicePackSpec) -> Tuple[bytes, int]:
        """Return (pcm16_bytes, sample_rate) for an AI persona reference clip."""
        ref_slot = VoiceSlotSpec(
            slot_id="__reference__",
            category="emote",
            text=pack.reference_text,
            style_instruct="自然平静、吐字清晰的标准参考朗读。",
        )
        return self.synthesize_slot(pack, ref_slot)

    def release(self) -> None:
        """Release any held GPU/model resources."""


class DryRunSynthesizer(VoicePackSynthesizer):
    """Deterministic pure-stdlib synthesizer for offline pipeline verification."""

    def __init__(self, sample_rate: int = DEFAULT_SAMPLE_RATE) -> None:
        self.sample_rate = sample_rate

    def _tone_pcm16(self, duration_sec: float, freq_hz: float, amplitude: float) -> bytes:
        lead = int(round(0.06 * self.sample_rate))
        body = max(1, int(round(duration_sec * self.sample_rate)))
        tail = int(round(0.06 * self.sample_rate))
        samples: List[int] = [0] * lead
        for i in range(body):
            t = i / self.sample_rate
            env = 1.0
            if i < 120:
                env = i / 120.0
            elif i > body - 120:
                env = max(0.0, (body - i) / 120.0)
            val = amplitude * env * (
                0.7 * math.sin(2.0 * math.pi * freq_hz * t)
                + 0.3 * math.sin(2.0 * math.pi * (freq_hz * 1.5) * t)
            )
            samples.append(int(round(max(-1.0, min(1.0, val)) * 32767.0)))
        samples.extend([0] * tail)
        return struct.pack(f"<{len(samples)}h", *samples)

    def synthesize_slot(
        self, pack: VoicePackSpec, slot: VoiceSlotSpec
    ) -> Tuple[bytes, int]:
        pack_idx = list(PACK_SPECS_BY_ID.keys()).index(pack.pack_id)
        base_freq = 210.0 + pack_idx * 45.0
        dur = 0.24 if slot.category == "tile" else (0.30 if slot.category == "action" else 0.42)
        freq = base_freq + (len(slot.text) * 18.0)
        return self._tone_pcm16(dur, freq, amplitude=0.35), self.sample_rate

    def synthesize_reference(self, pack: VoicePackSpec) -> Tuple[bytes, int]:
        pack_idx = list(PACK_SPECS_BY_ID.keys()).index(pack.pack_id)
        return self._tone_pcm16(0.60, 220.0 + pack_idx * 40.0, amplitude=0.35), self.sample_rate


class Qwen3Offline17BSynthesizer(VoicePackSynthesizer):
    """Production Windows GPU synthesizer using 1.7B CustomVoice + 1.7B VoiceDesign."""

    def __init__(
        self,
        *,
        custom_voice_model: str = MODEL_17B_CUSTOM_VOICE,
        voice_design_model: str = MODEL_17B_VOICE_DESIGN,
        torch_module: Any = None,
        model_loader: Optional[Callable[..., Any]] = None,
    ) -> None:
        self.custom_voice_model = custom_voice_model
        self.voice_design_model = voice_design_model
        self._torch = torch_module
        self._model_loader = model_loader
        self._active_repo: Optional[str] = None
        self._active_model: Any = None

    def _resolve_torch(self) -> Any:
        if self._torch is None:
            import torch  # type: ignore[import-not-found]

            self._torch = torch
        return self._torch

    def _default_load_model(self, repo: str, **kwargs: Any) -> Any:
        from qwen_tts import Qwen3TTSModel  # type: ignore[import-not-found]

        return Qwen3TTSModel.from_pretrained(repo, **kwargs)

    def _ensure_model(self, repo: str) -> Any:
        if self._active_model is not None and self._active_repo == repo:
            return self._active_model
        self.release()
        torch_mod = self._resolve_torch()
        loader = self._model_loader or self._default_load_model
        device = "cuda:0" if torch_mod.cuda.is_available() else "cpu"
        self._active_model = loader(
            repo,
            device_map=device,
            dtype=torch_mod.bfloat16,
            attn_implementation="sdpa",
        )
        self._active_repo = repo
        return self._active_model

    def release(self) -> None:
        if self._active_model is not None:
            self._active_model = None
            self._active_repo = None
            if self._torch is not None and self._torch.cuda.is_available():
                self._torch.cuda.empty_cache()

    def synthesize_slot(
        self, pack: VoicePackSpec, slot: VoiceSlotSpec
    ) -> Tuple[bytes, int]:
        max_tokens = (
            REFERENCE_MAX_NEW_TOKENS
            if slot.slot_id == "__reference__"
            else SLOT_MAX_NEW_TOKENS.get(slot.category, REFERENCE_MAX_NEW_TOKENS)
        )
        if pack.kind == "ai":
            model = self._ensure_model(self.custom_voice_model)
            instruct = f"{pack.base_instruct}{slot.style_instruct}".strip()
            wavs, sr = model.generate_custom_voice(
                text=slot.text,
                language=pack.language,
                speaker=pack.speaker,
                instruct=instruct,
                max_new_tokens=max_tokens,
            )
        else:
            model = self._ensure_model(self.voice_design_model)
            instruct = f"{pack.voice_design_instruct}{slot.style_instruct}".strip()
            wavs, sr = model.generate_voice_design(
                text=slot.text,
                language=pack.language,
                instruct=instruct,
                max_new_tokens=max_tokens,
            )

        waveform = wavs[0] if isinstance(wavs, (list, tuple)) and wavs else wavs
        pcm16 = coerce_audio_to_pcm16_bytes(waveform)
        return pcm16, int(sr or DEFAULT_SAMPLE_RATE)


def _unpack_pcm16(pcm16_bytes: bytes) -> List[float]:
    if len(pcm16_bytes) % 2 != 0:
        raise ValueError("PCM16 buffer length must be even")
    count = len(pcm16_bytes) // 2
    if count == 0:
        return []
    return [s / 32767.0 for s in struct.unpack(f"<{count}h", pcm16_bytes)]


def _pack_pcm16(samples: Sequence[float]) -> bytes:
    if not samples:
        return b""
    clamped = [int(round(max(-1.0, min(1.0, s)) * 32767.0)) for s in samples]
    return struct.pack(f"<{len(clamped)}h", *clamped)


def _resample_linear(samples: Sequence[float], src_rate: int, dst_rate: int) -> List[float]:
    if src_rate == dst_rate or not samples:
        return list(samples)
    ratio = dst_rate / float(src_rate)
    out_len = max(1, int(round(len(samples) * ratio)))
    out: List[float] = []
    last_idx = len(samples) - 1
    for i in range(out_len):
        src_pos = i / ratio
        lo = int(src_pos)
        if lo >= last_idx:
            out.append(samples[last_idx])
        else:
            frac = src_pos - lo
            out.append(samples[lo] * (1.0 - frac) + samples[lo + 1] * frac)
    return out


def trim_silence(
    pcm16_bytes: bytes,
    *,
    sample_rate: int = DEFAULT_SAMPLE_RATE,
    threshold_db: float = -42.0,
    head_pad_ms: float = 15.0,
    tail_pad_ms: float = 25.0,
    fade_ms: float = 4.0,
) -> bytes:
    """Trim leading and trailing silence from mono PCM16 audio with smooth edge fades."""
    samples = _unpack_pcm16(pcm16_bytes)
    if not samples:
        return b""

    peak = max(abs(s) for s in samples)
    if peak <= 1e-6:
        return pcm16_bytes

    abs_thresh = max(10.0 ** (threshold_db / 20.0), peak * 0.015)
    window = max(1, int(round(sample_rate * 0.005)))  # 5 ms scanning window

    first_active = 0
    for idx in range(0, len(samples), window):
        seg = samples[idx : idx + window]
        if max(abs(s) for s in seg) >= abs_thresh:
            first_active = idx
            break

    last_active = len(samples)
    for idx in range(len(samples), 0, -window):
        start = max(0, idx - window)
        seg = samples[start:idx]
        if max(abs(s) for s in seg) >= abs_thresh:
            last_active = idx
            break

    if last_active <= first_active:
        return pcm16_bytes

    head_pad = int(round(sample_rate * (head_pad_ms / 1000.0)))
    tail_pad = int(round(sample_rate * (tail_pad_ms / 1000.0)))
    crop_start = max(0, first_active - head_pad)
    crop_end = min(len(samples), last_active + tail_pad)
    cropped = samples[crop_start:crop_end]

    fade_samples = min(len(cropped) // 2, max(1, int(round(sample_rate * (fade_ms / 1000.0)))))
    for i in range(fade_samples):
        gain = i / float(fade_samples)
        cropped[i] *= gain
        cropped[-(i + 1)] *= gain

    return _pack_pcm16(cropped)


def normalize_loudness(
    pcm16_bytes: bytes,
    *,
    target_dbfs: float = DEFAULT_TARGET_RMS_DBFS,
    peak_limit_dbfs: float = DEFAULT_PEAK_LIMIT_DBFS,
) -> bytes:
    """Normalize mono PCM16 clip to consistent RMS loudness with peak safety limiting."""
    samples = _unpack_pcm16(pcm16_bytes)
    if not samples:
        return b""

    mean_sq = sum(s * s for s in samples) / len(samples)
    rms = math.sqrt(mean_sq)
    peak = max(abs(s) for s in samples)
    if rms <= 1e-6 or peak <= 1e-6:
        return pcm16_bytes

    target_rms = 10.0 ** (target_dbfs / 20.0)
    peak_ceiling = 10.0 ** (peak_limit_dbfs / 20.0)

    gain = target_rms / rms
    if peak * gain > peak_ceiling:
        gain = peak_ceiling / peak

    normalized = [s * gain for s in samples]
    return _pack_pcm16(normalized)


def _round_sec(value: float) -> float:
    return round(value, 4)


def stitch_audio_sprite(
    slot_clips: Sequence[Tuple[str, bytes]],
    *,
    sample_rate: int = DEFAULT_SAMPLE_RATE,
    gap_sec: float = DEFAULT_CLIP_GAP_SEC,
    tail_pad_sec: float = DEFAULT_TAIL_PAD_SEC,
) -> Tuple[bytes, Dict[str, List[float]], float]:
    """Concatenate ordered `(slot_id, pcm16_bytes)` clips into one AudioSprite PCM buffer."""
    gap_samples = max(1, int(round(gap_sec * sample_rate)))
    gap_bytes = b"\x00\x00" * gap_samples
    tail_bytes = b"\x00\x00" * max(0, int(round(tail_pad_sec * sample_rate)))

    chunks: List[bytes] = []
    sprites: Dict[str, List[float]] = {}
    cursor_samples = 0

    for idx, (slot_id, clip_pcm) in enumerate(slot_clips):
        if len(clip_pcm) == 0 or len(clip_pcm) % 2 != 0:
            raise ValueError(f"Slot '{slot_id}' produced empty or unaligned PCM16 audio")
        clip_samples = len(clip_pcm) // 2
        start_sec = _round_sec(cursor_samples / float(sample_rate))
        dur_sec = _round_sec(clip_samples / float(sample_rate))
        sprites[slot_id] = [start_sec, dur_sec]

        chunks.append(clip_pcm)
        cursor_samples += clip_samples

        if idx < len(slot_clips) - 1:
            chunks.append(gap_bytes)
            cursor_samples += gap_samples

    if tail_bytes:
        chunks.append(tail_bytes)
        cursor_samples += len(tail_bytes) // 2

    total_duration_sec = _round_sec(cursor_samples / float(sample_rate))
    return b"".join(chunks), sprites, total_duration_sec


def encode_m4a_auto(
    wav_path: pathlib.Path,
    m4a_path: pathlib.Path,
    bitrate_bps: int = DEFAULT_BITRATE_BPS,
) -> None:
    """Encode mono WAV to AAC `.m4a` via `ffmpeg` (Windows/Linux) or `afconvert` (macOS)."""
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg:
        subprocess.run(
            [
                ffmpeg,
                "-y",
                "-loglevel",
                "error",
                "-i",
                str(wav_path),
                "-ac",
                "1",
                "-ar",
                str(DEFAULT_SAMPLE_RATE),
                "-c:a",
                "aac",
                "-b:a",
                str(bitrate_bps),
                "-movflags",
                "+faststart",
                str(m4a_path),
            ],
            check=True,
        )
        return

    afconvert = pathlib.Path("/usr/bin/afconvert")
    if afconvert.exists():
        subprocess.run(
            [
                str(afconvert),
                "-f",
                "m4af",
                "-d",
                "aac",
                "-c",
                "1",
                "-b",
                str(bitrate_bps),
                str(wav_path),
                str(m4a_path),
            ],
            check=True,
        )
        return

    raise RuntimeError(
        "No AAC encoder available: install `ffmpeg` in PATH (Windows/Linux) or use macOS `/usr/bin/afconvert`."
    )


def _atomic_write_bytes(target: pathlib.Path, data: bytes) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp_name = tempfile.mkstemp(prefix=f".{target.name}.", dir=str(target.parent))
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(data)
        os.replace(tmp_name, target)
    except Exception:
        if os.path.exists(tmp_name):
            os.unlink(tmp_name)
        raise


def generate_voice_packs(
    *,
    pack_ids: Optional[Sequence[str]] = None,
    output_dir: pathlib.Path | str = REPO_ROOT / "web" / "public" / "assets" / "vo",
    voices_dir: pathlib.Path | str = THIS_DIR / "voices",
    synthesizer: VoicePackSynthesizer,
    encoder: EncoderFn = encode_m4a_auto,
    bitrate_bps: int = DEFAULT_BITRATE_BPS,
    sample_rate: int = DEFAULT_SAMPLE_RATE,
    target_dbfs: float = DEFAULT_TARGET_RMS_DBFS,
    peak_limit_dbfs: float = DEFAULT_PEAK_LIMIT_DBFS,
    gap_sec: float = DEFAULT_CLIP_GAP_SEC,
    tail_pad_sec: float = DEFAULT_TAIL_PAD_SEC,
) -> List[GeneratedPackResult]:
    """Generate one or all 7 VoicePacks idempotently into `output_dir` and `voices_dir`."""
    selected_specs: List[VoicePackSpec] = []
    if pack_ids is None:
        selected_specs = list(CANONICAL_PACK_SPECS)
    else:
        seen = set()
        for raw_id in pack_ids:
            pid = raw_id.strip()
            if pid not in PACK_SPECS_BY_ID:
                raise ValueError(
                    f"Unknown voice_pack_id '{pid}'; expected one of {list(PACK_SPECS_BY_ID.keys())}"
                )
            if pid not in seen:
                seen.add(pid)
                selected_specs.append(PACK_SPECS_BY_ID[pid])

    out_root = pathlib.Path(output_dir)
    voices_root = pathlib.Path(voices_dir)
    results: List[GeneratedPackResult] = []

    try:
        for spec in selected_specs:
            processed_clips: List[Tuple[str, bytes]] = []
            for slot in CANONICAL_SLOTS:
                max_dur_sec = SLOT_MAX_DURATION_SEC.get(slot.category, 3.5)
                best_trimmed: Optional[bytes] = None
                for _attempt in range(MAX_SLOT_RETRIES):
                    raw_pcm, raw_sr = synthesizer.synthesize_slot(spec, slot)
                    if raw_sr != sample_rate:
                        resampled = _resample_linear(_unpack_pcm16(raw_pcm), raw_sr, sample_rate)
                        raw_pcm = _pack_pcm16(resampled)
                    trimmed = trim_silence(raw_pcm, sample_rate=sample_rate)
                    if best_trimmed is None or len(trimmed) < len(best_trimmed):
                        best_trimmed = trimmed
                    dur_sec = (len(trimmed) // 2) / float(sample_rate)
                    if dur_sec <= max_dur_sec:
                        break
                assert best_trimmed is not None
                normalized = normalize_loudness(
                    best_trimmed,
                    target_dbfs=target_dbfs,
                    peak_limit_dbfs=peak_limit_dbfs,
                )
                processed_clips.append((slot.slot_id, normalized))

            master_pcm, sprites, total_duration = stitch_audio_sprite(
                processed_clips,
                sample_rate=sample_rate,
                gap_sec=gap_sec,
                tail_pad_sec=tail_pad_sec,
            )

            pack_dir = out_root / spec.pack_id
            pack_dir.mkdir(parents=True, exist_ok=True)
            sprite_path = pack_dir / "sprite.m4a"
            manifest_path = pack_dir / "manifest.json"

            with tempfile.TemporaryDirectory(prefix=f"majjj-vo-{spec.pack_id}-") as tmp_dir:
                tmp_path = pathlib.Path(tmp_dir)
                wav_path = tmp_path / "sprite.wav"
                encoded_tmp = tmp_path / "sprite.m4a"
                wav_path.write_bytes(encode_wav_pcm16(master_pcm, sample_rate=sample_rate))
                encoder(wav_path, encoded_tmp, bitrate_bps)
                _atomic_write_bytes(sprite_path, encoded_tmp.read_bytes())

            manifest = {
                "pack_id": spec.pack_id,
                "version": 1,
                "audio": "sprite.m4a",
                "sample_rate": sample_rate,
                "duration": total_duration,
                "sprites": sprites,
            }
            manifest_bytes = (json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
            _atomic_write_bytes(manifest_path, manifest_bytes)

            ref_wav_path: Optional[pathlib.Path] = None
            if spec.export_reference_clip:
                ref_raw_pcm, ref_sr = synthesizer.synthesize_reference(spec)
                if ref_sr != sample_rate:
                    ref_raw_pcm = _pack_pcm16(
                        _resample_linear(_unpack_pcm16(ref_raw_pcm), ref_sr, sample_rate)
                    )
                ref_processed = normalize_loudness(
                    trim_silence(ref_raw_pcm, sample_rate=sample_rate),
                    target_dbfs=target_dbfs,
                    peak_limit_dbfs=peak_limit_dbfs,
                )
                ref_wav_path = voices_root / f"{spec.pack_id}_ref.wav"
                ref_txt_path = voices_root / f"{spec.pack_id}_ref.txt"
                _atomic_write_bytes(
                    ref_wav_path,
                    encode_wav_pcm16(ref_processed, sample_rate=sample_rate),
                )
                _atomic_write_bytes(ref_txt_path, (spec.reference_text + "\n").encode("utf-8"))

            results.append(
                GeneratedPackResult(
                    pack_id=spec.pack_id,
                    manifest_path=manifest_path,
                    sprite_path=sprite_path,
                    sprite_bytes=sprite_path.stat().st_size,
                    duration_sec=total_duration,
                    slot_count=len(sprites),
                    reference_wav_path=ref_wav_path,
                )
            )
    finally:
        synthesizer.release()

    return results


def parse_args(argv: Optional[Sequence[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Offline 7-pack VoicePack (角色语音包) batch generator for Majjj."
    )
    parser.add_argument(
        "--packs",
        nargs="+",
        default=None,
        help=(
            "Optional subset of VoicePack IDs to generate idempotently "
            f"(default: all 7 -> {', '.join(PACK_SPECS_BY_ID.keys())})."
        ),
    )
    parser.add_argument(
        "--output-dir",
        type=pathlib.Path,
        default=REPO_ROOT / "web" / "public" / "assets" / "vo",
        help="Target directory for <pack_id>/{sprite.m4a,manifest.json}.",
    )
    parser.add_argument(
        "--voices-dir",
        type=pathlib.Path,
        default=THIS_DIR / "voices",
        help="Target directory for exported AI reference clips (<pack_id>_ref.wav).",
    )
    parser.add_argument(
        "--bitrate",
        type=int,
        default=DEFAULT_BITRATE_BPS,
        help=f"Mono AAC target bitrate in bps (default: {DEFAULT_BITRATE_BPS}).",
    )
    parser.add_argument(
        "--custom-voice-model",
        default=MODEL_17B_CUSTOM_VOICE,
        help=f"HuggingFace repo or local dir for AI packs (default: {MODEL_17B_CUSTOM_VOICE}).",
    )
    parser.add_argument(
        "--voice-design-model",
        default=MODEL_17B_VOICE_DESIGN,
        help=f"HuggingFace repo or local dir for human packs (default: {MODEL_17B_VOICE_DESIGN}).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Use deterministic pure-stdlib tones instead of loading 1.7B GPU weights.",
    )
    return parser.parse_args(argv)


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = parse_args(argv)
    synthesizer: VoicePackSynthesizer = (
        DryRunSynthesizer()
        if args.dry_run
        else Qwen3Offline17BSynthesizer(
            custom_voice_model=args.custom_voice_model,
            voice_design_model=args.voice_design_model,
        )
    )
    results = generate_voice_packs(
        pack_ids=args.packs,
        output_dir=args.output_dir,
        voices_dir=args.voices_dir,
        synthesizer=synthesizer,
        bitrate_bps=args.bitrate,
    )
    total_bytes = sum(r.sprite_bytes for r in results)
    for r in results:
        ref_note = f", ref={r.reference_wav_path.name}" if r.reference_wav_path else ""
        print(
            f"[voice-pack] {r.pack_id}: {r.slot_count} slots, "
            f"{r.duration_sec:.2f}s, {r.sprite_bytes:,} bytes{ref_note}"
        )
    print(f"[voice-pack] Total ({len(results)} packs): {total_bytes:,} bytes")
    if len(results) == len(CANONICAL_PACK_SPECS) and total_bytes > MAX_TOTAL_M4A_BYTES:
        print(
            f"ERROR: total VoicePack size {total_bytes} exceeds budget {MAX_TOTAL_M4A_BYTES} bytes",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
