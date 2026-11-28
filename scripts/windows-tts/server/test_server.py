"""Offline unit & contract tests for the Windows LAN emotional TTS service."""

from __future__ import annotations

import asyncio
import contextlib
import importlib.util
import io
import json
import os
import pathlib
import py_compile
import sys
import tempfile
import threading
import unittest
import wave

SERVER_DIR = pathlib.Path(__file__).resolve().parent
if str(SERVER_DIR) not in sys.path:
    sys.path.insert(0, str(SERVER_DIR))

from audio import (  # noqa: E402
    DEFAULT_CHANNELS,
    DEFAULT_SAMPLE_RATE,
    DEFAULT_SAMPLE_WIDTH_BYTES,
    compute_duration_ms,
    encode_wav_pcm16,
    float_samples_to_pcm16_bytes,
)
from personas import (  # noqa: E402
    CANONICAL_MOODS,
    CANONICAL_PERSONAS,
    PersonaNotFoundError,
    PersonaRegistry,
    MoodNotFoundError,
)
from vram_guard import (  # noqa: E402
    DEFAULT_CUDA_ALLOC_CONF,
    DEFAULT_MAX_NEW_TOKENS,
    DEFAULT_STREAM_CHUNK_SIZE,
    DEFAULT_VRAM_FRACTION,
    VramGuardrailError,
    apply_vram_fraction_guard,
    clamp_max_new_tokens,
    collect_vram_stats,
    validate_cuda_alloc_conf,
    validate_stream_chunk_size,
)
from backends import (  # noqa: E402
    BACKEND_COSYVOICE3,
    BACKEND_QWEN3_BASE_CLONE,
    BACKEND_QWEN3_CUSTOM_VOICE,
    SynthesisRawAudio,
    TtsBackend,
    create_backend,
)
from service import (  # noqa: E402
    DeadlineExceededError,
    InvalidRequestError,
    SynthesizeRequest,
    TtsService,
)


class AudioEncodingContractTests(unittest.TestCase):
    def test_encode_wav_pcm16_produces_valid_24khz_mono_16bit_wav(self) -> None:
        # 0.5 seconds of 24 kHz mono PCM16 = 12,000 samples
        pcm = float_samples_to_pcm16_bytes([0.0, 0.5, -0.5, 1.5, -1.5] * 2400)
        wav_bytes = encode_wav_pcm16(pcm, sample_rate=DEFAULT_SAMPLE_RATE)

        self.assertEqual(wav_bytes[:4], b"RIFF")
        self.assertEqual(wav_bytes[8:12], b"WAVE")

        with wave.open(io.BytesIO(wav_bytes), "rb") as wf:
            self.assertEqual(wf.getnchannels(), DEFAULT_CHANNELS)
            self.assertEqual(wf.getsampwidth(), DEFAULT_SAMPLE_WIDTH_BYTES)
            self.assertEqual(wf.getframerate(), 24000)
            self.assertEqual(wf.getnframes(), 12000)

        self.assertEqual(compute_duration_ms(len(pcm), sample_rate=24000), 500)

    def test_encode_wav_pcm16_rejects_odd_byte_length(self) -> None:
        with self.assertRaises(ValueError):
            encode_wav_pcm16(b"\x00\x01\x02", sample_rate=24000)


class PersonaRegistryContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.personas_path = SERVER_DIR / "personas.json"
        self.registry = PersonaRegistry.from_file(self.personas_path)

    def test_personas_json_covers_all_5_personas_and_6_moods(self) -> None:
        raw = json.loads(self.personas_path.read_text(encoding="utf-8"))
        self.assertEqual(set(raw["personas"].keys()), set(CANONICAL_PERSONAS))
        self.assertEqual(set(raw["moods"].keys()), set(CANONICAL_MOODS))
        self.assertEqual(self.registry.persona_count, 5)

        expected_speakers = {
            "QuickHothead": ("大川", "Eric"),
            "TeaSarcasm": ("小晚", "Serena"),
            "StubbornBlamer": ("大鹏", "Dylan"),
            "ChaosInstigator": ("小桃", "Vivian"),
            "TiltedMaster": ("老陈", "Uncle_Fu"),
        }
        for persona_id, (display_name, speaker) in expected_speakers.items():
            for mood in CANONICAL_MOODS:
                plan = self.registry.resolve(persona_id=persona_id, mood=mood)
                self.assertEqual(plan.persona_id, persona_id)
                self.assertEqual(plan.display_name, display_name)
                self.assertEqual(plan.speaker, speaker)
                self.assertEqual(plan.mood, mood)
                self.assertTrue(plan.instruct.strip())
                self.assertIn("<|endofprompt|>", plan.cosyvoice_instruct)
                self.assertEqual(
                    plan.reference_key, f"{plan.voice_pack_id}_{mood.lower()}"
                )

    def test_registry_resolves_humanized_display_names_and_legacy_chinese_aliases(
        self,
    ) -> None:
        alias_cases = [
            ("QuickHothead", "大川", "急眼快攻哥", "Eric", "quick_hothead"),
            ("TeaSarcasm", "小晚", "阴阳茶艺师", "Serena", "tea_sarcasm"),
            ("StubbornBlamer", "大鹏", "嘴硬死磕王", "Dylan", "stubborn_blamer"),
            ("ChaosInstigator", "小桃", "拱火乐子人", "Vivian", "chaos_instigator"),
            ("TiltedMaster", "老陈", "红温老雀神", "Uncle_Fu", "tilted_master"),
        ]
        for persona_id, new_name, legacy_name, speaker, voice_pack_id in alias_cases:
            for token in (new_name, legacy_name):
                plan = self.registry.resolve(persona_id=token, mood="Deadpan")
                self.assertEqual(plan.persona_id, persona_id)
                self.assertEqual(plan.display_name, new_name)
                self.assertEqual(plan.speaker, speaker)
                self.assertEqual(plan.voice_pack_id, voice_pack_id)

    def test_registry_accepts_snake_case_and_speaker_aliases_and_instruct_override(
        self,
    ) -> None:
        plan = self.registry.resolve(
            persona_id="quick_hothead",
            mood="Tilted",
            instruct_override="自定义急眼愤怒语气",
        )
        self.assertEqual(plan.persona_id, "QuickHothead")
        self.assertEqual(plan.speaker, "Eric")
        self.assertEqual(plan.instruct, "自定义急眼愤怒语气")
        self.assertIn("自定义急眼愤怒语气<|endofprompt|>", plan.cosyvoice_instruct)

        speaker_plan = self.registry.resolve_openai_voice("Uncle_Fu", mood="Deadpan")
        self.assertEqual(speaker_plan.persona_id, "TiltedMaster")
        self.assertEqual(speaker_plan.speaker, "Uncle_Fu")

    def test_registry_rejects_unknown_persona_or_mood(self) -> None:
        with self.assertRaises(PersonaNotFoundError):
            self.registry.resolve(persona_id="UnknownPersona", mood="Smug")
        with self.assertRaises(MoodNotFoundError):
            self.registry.resolve(persona_id="QuickHothead", mood="Ecstatic")


