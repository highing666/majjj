"""Offline unit & contract tests for the Windows LAN emotional TTS service."""

from __future__ import annotations

import asyncio
import importlib.util
import io
import json
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
    DEFAULT_VRAM_FRACTION,
    VramGuardrailError,
    apply_vram_fraction_guard,
    clamp_max_new_tokens,
    collect_vram_stats,
    validate_cuda_alloc_conf,
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
            "QuickHothead": ("急眼快攻哥", "Eric"),
            "TeaSarcasm": ("阴阳茶艺师", "Serena"),
            "StubbornBlamer": ("嘴硬死磕王", "Dylan"),
            "ChaosInstigator": ("拱火乐子人", "Vivian"),
            "TiltedMaster": ("红温老雀神", "Uncle_Fu"),
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

    def test_create_backend_rejects_unknown_backend_name(self) -> None:
        with self.assertRaises(ValueError):
            create_backend("nonexistent-engine", model_path="x", registry=self.registry)


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
                    text="发牌员演我。",
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

        # v1.5 stream=True returns raw 24 kHz pcm_s16le chunks
        stream_res = await service.synthesize_stream(
            SynthesizeRequest(
                request_id="stream-1",
                persona_id="ChaosInstigator",
                mood="Smug",
                text="哈哈，点炮了吧！",
                stream=True,
                deadline_ms=2500,
            )
        )
        self.assertEqual(stream_res.content_type, "application/octet-stream")
        self.assertEqual(stream_res.headers["X-Audio-Sample-Rate"], "24000")
        self.assertEqual(stream_res.headers["X-Audio-Channels"], "1")
        self.assertEqual(stream_res.headers["X-Audio-Format"], "pcm_s16le")
        self.assertEqual(len(b"".join(stream_res.chunks)), 4800)

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

    def test_fastapi_testclient_roundtrip_when_fastapi_installed(self) -> None:
        if importlib.util.find_spec("fastapi") is None or importlib.util.find_spec("httpx") is None:
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


if __name__ == "__main__":
    unittest.main()
