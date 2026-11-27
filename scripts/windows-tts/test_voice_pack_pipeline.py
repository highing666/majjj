"""Offline pure-stdlib unit & contract tests for the 1.7B VoicePack batch pipeline."""

from __future__ import annotations

import json
import math
import pathlib
import re
import struct
import sys
import tempfile
import unittest
import wave
from typing import Any, Dict, List, Tuple

THIS_DIR = pathlib.Path(__file__).resolve().parent
REPO_ROOT = THIS_DIR.parent.parent
if str(THIS_DIR) not in sys.path:
    sys.path.insert(0, str(THIS_DIR))

import generate_voice_packs as gvp  # noqa: E402


def _make_sine_pcm16(
    duration_sec: float,
    *,
    freq_hz: float = 440.0,
    amplitude: float = 0.5,
    sample_rate: int = 24000,
) -> bytes:
    total = int(round(duration_sec * sample_rate))
    samples = [
        int(round(max(-1.0, min(1.0, amplitude * math.sin(2.0 * math.pi * freq_hz * i / sample_rate))) * 32767.0))
        for i in range(total)
    ]
    return struct.pack(f"<{len(samples)}h", *samples)


def _pcm16_rms_dbfs(pcm16_bytes: bytes) -> float:
    count = len(pcm16_bytes) // 2
    if count == 0:
        return -120.0
    samples = struct.unpack(f"<{count}h", pcm16_bytes)
    mean_sq = sum((s / 32767.0) ** 2 for s in samples) / count
    if mean_sq <= 1e-12:
        return -120.0
    return 10.0 * math.log10(mean_sq)


def _pcm16_peak_dbfs(pcm16_bytes: bytes) -> float:
    count = len(pcm16_bytes) // 2
    if count == 0:
        return -120.0
    samples = struct.unpack(f"<{count}h", pcm16_bytes)
    peak = max(abs(s) / 32767.0 for s in samples)
    if peak <= 1e-12:
        return -120.0
    return 20.0 * math.log10(peak)


def _parse_ts_slot_texts(schema_path: pathlib.Path) -> Dict[str, str]:
    """Independent source of truth parsed from web/src/lib/audio/voicePackSchema.ts."""
    src = schema_path.read_text(encoding="utf-8")
    match = re.search(
        r"export const VOICE_SLOT_SPEECH_TEXT:\s*Record<VoiceSlotId,\s*string>\s*=\s*\{(.*?)\};",
        src,
        re.DOTALL,
    )
    if not match:
        raise AssertionError("Failed to locate VOICE_SLOT_SPEECH_TEXT in voicePackSchema.ts")
    body = match.group(1)
    pairs = re.findall(r'([a-z0-9_]+)\s*:\s*"([^"]+)"', body)
    return dict(pairs)


def _fake_m4a_encoder(wav_path: pathlib.Path, m4a_path: pathlib.Path, bitrate_bps: int) -> None:
    """Deterministic fake AAC encoder writing a valid minimal MP4 ftyp header + payload."""
    with wave.open(str(wav_path), "rb") as wf:
        frames = wf.getnframes()
        rate = wf.getframerate()
    header = struct.pack(">I4s4sI", 20, b"ftyp", b"M4A ", 0) + b"M4A "
    meta = f"FAKE_M4A:frames={frames}:rate={rate}:bitrate={bitrate_bps}".encode("ascii")
    m4a_path.write_bytes(header + meta)


class CanonicalSchemaParityTests(unittest.TestCase):
    def test_pack_specs_match_personas_json_and_17b_models(self) -> None:
        personas_json = json.loads(
            (THIS_DIR / "server" / "personas.json").read_text(encoding="utf-8")
        )
        expected_ai_speakers = {
            entry["voice_pack_id"]: entry["speaker"]
            for entry in personas_json["personas"].values()
        }

        pack_ids = [spec.pack_id for spec in gvp.CANONICAL_PACK_SPECS]
        self.assertEqual(
            pack_ids,
            [
                "quick_hothead",
                "tea_sarcasm",
                "stubborn_blamer",
                "chaos_instigator",
                "tilted_master",
                "human_male",
                "human_female",
            ],
        )

        for spec in gvp.CANONICAL_PACK_SPECS:
            if spec.kind == "ai":
                self.assertEqual(spec.model_repo, gvp.MODEL_17B_CUSTOM_VOICE)
                self.assertEqual(spec.speaker, expected_ai_speakers[spec.pack_id])
                self.assertTrue(spec.export_reference_clip)
            else:
                self.assertEqual(spec.kind, "human")
                self.assertEqual(spec.model_repo, gvp.MODEL_17B_VOICE_DESIGN)
                self.assertIsNone(spec.speaker)
                self.assertTrue(spec.voice_design_instruct.strip())
                self.assertFalse(spec.export_reference_clip)

    def test_46_canonical_slots_match_voice_pack_schema_ts(self) -> None:
        schema_path = REPO_ROOT / "web" / "src" / "lib" / "audio" / "voicePackSchema.ts"
        ts_slots = _parse_ts_slot_texts(schema_path)
        self.assertEqual(len(ts_slots), 46)
        self.assertEqual(len(gvp.CANONICAL_SLOTS), 46)

        py_slots = {slot.slot_id: slot.text for slot in gvp.CANONICAL_SLOTS}
        self.assertEqual(py_slots, ts_slots)