class _FakeCuda:
    def __init__(self, *, available: bool = True, allocated_bytes: int = 0, reserved_bytes: int = 0) -> None:
        self._available = available
        self._allocated = allocated_bytes
        self._reserved = reserved_bytes
        self.fraction_calls: list[tuple[float, int]] = []

    def is_available(self) -> bool:
        return self._available

    def set_per_process_memory_fraction(self, fraction: float, device: int = 0) -> None:
        self.fraction_calls.append((fraction, device))

    def memory_allocated(self, device: int = 0) -> int:
        return self._allocated

    def memory_reserved(self, device: int = 0) -> int:
        return self._reserved


class _FakeTorch:
    bfloat16 = "bf16"
    float16 = "fp16"

    def __init__(self, cuda: _FakeCuda) -> None:
        self.cuda = cuda


class VramGuardrailContractTests(unittest.TestCase):
    def test_cuda_alloc_conf_enforces_windows_wddm_safe_parameters(self) -> None:
        self.assertEqual(
            validate_cuda_alloc_conf(DEFAULT_CUDA_ALLOC_CONF),
            "max_split_size_mb:128,garbage_collection_threshold:0.8",
        )
        with self.assertRaises(VramGuardrailError):
            validate_cuda_alloc_conf("expandable_segments:True,max_split_size_mb:128")

    def test_vram_fraction_guard_sets_30_percent_and_rejects_unsafe_fraction(self) -> None:
        fake_cuda = _FakeCuda(
            available=True,
            allocated_bytes=2410 * 1024 * 1024,
            reserved_bytes=2860 * 1024 * 1024,
        )
        fake_torch = _FakeTorch(fake_cuda)

        applied = apply_vram_fraction_guard(fake_torch, fraction=DEFAULT_VRAM_FRACTION, device=0)
        self.assertTrue(applied)
        self.assertEqual(fake_cuda.fraction_calls, [(0.30, 0)])

        stats = collect_vram_stats(fake_torch, vram_limit_fraction=0.30)
        self.assertEqual(stats.vram_allocated_mib, 2410)
        self.assertEqual(stats.vram_reserved_mib, 2860)
        self.assertAlmostEqual(stats.vram_limit_fraction, 0.30)

        with self.assertRaises(VramGuardrailError):
            apply_vram_fraction_guard(fake_torch, fraction=0.65, device=0)

    def test_max_new_tokens_hard_cap_never_exceeds_120(self) -> None:
        self.assertEqual(DEFAULT_MAX_NEW_TOKENS, 120)
        self.assertEqual(clamp_max_new_tokens(120), 120)
        self.assertEqual(clamp_max_new_tokens(500), 120)
        self.assertEqual(clamp_max_new_tokens(80), 80)
        with self.assertRaises(ValueError):
            clamp_max_new_tokens(0)

    def test_stream_chunk_size_defaults_to_6_and_validates_positive_range(self) -> None:
        self.assertEqual(DEFAULT_STREAM_CHUNK_SIZE, 6)
        self.assertEqual(validate_stream_chunk_size(6), 6)
        self.assertEqual(validate_stream_chunk_size(4), 4)
        self.assertEqual(validate_stream_chunk_size("8"), 8)
        self.assertEqual(validate_stream_chunk_size(120), 120)
        for bad_value in (0, -1, -10, 121, "0", "-4", "abc", "", True, False, 4.5):
            with self.subTest(bad_value=bad_value):
                with self.assertRaises(ValueError):
                    validate_stream_chunk_size(bad_value)


class PluggableBackendContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.registry = PersonaRegistry.from_file(SERVER_DIR / "personas.json")

    def test_qwen3_custom_voice_backend_uses_sdpa_bf16_and_120_token_cap(self) -> None:
        events: list[tuple[str, object]] = []
        fake_cuda = _FakeCuda(available=True)
        fake_torch = _FakeTorch(fake_cuda)

        class FakeQwenModel:
            def generate_custom_voice(self, **kwargs):
                events.append(("generate_custom_voice", kwargs))
                return ([[0.0, 0.25, -0.25, 0.0]], 24000)

        def fake_loader(model_path: str, **kwargs):
            events.append(("load_model", (model_path, kwargs)))
            return FakeQwenModel()

        backend = create_backend(
            BACKEND_QWEN3_CUSTOM_VOICE,
            model_path="Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
            registry=self.registry,
            torch_module=fake_torch,
            model_loader=fake_loader,
            vram_fraction=0.30,
        )
        backend.load()

        # Guardrail #2 must execute before model loading
        self.assertEqual(fake_cuda.fraction_calls, [(0.30, 0)])
        self.assertEqual(events[0][0], "load_model")
        _, load_kwargs = events[0][1]
        self.assertEqual(load_kwargs["attn_implementation"], "sdpa")
        self.assertEqual(load_kwargs["dtype"], "bf16")
        self.assertEqual(backend.personas_cached, 5)

        plan = self.registry.resolve(persona_id="QuickHothead", mood="Tilted")
        audio = backend.synthesize(plan, "啧，你这也能碰？", max_new_tokens=200)
        self.assertEqual(audio.sample_rate, 24000)
        self.assertEqual(len(audio.pcm16_bytes), 8)
        gen_kwargs = events[1][1]
        self.assertEqual(gen_kwargs["speaker"], "Eric")
        self.assertEqual(gen_kwargs["instruct"], plan.instruct)
        self.assertEqual(gen_kwargs["max_new_tokens"], 120)

    def test_cosyvoice3_backend_caches_5_speakers_and_avoids_zero_shot_spk_id_bug(self) -> None:
        calls: list[tuple[str, object]] = []
        fake_torch = _FakeTorch(_FakeCuda(available=True))

        class FakeCosyModel:
            sample_rate = 24000

            def extract_speaker_features(self, ref_audio_path: str):
                calls.append(("extract", ref_audio_path))
                return {"llm_embedding": f"emb:{ref_audio_path}", "flow_embedding": "flow"}

            def synthesize_with_cached_features(self, *, text: str, instruct_prompt: str, spk_features: dict):
                calls.append(("synth", (text, instruct_prompt, spk_features)))
                return ([0.1, -0.1, 0.2, -0.2], 24000)

        def fake_loader(model_path: str, **kwargs):
            calls.append(("load", (model_path, kwargs)))
            return FakeCosyModel()

        backend = create_backend(
            BACKEND_COSYVOICE3,
            model_path="FunAudioLLM/Fun-CosyVoice3-0.5B-2512",
            registry=self.registry,
            voices_dir=SERVER_DIR / "voices_missing_uses_manifest_paths",
            torch_module=fake_torch,
            model_loader=fake_loader,
            vram_fraction=0.30,
        )
        backend.load()

        _, load_kwargs = calls[0][1]
        self.assertTrue(load_kwargs["fp16"])
        self.assertFalse(load_kwargs["load_vllm"])
        self.assertEqual(backend.personas_cached, 5)

        plan = self.registry.resolve(persona_id="TeaSarcasm", mood="Mocking")
        audio = backend.synthesize(plan, "哎哟，您这手牌打得真稳呢。", max_new_tokens=120)
        self.assertEqual(audio.sample_rate, 24000)
        self.assertEqual(len(audio.pcm16_bytes), 8)
        synth_call = [payload for kind, payload in calls if kind == "synth"][0]
        self.assertIn("<|endofprompt|>", synth_call[1])
        self.assertIn("llm_embedding", synth_call[2])

    def test_qwen3_base_clone_backend_uses_5x6_cached_reference_matrix(self) -> None:
        calls: list[tuple[str, object]] = []
        fake_torch = _FakeTorch(_FakeCuda(available=True))

        class FakeBaseModel:
            def create_voice_clone_prompt(self, *, ref_audio: str, ref_text: str, x_vector_only_mode: bool):
                calls.append(("cache_prompt", (ref_audio, x_vector_only_mode)))
                return {"ref_audio": ref_audio, "x_vector_only_mode": x_vector_only_mode}

            def generate_voice_clone(self, *, text: str, language: str, voice_clone_prompt: dict, max_new_tokens: int):
                calls.append(("clone", (text, voice_clone_prompt, max_new_tokens)))
                return ([[0.0, 0.1, -0.1, 0.0]], 24000)

        backend = create_backend(
            BACKEND_QWEN3_BASE_CLONE,
            model_path="Qwen/Qwen3-TTS-12Hz-0.6B-Base",
            registry=self.registry,
            voices_dir=SERVER_DIR,
            torch_module=fake_torch,
            model_loader=lambda model_path, **kwargs: FakeBaseModel(),
            vram_fraction=0.30,
        )
        backend.load()

        # 5 personas x 6 moods = 30 pre-cached reference prompts
        cache_calls = [c for c in calls if c[0] == "cache_prompt"]
        self.assertEqual(len(cache_calls), 30)
        self.assertEqual(backend.personas_cached, 5)
        self.assertFalse(cache_calls[0][1][1])  # x_vector_only_mode=False

        plan = self.registry.resolve(persona_id="TiltedMaster", mood="Tilted")
        audio = backend.synthesize(plan, "这把也能被抢杠胡？！", max_new_tokens=150)
        self.assertEqual(audio.sample_rate, 24000)
        clone_call = [payload for kind, payload in calls if kind == "clone"][0]
        self.assertIn("tilted_master_tilted.wav", clone_call[1]["ref_audio"])
        self.assertEqual(clone_call[2], 120)

    def test_cosyvoice3_and_qwen3_base_clone_resolve_reference_clip_fallbacks(self) -> None:
        fake_torch = _FakeTorch(_FakeCuda(available=True))
        with tempfile.TemporaryDirectory() as tmp:
            voices_dir = pathlib.Path(tmp)
            # QuickHothead has exact mood clip + neutral clip + ref clip
            (voices_dir / "quick_hothead_tilted.wav").write_bytes(b"RIFFtilted")
            (voices_dir / "quick_hothead_tilted.txt").write_text("急眼红温参考台词\n", encoding="utf-8")
            (voices_dir / "quick_hothead_neutral.wav").write_bytes(b"RIFFneutral")
            (voices_dir / "quick_hothead_neutral.txt").write_text("急眼中性参考台词\n", encoding="utf-8")
            (voices_dir / "quick_hothead_ref.wav").write_bytes(b"RIFFref")
            (voices_dir / "quick_hothead_ref.txt").write_text("急眼通用参考台词\n", encoding="utf-8")

            # TeaSarcasm only has generate_voice_packs.py's *_ref.wav / *_ref.txt
            (voices_dir / "tea_sarcasm_ref.wav").write_bytes(b"RIFFtearef")
            (voices_dir / "tea_sarcasm_ref.txt").write_text("小晚离线参考台词\n", encoding="utf-8")

            # 1. CosyVoice3: prefers _neutral.wav, falls back to _ref.wav
            cosy_extracted: dict[str, str] = {}

            class FakeCosy:
                sample_rate = 24000

                def extract_speaker_features(self, ref_audio_path: str):
                    cosy_extracted[pathlib.Path(ref_audio_path).name] = ref_audio_path
                    return {"ref": ref_audio_path}

            cosy_backend = create_backend(
                BACKEND_COSYVOICE3,
                model_path="FunAudioLLM/Fun-CosyVoice3-0.5B-2512",
                registry=self.registry,
                voices_dir=voices_dir,
                torch_module=fake_torch,
                model_loader=lambda model_path, **kwargs: FakeCosy(),
            )
            cosy_backend.load()
            self.assertIn("quick_hothead_neutral.wav", cosy_extracted)
            self.assertIn("tea_sarcasm_ref.wav", cosy_extracted)

            # 2. Qwen3BaseClone: prefers _<mood>.wav -> _neutral.wav -> _ref.wav (with matching .txt)
            used_prompts: list[dict[str, str]] = []

            class FakeBase:
                def create_voice_clone_prompt(self, *, ref_audio: str, ref_text: str, x_vector_only_mode: bool):
                    return {"ref_audio": pathlib.Path(ref_audio).name, "ref_text": ref_text}

                def generate_voice_clone(self, *, text: str, language: str, voice_clone_prompt: dict, max_new_tokens: int):
                    used_prompts.append(voice_clone_prompt)
                    return ([[0.0, 0.1]], 24000)

            base_backend = create_backend(
                BACKEND_QWEN3_BASE_CLONE,
                model_path="Qwen/Qwen3-TTS-12Hz-0.6B-Base",
                registry=self.registry,
                voices_dir=voices_dir,
                torch_module=fake_torch,
                model_loader=lambda model_path, **kwargs: FakeBase(),
            )
            base_backend.load()

            base_backend.synthesize(
                self.registry.resolve(persona_id="QuickHothead", mood="Tilted"),
                "红温了！",
            )
            base_backend.synthesize(
                self.registry.resolve(persona_id="QuickHothead", mood="Smug"),
                "这把稳了！",
            )
            base_backend.synthesize(
                self.registry.resolve(persona_id="TeaSarcasm", mood="Mocking"),
                "真厉害呢。",
            )
            self.assertEqual(
                used_prompts,
                [
                    {"ref_audio": "quick_hothead_tilted.wav", "ref_text": "急眼红温参考台词"},
                    {"ref_audio": "quick_hothead_neutral.wav", "ref_text": "急眼中性参考台词"},
                    {"ref_audio": "tea_sarcasm_ref.wav", "ref_text": "小晚离线参考台词"},
                ],
            )

    def test_create_backend_rejects_unknown_backend_name(self) -> None:
        with self.assertRaises(ValueError):
            create_backend("nonexistent-engine", model_path="x", registry=self.registry)

    def test_qwen3_custom_voice_backend_native_streaming_cancellation_and_fallback(self) -> None:
        fake_torch = _FakeTorch(_FakeCuda(available=True))
        stream_calls: list[dict[str, object]] = []
        produced_steps: list[int] = []
        gen_closed = False

        class StreamingQwenModel:
            def generate_custom_voice(self, **kwargs):
                raise AssertionError("Non-streaming generate_custom_voice should not be called")

            def generate_custom_voice_streaming(self, **kwargs):
                nonlocal gen_closed
                stream_calls.append(kwargs)

                def _gen():
                    nonlocal gen_closed
                    try:
                        produced_steps.append(1)
                        yield ([0.0, 0.25, -0.25, 0.0], 24000, {"chunk": 1})
                        # Empty chunk must be ignored cleanly
                        produced_steps.append(2)
                        yield ([], 24000, {"chunk": 2})
                        produced_steps.append(3)
                        yield ([0.5, -0.5], 24000, {"chunk": 3})
                        produced_steps.append(4)
                        yield ([0.1, -0.1], 24000, {"chunk": 4})
                    finally:
                        gen_closed = True

                return _gen()

        backend = create_backend(
            BACKEND_QWEN3_CUSTOM_VOICE,
            model_path="Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
            registry=self.registry,
            torch_module=fake_torch,
            model_loader=lambda model_path, **kwargs: StreamingQwenModel(),
        )
        backend.load()
        plan = self.registry.resolve(persona_id="QuickHothead", mood="Tilted")

        chunks = list(backend.synthesize_stream(plan, "啧，你这也能碰？", max_new_tokens=200, chunk_size=4))
        self.assertEqual(len(chunks), 3)
        self.assertEqual([len(c) for c in chunks], [8, 4, 4])
        self.assertEqual(stream_calls[0]["speaker"], "Eric")
        self.assertEqual(stream_calls[0]["instruct"], plan.instruct)
        self.assertEqual(stream_calls[0]["chunk_size"], 4)
        self.assertEqual(stream_calls[0]["max_new_tokens"], 120)

        # Cancellation at chunk boundary must stop before generating step 2 and close generator
        produced_steps.clear()
        gen_closed = False
        cancel_event = threading.Event()
        collected: list[bytes] = []
        for chunk in backend.synthesize_stream(plan, "早停测试", chunk_size=6, cancel_event=cancel_event):
            collected.append(chunk)
            cancel_event.set()
        self.assertEqual(len(collected), 1)
        self.assertEqual(produced_steps, [1])
        self.assertTrue(gen_closed)

        # Smooth fallback when underlying library has no generate_custom_voice_streaming
        fallback_calls = 0

        class LegacyQwenModel:
            def generate_custom_voice(self, **kwargs):
                nonlocal fallback_calls
                fallback_calls += 1
                return ([[0.0, 0.2, -0.2, 0.0, 0.1, -0.1]], 24000)

        legacy_backend = create_backend(
            BACKEND_QWEN3_CUSTOM_VOICE,
            model_path="Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
            registry=self.registry,
            torch_module=fake_torch,
            model_loader=lambda model_path, **kwargs: LegacyQwenModel(),
        )
        legacy_backend.load()
        fallback_chunks = list(
            legacy_backend.synthesize_stream(plan, "回退切片", chunk_size=4, chunk_bytes=4)
        )
        self.assertEqual(fallback_calls, 1)
        self.assertEqual(len(fallback_chunks), 3)
        self.assertTrue(all(len(c) == 4 for c in fallback_chunks))

    def test_cosyvoice3_backend_native_streaming_and_fallback(self) -> None:
        fake_torch = _FakeTorch(_FakeCuda(available=True))
        instruct2_calls: list[tuple[str, str, str, bool]] = []
        produced: list[int] = []
        gen_closed = False

        class StreamingCosyModel:
            sample_rate = 24000

            def inference_instruct2(self, text: str, instruct: str, ref_path: str, stream: bool = False):
                nonlocal gen_closed
                instruct2_calls.append((text, instruct, ref_path, stream))

                def _gen():
                    nonlocal gen_closed
                    try:
                        produced.append(1)
                        yield {"tts_speech": [0.1, -0.1, 0.2, -0.2]}
                        produced.append(2)
                        yield {"tts_speech": [0.3, -0.3]}
                    finally:
                        gen_closed = True

                return _gen()

        backend = create_backend(
            BACKEND_COSYVOICE3,
            model_path="FunAudioLLM/Fun-CosyVoice3-0.5B-2512",
            registry=self.registry,
            voices_dir=SERVER_DIR,
            torch_module=fake_torch,
            model_loader=lambda model_path, **kwargs: StreamingCosyModel(),
        )
        backend.load()
        plan = self.registry.resolve(persona_id="TeaSarcasm", mood="Mocking")

        chunks = list(backend.synthesize_stream(plan, "您这手牌可真稳呢。", chunk_size=6))
        self.assertEqual(len(chunks), 2)
        self.assertEqual([len(c) for c in chunks], [8, 4])
        self.assertTrue(instruct2_calls[0][3])  # stream=True

        produced.clear()
        gen_closed = False
        cancel_event = threading.Event()
        got = []
        for chunk in backend.synthesize_stream(plan, "早停", cancel_event=cancel_event):
            got.append(chunk)
            cancel_event.set()
        self.assertEqual(len(got), 1)
        self.assertEqual(produced, [1])
        self.assertTrue(gen_closed)

        # Fallback when only non-streaming synthesize_with_cached_features exists
        class NonStreamingCachedCosy:
            sample_rate = 24000

            def extract_speaker_features(self, ref_audio_path: str):
                return {"emb": ref_audio_path}

            def synthesize_with_cached_features(self, *, text: str, instruct_prompt: str, spk_features: dict):
                return ([0.1, -0.1, 0.2, -0.2], 24000)

        fallback_backend = create_backend(
            BACKEND_COSYVOICE3,
            model_path="FunAudioLLM/Fun-CosyVoice3-0.5B-2512",
            registry=self.registry,
            voices_dir=SERVER_DIR,
            torch_module=fake_torch,
            model_loader=lambda model_path, **kwargs: NonStreamingCachedCosy(),
        )
        fallback_backend.load()
        fallback_chunks = list(fallback_backend.synthesize_stream(plan, "回退切片", chunk_bytes=4))
        self.assertEqual([len(c) for c in fallback_chunks], [4, 4])

    def test_qwen3_base_clone_backend_native_streaming_cancellation_and_fallback(self) -> None:
        fake_torch = _FakeTorch(_FakeCuda(available=True))
        stream_calls: list[dict[str, object]] = []
        produced: list[int] = []
        gen_closed = False

        class StreamingBaseModel:
            def create_voice_clone_prompt(self, *, ref_audio: str, ref_text: str, x_vector_only_mode: bool):
                return {"ref_audio": ref_audio}

            def generate_voice_clone(self, **kwargs):
                raise AssertionError("Non-streaming generate_voice_clone should not be called")

            def generate_voice_clone_streaming(self, **kwargs):
                nonlocal gen_closed
                stream_calls.append(kwargs)

                def _gen():
                    nonlocal gen_closed
                    try:
                        produced.append(1)
                        yield ([0.0, 0.1, -0.1, 0.0], 24000, {"chunk": 1})
                        produced.append(2)
                        yield ([0.2, -0.2], 24000, {"chunk": 2})
                    finally:
                        gen_closed = True

                return _gen()

        backend = create_backend(
            BACKEND_QWEN3_BASE_CLONE,
            model_path="Qwen/Qwen3-TTS-12Hz-0.6B-Base",
            registry=self.registry,
            voices_dir=SERVER_DIR,
            torch_module=fake_torch,
            model_loader=lambda model_path, **kwargs: StreamingBaseModel(),
        )
        backend.load()
        plan = self.registry.resolve(persona_id="TiltedMaster", mood="Tilted")

        chunks = list(backend.synthesize_stream(plan, "又被抢杠胡？！", max_new_tokens=180, chunk_size=4))
        self.assertEqual([len(c) for c in chunks], [8, 4])
        self.assertEqual(stream_calls[0]["chunk_size"], 4)
        self.assertEqual(stream_calls[0]["max_new_tokens"], 120)
        self.assertIn("tilted_master_tilted.wav", stream_calls[0]["voice_clone_prompt"]["ref_audio"])

        produced.clear()
        gen_closed = False
        cancel_event = threading.Event()
        got = []
        for chunk in backend.synthesize_stream(plan, "早停", cancel_event=cancel_event):
            got.append(chunk)
            cancel_event.set()
        self.assertEqual(len(got), 1)
        self.assertEqual(produced, [1])
        self.assertTrue(gen_closed)

        # Fallback when generate_voice_clone_streaming is missing
        class LegacyBaseModel:
            def create_voice_clone_prompt(self, *, ref_audio: str, ref_text: str, x_vector_only_mode: bool):
                return {"ref_audio": ref_audio}

            def generate_voice_clone(self, **kwargs):
                return ([[0.0, 0.1, -0.1, 0.0]], 24000)

        legacy_backend = create_backend(
            BACKEND_QWEN3_BASE_CLONE,
            model_path="Qwen/Qwen3-TTS-12Hz-0.6B-Base",
            registry=self.registry,
            voices_dir=SERVER_DIR,
            torch_module=fake_torch,
            model_loader=lambda model_path, **kwargs: LegacyBaseModel(),
        )
        legacy_backend.load()
        fallback_chunks = list(legacy_backend.synthesize_stream(plan, "回退切片", chunk_bytes=4))
        self.assertEqual([len(c) for c in fallback_chunks], [4, 4])