class AudioProcessingTests(unittest.TestCase):
    def test_trim_silence_removes_leading_and_trailing_silence(self) -> None:
        sr = 24000
        lead_silence = b"\x00\x00" * int(sr * 0.40)
        tone = _make_sine_pcm16(0.30, freq_hz=440.0, amplitude=0.45, sample_rate=sr)
        trail_silence = b"\x00\x00" * int(sr * 0.50)

        trimmed = gvp.trim_silence(lead_silence + tone + trail_silence, sample_rate=sr)
        trimmed_sec = (len(trimmed) // 2) / sr

        # Original was 1.20s, speech was 0.30s; trimmed should be ~0.34s (speech + small head/tail pad)
        self.assertGreaterEqual(trimmed_sec, 0.29)
        self.assertLess(trimmed_sec, 0.45)

    def test_normalize_loudness_equalizes_rms_and_respects_peak_ceiling(self) -> None:
        quiet = _make_sine_pcm16(0.25, freq_hz=500.0, amplitude=0.05)
        loud = _make_sine_pcm16(0.25, freq_hz=500.0, amplitude=0.95)

        norm_quiet = gvp.normalize_loudness(quiet, target_dbfs=-16.0, peak_limit_dbfs=-1.5)
        norm_loud = gvp.normalize_loudness(loud, target_dbfs=-16.0, peak_limit_dbfs=-1.5)

        self.assertAlmostEqual(_pcm16_rms_dbfs(norm_quiet), -16.0, delta=0.6)
        self.assertAlmostEqual(_pcm16_rms_dbfs(norm_loud), -16.0, delta=0.6)
        self.assertLessEqual(_pcm16_peak_dbfs(norm_quiet), -1.4)
        self.assertLessEqual(_pcm16_peak_dbfs(norm_loud), -1.4)

    def test_stitch_audio_sprite_produces_non_overlapping_slices_within_duration(self) -> None:
        clips = [
            ("m1", _make_sine_pcm16(0.20, freq_hz=330.0)),
            ("pong", _make_sine_pcm16(0.35, freq_hz=440.0)),
            ("emote_hurry_up", _make_sine_pcm16(0.50, freq_hz=550.0)),
        ]
        master_pcm, sprites, total_dur = gvp.stitch_audio_sprite(
            clips, sample_rate=24000, gap_sec=0.08, tail_pad_sec=0.06
        )

        self.assertGreater(len(master_pcm), 0)
        self.assertEqual(set(sprites.keys()), {"m1", "pong", "emote_hurry_up"})
        ordered = sorted(sprites.values(), key=lambda pair: pair[0])
        for idx, (start, dur) in enumerate(ordered):
            self.assertGreaterEqual(start, 0.0)
            self.assertGreater(dur, 0.0)
            self.assertLessEqual(start + dur, total_dur + 1e-4)
            if idx > 0:
                prev_start, prev_dur = ordered[idx - 1]
                self.assertGreaterEqual(start, prev_start + prev_dur + 0.07)


class BatchPipelineEndToEndTests(unittest.TestCase):
    def test_dry_run_single_and_all_packs_with_ref_clips_and_idempotent_rerun(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            out_dir = root / "vo"
            voices_dir = root / "voices"

            # 1. Single-pack run
            single_results = gvp.generate_voice_packs(
                pack_ids=["quick_hothead"],
                output_dir=out_dir,
                voices_dir=voices_dir,
                synthesizer=gvp.DryRunSynthesizer(),
                encoder=_fake_m4a_encoder,
                bitrate_bps=24000,
            )
            self.assertEqual([r.pack_id for r in single_results], ["quick_hothead"])
            self.assertTrue((out_dir / "quick_hothead" / "sprite.m4a").is_file())
            self.assertTrue((out_dir / "quick_hothead" / "manifest.json").is_file())
            self.assertTrue((voices_dir / "quick_hothead_ref.wav").is_file())
            self.assertTrue((voices_dir / "quick_hothead_ref.txt").is_file())

            first_manifest_text = (out_dir / "quick_hothead" / "manifest.json").read_text(encoding="utf-8")

            # 2. All-7-packs idempotent rerun
            all_results = gvp.generate_voice_packs(
                pack_ids=None,
                output_dir=out_dir,
                voices_dir=voices_dir,
                synthesizer=gvp.DryRunSynthesizer(),
                encoder=_fake_m4a_encoder,
                bitrate_bps=24000,
            )
            self.assertEqual(len(all_results), 7)

            # Rerun for quick_hothead produced identical deterministic manifest
            second_manifest_text = (out_dir / "quick_hothead" / "manifest.json").read_text(encoding="utf-8")
            self.assertEqual(first_manifest_text, second_manifest_text)

            # Verify every pack's manifest & reference clip contract
            expected_slot_ids = [s.slot_id for s in gvp.CANONICAL_SLOTS]
            for spec in gvp.CANONICAL_PACK_SPECS:
                pack_dir = out_dir / spec.pack_id
                manifest_path = pack_dir / "manifest.json"
                sprite_path = pack_dir / "sprite.m4a"
                self.assertTrue(manifest_path.is_file(), f"missing {manifest_path}")
                self.assertTrue(sprite_path.is_file(), f"missing {sprite_path}")
                self.assertGreater(sprite_path.stat().st_size, 16)

                manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
                self.assertEqual(manifest["pack_id"], spec.pack_id)
                self.assertEqual(manifest["version"], 1)
                self.assertEqual(manifest["audio"], "sprite.m4a")
                self.assertEqual(manifest["sample_rate"], 24000)
                self.assertGreater(manifest["duration"], 0.0)
                self.assertEqual(list(manifest["sprites"].keys()), expected_slot_ids)

                # Verify non-overlapping bounds
                slices = list(manifest["sprites"].values())
                for idx, (start, dur) in enumerate(slices):
                    self.assertGreaterEqual(start, 0.0)
                    self.assertGreater(dur, 0.0)
                    self.assertLessEqual(start + dur, manifest["duration"] + 1e-3)
                    if idx > 0:
                        prev_start, prev_dur = slices[idx - 1]
                        self.assertGreaterEqual(start, prev_start + prev_dur)

                ref_wav = voices_dir / f"{spec.pack_id}_ref.wav"
                if spec.export_reference_clip:
                    self.assertTrue(ref_wav.is_file(), f"missing AI reference clip {ref_wav}")
                    with wave.open(str(ref_wav), "rb") as wf:
                        self.assertEqual(wf.getnchannels(), 1)
                        self.assertEqual(wf.getsampwidth(), 2)
                        self.assertEqual(wf.getframerate(), 24000)
                        self.assertGreater(wf.getnframes(), 2400)
                else:
                    self.assertFalse(ref_wav.exists(), f"human pack should not export AI ref clip: {ref_wav}")

    def test_unknown_pack_id_raises_value_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            with self.assertRaises(ValueError):
                gvp.generate_voice_packs(
                    pack_ids=["nonexistent_pack"],
                    output_dir=root / "vo",
                    voices_dir=root / "voices",
                    synthesizer=gvp.DryRunSynthesizer(),
                    encoder=_fake_m4a_encoder,
                )

    def test_qwen3_17b_synthesizer_routes_ai_to_custom_voice_and_human_to_voice_design(self) -> None:
        loaded_models: List[Tuple[str, Dict[str, Any]]] = []
        calls: List[Tuple[str, Dict[str, Any]]] = []

        class FakeCuda:
            @staticmethod
            def is_available() -> bool:
                return True

            @staticmethod
            def empty_cache() -> None:
                pass

        class FakeTorch:
            bfloat16 = "bf16"
            cuda = FakeCuda()

        class FakeModel:
            def __init__(self, repo: str) -> None:
                self.repo = repo

            def generate_custom_voice(self, **kwargs: Any) -> Tuple[List[List[float]], int]:
                calls.append(("custom_voice", kwargs))
                return ([[0.25] * 2400], 24000)

            def generate_voice_design(self, **kwargs: Any) -> Tuple[List[List[float]], int]:
                calls.append(("voice_design", kwargs))
                return ([[0.20] * 2400], 24000)

        def fake_loader(repo: str, **kwargs: Any) -> FakeModel:
            loaded_models.append((repo, kwargs))
            return FakeModel(repo)

        synth = gvp.Qwen3Offline17BSynthesizer(
            torch_module=FakeTorch(),
            model_loader=fake_loader,
        )

        ai_spec = gvp.PACK_SPECS_BY_ID["quick_hothead"]
        human_spec = gvp.PACK_SPECS_BY_ID["human_female"]

        ai_pcm, ai_sr = synth.synthesize_slot(ai_spec, gvp.CANONICAL_SLOTS[0])
        human_pcm, human_sr = synth.synthesize_slot(human_spec, gvp.CANONICAL_SLOTS[0])

        self.assertEqual(ai_sr, 24000)
        self.assertEqual(human_sr, 24000)
        self.assertEqual(len(ai_pcm), 2400 * 2)
        self.assertEqual(len(human_pcm), 2400 * 2)

        self.assertEqual(loaded_models[0][0], gvp.MODEL_17B_CUSTOM_VOICE)
        self.assertEqual(loaded_models[0][1]["attn_implementation"], "sdpa")
        self.assertEqual(loaded_models[0][1]["dtype"], "bf16")

        self.assertEqual(loaded_models[1][0], gvp.MODEL_17B_VOICE_DESIGN)
        self.assertEqual(loaded_models[1][1]["attn_implementation"], "sdpa")
        self.assertEqual(loaded_models[1][1]["dtype"], "bf16")

        self.assertEqual(calls[0][0], "custom_voice")
        self.assertEqual(calls[0][1]["speaker"], "Eric")
        self.assertEqual(calls[0][1]["text"], "一万")
        self.assertEqual(calls[1][0], "voice_design")
        self.assertEqual(calls[1][1]["text"], "一万")
        self.assertIn("女声", calls[1][1]["instruct"])

    def test_exported_reference_clips_load_cleanly_in_cosyvoice3_and_qwen3_base_clone(self) -> None:
        server_dir = THIS_DIR / "server"
        if str(server_dir) not in sys.path:
            sys.path.insert(0, str(server_dir))

        from backends import (  # type: ignore[import-not-found]
            BACKEND_COSYVOICE3,
            BACKEND_QWEN3_BASE_CLONE,
            create_backend,
        )
        from personas import PersonaRegistry  # type: ignore[import-not-found]

        registry = PersonaRegistry.from_file(server_dir / "personas.json")

        class FakeCuda:
            @staticmethod
            def is_available() -> bool:
                return True

            @staticmethod
            def set_per_process_memory_fraction(fraction: float, device: int = 0) -> None:
                pass

        class FakeTorch:
            bfloat16 = "bf16"
            cuda = FakeCuda()

        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            out_dir = root / "vo"
            voices_dir = root / "voices"

            gvp.generate_voice_packs(
                pack_ids=None,
                output_dir=out_dir,
                voices_dir=voices_dir,
                synthesizer=gvp.DryRunSynthesizer(),
                encoder=_fake_m4a_encoder,
                bitrate_bps=24000,
            )

            cosy_refs: List[pathlib.Path] = []

            class FakeCosy:
                sample_rate = 24000

                def extract_speaker_features(self, ref_audio_path: str) -> Dict[str, str]:
                    path = pathlib.Path(ref_audio_path)
                    self.assert_exists(path)
                    cosy_refs.append(path)
                    return {"ref_audio_path": ref_audio_path}

                @staticmethod
                def assert_exists(path: pathlib.Path) -> None:
                    if not path.is_file():
                        raise FileNotFoundError(f"Missing reference audio: {path}")

            cosy_backend = create_backend(
                BACKEND_COSYVOICE3,
                registry=registry,
                voices_dir=voices_dir,
                torch_module=FakeTorch(),
                model_loader=lambda model_path, **kwargs: FakeCosy(),
            )
            cosy_backend.load()
            self.assertEqual(len(cosy_refs), 5)
            self.assertEqual(
                {p.name for p in cosy_refs},
                {f"{p.voice_pack_id}_ref.wav" for p in registry.list_personas()},
            )

            base_calls: List[Tuple[pathlib.Path, str]] = []

            class FakeBase:
                def create_voice_clone_prompt(
                    self, *, ref_audio: str, ref_text: str, x_vector_only_mode: bool
                ) -> Dict[str, Any]:
                    path = pathlib.Path(ref_audio)
                    if not path.is_file():
                        raise FileNotFoundError(f"Missing reference audio: {path}")
                    base_calls.append((path, ref_text))
                    return {"ref_audio": ref_audio, "ref_text": ref_text}

                def generate_voice_clone(
                    self, *, text: str, language: str, voice_clone_prompt: Dict[str, Any], max_new_tokens: int
                ) -> Tuple[List[List[float]], int]:
                    return ([[0.0, 0.1]], 24000)

            base_backend = create_backend(
                BACKEND_QWEN3_BASE_CLONE,
                registry=registry,
                voices_dir=voices_dir,
                torch_module=FakeTorch(),
                model_loader=lambda model_path, **kwargs: FakeBase(),
            )
            base_backend.load()
            self.assertEqual(len(base_calls), 30)
            for ref_path, ref_text in base_calls:
                self.assertTrue(ref_path.name.endswith("_ref.wav"))
                expected_txt = ref_path.with_suffix(".txt").read_text(encoding="utf-8").strip()
                self.assertEqual(ref_text, expected_txt)


if __name__ == "__main__":
    unittest.main()