class _DeterministicFakeClock:
    def __init__(self, initial_ms: float = 1000.0) -> None:
        self.now_ms = initial_ms

    def __call__(self) -> float:
        return self.now_ms

    def advance(self, delta_ms: float) -> None:
        self.now_ms += delta_ms


class _FakeSynthBackend(TtsBackend):
    backend_name = "fake-tts"

    def __init__(
        self,
        registry: PersonaRegistry,
        *,
        clock: _DeterministicFakeClock,
        synth_cost_ms: float = 120.0,
        gate_on_first: threading.Event | None = None,
        entered_first: threading.Event | None = None,
    ) -> None:
        super().__init__(
            model_path="Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
            registry=registry,
            torch_module=_FakeTorch(
                _FakeCuda(
                    available=True,
                    allocated_bytes=2400 * 1024 * 1024,
                    reserved_bytes=2800 * 1024 * 1024,
                )
            ),
        )
        self._clock = clock
        self._synth_cost_ms = synth_cost_ms
        self._gate_on_first = gate_on_first
        self._entered_first = entered_first
        self._lock = threading.Lock()
        self.in_flight = 0
        self.max_in_flight = 0
        self.calls: list[tuple[str, str, str]] = []

    def load(self) -> None:
        self._ready = True

    def synthesize(self, plan, text: str, *, max_new_tokens: int = 120) -> SynthesisRawAudio:
        with self._lock:
            self.in_flight += 1
            self.max_in_flight = max(self.max_in_flight, self.in_flight)
            call_idx = len(self.calls)
            self.calls.append((plan.persona_id, plan.mood, text))
        try:
            if call_idx == 0 and self._entered_first is not None:
                self._entered_first.set()
            if call_idx == 0 and self._gate_on_first is not None:
                self._gate_on_first.wait(timeout=2.0)
            self._clock.advance(self._synth_cost_ms)
            # 2400 samples = 100 ms @ 24 kHz mono PCM16
            pcm = float_samples_to_pcm16_bytes([0.0, 0.2, -0.2, 0.0] * 600)
            return SynthesisRawAudio(pcm16_bytes=pcm, sample_rate=24000)
        finally:
            with self._lock:
                self.in_flight -= 1


class TtsServiceContractTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self) -> None:
        self.registry = PersonaRegistry.from_file(SERVER_DIR / "personas.json")

    async def test_health_and_synthesize_return_valid_24khz_wav_and_headers(self) -> None:
        clock = _DeterministicFakeClock(1000.0)
        backend = _FakeSynthBackend(self.registry, clock=clock, synth_cost_ms=145.0)
        service = TtsService(backend=backend, registry=self.registry, clock_ms=clock)
        service.startup()

        health = service.health()
        self.assertEqual(health["status"], "ok")
        self.assertTrue(health["ready"])
        self.assertEqual(health["backend"], "fake-tts")
        self.assertEqual(health["model"], "Qwen3-TTS-12Hz-0.6B-CustomVoice")
        self.assertEqual(health["queue_depth"], 0)
        self.assertEqual(health["vram_allocated_mib"], 2400)
        self.assertEqual(health["vram_reserved_mib"], 2800)
        self.assertEqual(health["personas_cached"], 5)
        self.assertEqual(health["max_new_tokens"], 120)

        result = await service.synthesize(
            SynthesizeRequest(
                request_id="seat2-gen41",
                persona_id="QuickHothead",
                mood="Tilted",
                text="啧，你这也能碰？手气全让你一个人摸光了吧！",
                deadline_ms=2500,
            )
        )
        self.assertEqual(result.content_type, "audio/wav")
        self.assertEqual(result.headers["X-TTS-Request-Id"], "seat2-gen41")
        self.assertEqual(result.headers["X-TTS-Duration-Ms"], "100")
        self.assertEqual(result.headers["X-TTS-Synth-Ms"], "145")
        self.assertEqual(result.headers["X-TTS-Queue-Wait-Ms"], "0")
        self.assertEqual(result.headers["X-TTS-Backend"], "fake-tts")

        with wave.open(io.BytesIO(result.body), "rb") as wf:
            self.assertEqual(wf.getnchannels(), 1)
            self.assertEqual(wf.getsampwidth(), 2)
            self.assertEqual(wf.getframerate(), 24000)
            self.assertEqual(wf.getnframes(), 2400)

    async def test_concurrent_requests_execute_strictly_serially(self) -> None:
        clock = _DeterministicFakeClock(1000.0)
        entered_first = threading.Event()
        gate_on_first = threading.Event()
        backend = _FakeSynthBackend(
            self.registry,
            clock=clock,
            synth_cost_ms=100.0,
            gate_on_first=gate_on_first,
            entered_first=entered_first,
        )
        service = TtsService(backend=backend, registry=self.registry, clock_ms=clock)
        service.startup()

        req1 = asyncio.create_task(
            service.synthesize(
                SynthesizeRequest(
                    request_id="r1",
                    persona_id="QuickHothead",
                    mood="Smug",
                    text="五巡自摸，就这？",
                    deadline_ms=2500,
                )
            )
        )
        # Wait until r1 enters GPU synthesis so r2 truly contends for the serial queue
        await asyncio.to_thread(entered_first.wait, 2.0)

        req2 = asyncio.create_task(
            service.synthesize(
                SynthesizeRequest(
                    request_id="r2",
                    persona_id="TeaSarcasm",
                    mood="Mocking",
                    text="哎哟，运气真好呢。",
                    deadline_ms=2500,
                )
            )
        )
        await asyncio.sleep(0.02)
        self.assertEqual(service.queue_depth, 2)

        gate_on_first.set()
        res1, res2 = await asyncio.gather(req1, req2)

        self.assertEqual(backend.max_in_flight, 1)
        self.assertEqual(len(backend.calls), 2)
        self.assertEqual(service.queue_depth, 0)
        self.assertEqual(res1.headers["X-TTS-Request-Id"], "r1")
        self.assertEqual(res2.headers["X-TTS-Request-Id"], "r2")
        self.assertGreaterEqual(int(res2.headers["X-TTS-Queue-Wait-Ms"]), 100)

    async def test_expired_deadline_is_discarded_before_enqueue_and_before_gpu(self) -> None:
        clock = _DeterministicFakeClock(5000.0)
        entered_first = threading.Event()
        gate_on_first = threading.Event()
        backend = _FakeSynthBackend(
            self.registry,
            clock=clock,
            synth_cost_ms=900.0,
            gate_on_first=gate_on_first,
            entered_first=entered_first,
        )
        service = TtsService(backend=backend, registry=self.registry, clock_ms=clock)
        service.startup()

        # 1. Pre-enqueue immediate drop: deadline_ms <= 0 never touches backend or queue
        with self.assertRaises(DeadlineExceededError) as pre_ctx:
            await service.synthesize(
                SynthesizeRequest(
                    request_id="expired-pre",
                    persona_id="StubbornBlamer",
                    mood="Deadpan",
                    text="什么破手气。",
                    deadline_ms=0,
                )
            )
        self.assertEqual(pre_ctx.exception.status_code, 408)
        self.assertEqual(len(backend.calls), 0)

        # 2. Pre-GPU drop after queue wait exceeds deadline_ms
        req1 = asyncio.create_task(
            service.synthesize(
                SynthesizeRequest(
                    request_id="slow-1",
                    persona_id="TiltedMaster",
                    mood="Tilted",
                    text="这牌怎么打都点炮！",
                    deadline_ms=2500,
                )
            )
        )
        await asyncio.to_thread(entered_first.wait, 2.0)

        req2 = asyncio.create_task(
            service.synthesize(
                SynthesizeRequest(
                    request_id="stale-in-queue",
                    persona_id="ChaosInstigator",
                    mood="Plotting",
                    text="好戏开场咯。",
                    deadline_ms=500,
                )
            )
        )
        await asyncio.sleep(0.02)
        gate_on_first.set()

        await req1
        with self.assertRaises(DeadlineExceededError) as queued_ctx:
            await req2
        self.assertEqual(queued_ctx.exception.status_code, 408)
        self.assertEqual(queued_ctx.exception.headers["X-TTS-Dropped-Reason"], "deadline_exceeded")
        self.assertGreaterEqual(int(queued_ctx.exception.headers["X-TTS-Queue-Wait-Ms"]), 900)
        # Crucially: req2 was discarded before invoking GPU backend
        self.assertEqual(len(backend.calls), 1)
        self.assertEqual(service.queue_depth, 0)

    async def test_stream_mode_and_openai_compat_and_validation_errors(self) -> None:
        clock = _DeterministicFakeClock(1000.0)
        backend = _FakeSynthBackend(self.registry, clock=clock, synth_cost_ms=50.0)
        service = TtsService(backend=backend, registry=self.registry, clock_ms=clock)
        service.startup()

        # v1.5 stream=True returns true async iterator of raw 24 kHz pcm_s16le chunks
        stream_res = await service.synthesize_stream(
            SynthesizeRequest(
                request_id="stream-1",
                persona_id="ChaosInstigator",
                mood="Smug",
                text="哈哈，点炮了吧！",
                format="pcm",
                stream=True,
                deadline_ms=2500,
            )
        )
        self.assertEqual(stream_res.content_type, "application/octet-stream")
        self.assertEqual(stream_res.headers["X-Audio-Sample-Rate"], "24000")
        self.assertEqual(stream_res.headers["X-Audio-Channels"], "1")
        self.assertEqual(stream_res.headers["X-Audio-Format"], "pcm_s16le")
        self.assertEqual(stream_res.headers["X-TTS-Request-Id"], "stream-1")
        self.assertEqual(stream_res.headers["X-TTS-Queue-Wait-Ms"], "0")
        self.assertEqual(stream_res.headers["X-TTS-Backend"], "fake-tts")
        self.assertEqual(stream_res.headers["X-TTS-Model"], "Qwen3-TTS-12Hz-0.6B-CustomVoice")
        collected_chunks = [chunk async for chunk in stream_res.chunks]
        self.assertTrue(all(len(c) > 0 and len(c) % 2 == 0 for c in collected_chunks))
        self.assertEqual(len(b"".join(collected_chunks)), 4800)
        self.assertEqual(service.queue_depth, 0)

        # Backwards compat: stream=False with format='pcm' still works
        pcm_res = await service.synthesize(
            SynthesizeRequest(
                request_id="nonstream-pcm",
                persona_id="ChaosInstigator",
                mood="Smug",
                text="非流式原始PCM",
                format="pcm",
                stream=False,
                deadline_ms=2500,
            )
        )
        self.assertEqual(pcm_res.content_type, "application/octet-stream")
        self.assertEqual(pcm_res.headers["X-Audio-Format"], "pcm_s16le")
        self.assertEqual(len(pcm_res.body), 4800)

        # OpenAI-compatible speech helper routes through the same serial backend
        openai_res = await service.synthesize_openai_speech(
            voice="Eric",
            input_text="快点出牌啊！",
            instructions="急躁催促",
            response_format="wav",
        )
        self.assertEqual(openai_res.content_type, "audio/wav")
        self.assertEqual(openai_res.body[:4], b"RIFF")

        # Empty text rejected with 400
        with self.assertRaises(InvalidRequestError) as err_ctx:
            await service.synthesize(
                SynthesizeRequest(
                    request_id="bad-text",
                    persona_id="QuickHothead",
                    mood="Smug",
                    text="   ",
                    deadline_ms=2500,
                )
            )
        self.assertEqual(err_ctx.exception.status_code, 400)

    async def test_stream_yields_first_chunk_incrementally_before_full_generation_completes(
        self,
    ) -> None:
        clock = _DeterministicFakeClock(2000.0)
        allow_chunk2 = threading.Event()
        chunk2_reached = threading.Event()
        observed_chunk_sizes: list[int] = []

        class GatedStreamBackend(_FakeSynthBackend):
            def synthesize_stream(
                self,
                plan,
                text: str,
                *,
                max_new_tokens: int = 120,
                chunk_size: int = 6,
                cancel_event: threading.Event | None = None,
                chunk_bytes: int = 9600,
            ):
                observed_chunk_sizes.append(chunk_size)
                with self._lock:
                    self.in_flight += 1
                    self.max_in_flight = max(self.max_in_flight, self.in_flight)
                try:
                    # Emit odd-length raw bytes (3 + 5) to verify even-byte alignment across chunks
                    yield b"\x01\x02\x03"
                    chunk2_reached.set()
                    allow_chunk2.wait(timeout=2.0)
                    if cancel_event is not None and cancel_event.is_set():
                        return
                    yield b"\x04\x05\x06\x07\x08"
                finally:
                    with self._lock:
                        self.in_flight -= 1

        backend = GatedStreamBackend(self.registry, clock=clock)
        service = TtsService(
            backend=backend,
            registry=self.registry,
            stream_chunk_size=4,
            clock_ms=clock,
        )
        service.startup()

        stream_res = await service.synthesize_stream(
            SynthesizeRequest(
                request_id="inc-1",
                persona_id="QuickHothead",
                mood="Smug",
                text="增量吐流测试",
                format="pcm",
                stream=True,
                deadline_ms=2500,
            )
        )

        # First chunk MUST arrive while backend is still blocked waiting for allow_chunk2!
        first_chunk = await asyncio.wait_for(stream_res.chunks.__anext__(), timeout=1.5)
        self.assertEqual(first_chunk, b"\x01\x02")
        self.assertEqual(len(first_chunk) % 2, 0)
        self.assertFalse(allow_chunk2.is_set())
        self.assertEqual(service.queue_depth, 1)
        self.assertEqual(observed_chunk_sizes, [4])

        # Unblock chunk 2 and verify remainder byte coalescing + clean finish
        allow_chunk2.set()
        rest = [chunk async for chunk in stream_res.chunks]
        self.assertEqual(rest, [b"\x03\x04\x05\x06\x07\x08"])
        self.assertEqual(service.queue_depth, 0)
        self.assertEqual(backend.max_in_flight, 1)

    async def test_stream_cancellation_stops_backend_early_and_releases_semaphore(self) -> None:
        clock = _DeterministicFakeClock(3000.0)
        chunk1_emitted = threading.Event()
        allow_chunk2_check = threading.Event()
        produced_chunks: list[int] = []
        backend_exited = threading.Event()

        class CancellableStreamBackend(_FakeSynthBackend):
            def synthesize_stream(
                self,
                plan,
                text: str,
                *,
                max_new_tokens: int = 120,
                chunk_size: int = 6,
                cancel_event: threading.Event | None = None,
                chunk_bytes: int = 9600,
            ):
                with self._lock:
                    self.in_flight += 1
                    self.max_in_flight = max(self.max_in_flight, self.in_flight)
                try:
                    for idx in range(1, 6):
                        if cancel_event is not None and cancel_event.is_set():
                            break
                        produced_chunks.append(idx)
                        yield b"\x10\x20\x30\x40"
                        if idx == 1:
                            chunk1_emitted.set()
                            # Wait briefly for cancellation or test gate before next chunk
                            if cancel_event is not None:
                                cancel_event.wait(timeout=2.0)
                            else:
                                allow_chunk2_check.wait(timeout=2.0)
                finally:
                    with self._lock:
                        self.in_flight -= 1
                    backend_exited.set()

        backend = CancellableStreamBackend(self.registry, clock=clock, synth_cost_ms=50.0)
        service = TtsService(backend=backend, registry=self.registry, clock_ms=clock)
        service.startup()

        stream_res = await service.synthesize_stream(
            SynthesizeRequest(
                request_id="cancel-1",
                persona_id="TeaSarcasm",
                mood="Mocking",
                text="客户端中途断连早停测试",
                format="pcm",
                stream=True,
                deadline_ms=2500,
            )
        )

        # Queue a second request while stream #1 holds the GPU semaphore
        first_chunk = await asyncio.wait_for(stream_res.chunks.__anext__(), timeout=1.5)
        self.assertEqual(first_chunk, b"\x10\x20\x30\x40")
        self.assertTrue(chunk1_emitted.is_set())

        req2_task = asyncio.create_task(
            service.synthesize(
                SynthesizeRequest(
                    request_id="after-cancel-2",
                    persona_id="QuickHothead",
                    mood="Smug",
                    text="紧接着下一句",
                    deadline_ms=2500,
                )
            )
        )
        await asyncio.sleep(0.02)
        self.assertEqual(service.queue_depth, 2)

        # Simulate client disconnect by closing the async stream iterator
        await asyncio.wait_for(stream_res.chunks.aclose(), timeout=1.5)
        self.assertTrue(backend_exited.is_set())
        self.assertEqual(produced_chunks, [1])

        # Second request must now acquire the released GPU lock cleanly with strict serial execution
        res2 = await asyncio.wait_for(req2_task, timeout=1.5)
        self.assertEqual(res2.headers["X-TTS-Request-Id"], "after-cancel-2")
        self.assertEqual(backend.max_in_flight, 1)
        self.assertEqual(service.queue_depth, 0)

    async def test_stream_task_cancel_also_triggers_early_stop_and_releases_semaphore(self) -> None:
        clock = _DeterministicFakeClock(4000.0)
        produced_chunks: list[int] = []
        got_first = asyncio.Event()

        class WaitCancelBackend(_FakeSynthBackend):
            def synthesize_stream(
                self,
                plan,
                text: str,
                *,
                max_new_tokens: int = 120,
                chunk_size: int = 6,
                cancel_event: threading.Event | None = None,
                chunk_bytes: int = 9600,
            ):
                with self._lock:
                    self.in_flight += 1
                    self.max_in_flight = max(self.max_in_flight, self.in_flight)
                try:
                    for idx in range(1, 5):
                        if cancel_event is not None and cancel_event.is_set():
                            break
                        produced_chunks.append(idx)
                        yield b"\xaa\xbb"
                        if cancel_event is not None:
                            cancel_event.wait(timeout=2.0)
                finally:
                    with self._lock:
                        self.in_flight -= 1

        backend = WaitCancelBackend(self.registry, clock=clock, synth_cost_ms=20.0)
        service = TtsService(backend=backend, registry=self.registry, clock_ms=clock)
        service.startup()

        stream_res = await service.synthesize_stream(
            SynthesizeRequest(
                request_id="task-cancel-1",
                persona_id="QuickHothead",
                mood="Tilted",
                text="协程取消早停测试",
                format="pcm",
                stream=True,
                deadline_ms=2500,
            )
        )

        async def _consume() -> None:
            async for _chunk in stream_res.chunks:
                got_first.set()

        consumer_task = asyncio.create_task(_consume())
        await asyncio.wait_for(got_first.wait(), timeout=1.5)
        # Consumer is now suspended inside __anext__() waiting for chunk 2 -> cancel task!
        consumer_task.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await consumer_task

        # A follow-up request must acquire Semaphore(1) cleanly once worker exits on cancel_event
        follow_up = await asyncio.wait_for(
            service.synthesize(
                SynthesizeRequest(
                    request_id="after-task-cancel",
                    persona_id="StubbornBlamer",
                    mood="Deadpan",
                    text="继续下一句",
                    deadline_ms=2500,
                )
            ),
            timeout=1.5,
        )
        self.assertEqual(follow_up.headers["X-TTS-Request-Id"], "after-task-cancel")
        self.assertEqual(produced_chunks, [1])
        self.assertEqual(backend.max_in_flight, 1)
        self.assertEqual(service.queue_depth, 0)

    async def test_stream_deadline_exceeded_returns_408_pre_enqueue_and_after_queue_wait(
        self,
    ) -> None:
        clock = _DeterministicFakeClock(6000.0)
        entered_first = threading.Event()
        gate_on_first = threading.Event()
        backend = _FakeSynthBackend(
            self.registry,
            clock=clock,
            synth_cost_ms=800.0,
            gate_on_first=gate_on_first,
            entered_first=entered_first,
        )
        service = TtsService(backend=backend, registry=self.registry, clock_ms=clock)
        service.startup()

        with self.assertRaises(DeadlineExceededError) as pre_ctx:
            await service.synthesize_stream(
                SynthesizeRequest(
                    request_id="stream-pre-expired",
                    persona_id="QuickHothead",
                    mood="Smug",
                    text="入队前超时",
                    format="pcm",
                    stream=True,
                    deadline_ms=0,
                )
            )
        self.assertEqual(pre_ctx.exception.status_code, 408)
        self.assertEqual(len(backend.calls), 0)

        req1 = asyncio.create_task(
            service.synthesize(
                SynthesizeRequest(
                    request_id="blocking-1",
                    persona_id="TiltedMaster",
                    mood="Tilted",
                    text="占用GPU的请求",
                    deadline_ms=2500,
                )
            )
        )
        await asyncio.to_thread(entered_first.wait, 2.0)

        req2 = asyncio.create_task(
            service.synthesize_stream(
                SynthesizeRequest(
                    request_id="stream-queued-expired",
                    persona_id="ChaosInstigator",
                    mood="Plotting",
                    text="排队超时的流式请求",
                    format="pcm",
                    stream=True,
                    deadline_ms=500,
                )
            )
        )
        await asyncio.sleep(0.02)
        gate_on_first.set()

        await req1
        with self.assertRaises(DeadlineExceededError) as queued_ctx:
            await req2
        self.assertEqual(queued_ctx.exception.status_code, 408)
        self.assertEqual(queued_ctx.exception.headers["X-TTS-Dropped-Reason"], "deadline_exceeded")
        self.assertEqual(len(backend.calls), 1)
        self.assertEqual(service.queue_depth, 0)


class FastApiAppWiringTests(unittest.TestCase):
    def test_app_and_main_compile_cleanly_and_declare_expected_routes(self) -> None:
        app_path = SERVER_DIR / "app.py"
        main_path = SERVER_DIR / "__main__.py"
        with tempfile.TemporaryDirectory() as tmp_dir:
            py_compile.compile(str(app_path), cfile=str(pathlib.Path(tmp_dir) / "app.pyc"), doraise=True)
            py_compile.compile(str(main_path), cfile=str(pathlib.Path(tmp_dir) / "main.pyc"), doraise=True)

        source = app_path.read_text(encoding="utf-8")
        self.assertIn("/health", source)
        self.assertIn("/v1/tts/synthesize", source)
        self.assertIn("/v1/audio/speech", source)
        # Guard against PEP 563 breaking FastAPI resolution of create_app()-local Pydantic models
        for line in source.splitlines():
            self.assertNotEqual(
                line.strip(),
                "from __future__ import annotations",
                "app.py must not enable postponed annotations while body models are local to create_app()",
            )

    def test_cli_and_build_default_service_configure_stream_chunk_size(self) -> None:
        spec = importlib.util.spec_from_file_location("tts_server_main", SERVER_DIR / "__main__.py")
        assert spec is not None and spec.loader is not None
        server_main = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(server_main)
        from app import build_default_service  # noqa: E402

        saved_env = {
            key: os.environ.get(key)
            for key in ("MAJJJ_TTS_STREAM_CHUNK_SIZE", "MAJJJ_TTS_BACKEND", "PYTORCH_CUDA_ALLOC_CONF")
        }
        try:
            os.environ.pop("MAJJJ_TTS_STREAM_CHUNK_SIZE", None)
            args_default = server_main.parse_args([])
            self.assertEqual(args_default.stream_chunk_size, 6)

            args_cli = server_main.parse_args(["--stream-chunk-size", "4"])
            self.assertEqual(args_cli.stream_chunk_size, 4)

            os.environ["MAJJJ_TTS_STREAM_CHUNK_SIZE"] = "8"
            args_env = server_main.parse_args([])
            self.assertEqual(args_env.stream_chunk_size, 8)

            service = build_default_service()
            self.assertEqual(service.stream_chunk_size, 8)
            self.assertEqual(service.health()["stream_chunk_size"], 8)

            os.environ["MAJJJ_TTS_STREAM_CHUNK_SIZE"] = "0"
            with self.assertRaises(ValueError):
                build_default_service()

            with contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit):
                    server_main.parse_args(["--stream-chunk-size", "0"])
        finally:
            for key, value in saved_env.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

    def test_fastapi_testclient_roundtrip_when_fastapi_installed(self) -> None:
        if importlib.util.find_spec("fastapi") is None or importlib.util.find_spec("httpx") is None:
            if os.environ.get("MAJJJ_REQUIRE_FASTAPI_TESTS") == "1":
                self.fail("MAJJJ_REQUIRE_FASTAPI_TESTS=1 but fastapi/httpx are not installed")
            self.skipTest("fastapi/httpx not installed in offline stdlib sandbox")

        from fastapi.testclient import TestClient  # type: ignore[import-not-found]
        from app import create_app  # noqa: E402

        registry = PersonaRegistry.from_file(SERVER_DIR / "personas.json")
        clock = _DeterministicFakeClock(1000.0)
        backend = _FakeSynthBackend(registry, clock=clock, synth_cost_ms=80.0)
        service = TtsService(backend=backend, registry=registry, clock_ms=clock)
        app = create_app(service)

        with TestClient(app) as client:
            health_resp = client.get("/health")
            self.assertEqual(health_resp.status_code, 200)
            self.assertEqual(health_resp.json()["status"], "ok")

            synth_resp = client.post(
                "/v1/tts/synthesize",
                json={
                    "request_id": "fastapi-1",
                    "persona_id": "QuickHothead",
                    "mood": "Smug",
                    "text": "五巡自摸！",
                    "deadline_ms": 2500,
                },
            )
            self.assertEqual(synth_resp.status_code, 200)
            self.assertEqual(synth_resp.headers["content-type"], "audio/wav")
            self.assertEqual(synth_resp.content[:4], b"RIFF")

            speech_resp = client.post(
                "/v1/audio/speech",
                json={
                    "model": "majjj-tts",
                    "voice": "QuickHothead",
                    "input": "搞快点！",
                    "response_format": "wav",
                },
            )
            self.assertEqual(speech_resp.status_code, 200)
            self.assertEqual(speech_resp.headers["content-type"], "audio/wav")
            self.assertEqual(speech_resp.content[:4], b"RIFF")

            expired_resp = client.post(
                "/v1/tts/synthesize",
                json={
                    "request_id": "fastapi-expired",
                    "persona_id": "QuickHothead",
                    "mood": "Smug",
                    "text": "超时丢弃",
                    "deadline_ms": 0,
                },
            )
            self.assertEqual(expired_resp.status_code, 408)

            stream_resp = client.post(
                "/v1/tts/synthesize",
                json={
                    "request_id": "fastapi-stream-1",
                    "persona_id": "QuickHothead",
                    "mood": "Smug",
                    "text": "流式响应契约测试",
                    "format": "pcm",
                    "stream": True,
                    "deadline_ms": 2500,
                },
            )
            self.assertEqual(stream_resp.status_code, 200)
            self.assertEqual(stream_resp.headers["content-type"], "application/octet-stream")
            self.assertEqual(stream_resp.headers["x-audio-format"], "pcm_s16le")
            self.assertEqual(stream_resp.headers["x-audio-sample-rate"], "24000")
            self.assertEqual(stream_resp.headers["x-audio-channels"], "1")
            self.assertEqual(stream_resp.headers["x-tts-request-id"], "fastapi-stream-1")
            self.assertIn("x-tts-queue-wait-ms", stream_resp.headers)
            self.assertEqual(stream_resp.headers["x-tts-backend"], "fake-tts")
            self.assertEqual(stream_resp.headers["x-tts-model"], "Qwen3-TTS-12Hz-0.6B-CustomVoice")
            self.assertEqual(len(stream_resp.content), 4800)

            stream_expired_resp = client.post(
                "/v1/tts/synthesize",
                json={
                    "request_id": "fastapi-stream-expired",
                    "persona_id": "QuickHothead",
                    "mood": "Smug",
                    "text": "流式超时丢弃",
                    "format": "pcm",
                    "stream": True,
                    "deadline_ms": 0,
                },
            )
            self.assertEqual(stream_expired_resp.status_code, 408)


if __name__ == "__main__":
    unittest.main()
