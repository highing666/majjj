"""FastAPI HTTP server wiring for the Majjj Windows LAN emotional TTS service."""

# NOTE: Do NOT enable `from __future__ import annotations` here — SynthesizeBody and
# OpenAiSpeechBody are created inside create_app() (for lazy FastAPI/Pydantic imports),
# so PEP 563 stringified annotations cannot be resolved via globals() by FastAPI.

from contextlib import asynccontextmanager
import os
import pathlib
from typing import Optional

from backends import BACKEND_QWEN3_CUSTOM_VOICE, create_backend
from personas import PersonaRegistry
from service import (
    DEFAULT_DEADLINE_MS,
    SynthesizeRequest,
    TtsService,
    TtsServiceError,
)
from vram_guard import (
    DEFAULT_MAX_NEW_TOKENS,
    DEFAULT_STREAM_CHUNK_SIZE,
    DEFAULT_VRAM_FRACTION,
    configure_cuda_alloc_env,
    validate_stream_chunk_size,
)


def build_default_service() -> TtsService:
    """Construct a TtsService from MAJJJ_TTS_* environment variables."""
    configure_cuda_alloc_env()

    server_dir = pathlib.Path(__file__).resolve().parent
    personas_path = pathlib.Path(
        os.environ.get("MAJJJ_TTS_PERSONAS_PATH") or (server_dir / "personas.json")
    )
    registry = PersonaRegistry.from_file(personas_path)

    backend_name = os.environ.get("MAJJJ_TTS_BACKEND", BACKEND_QWEN3_CUSTOM_VOICE)
    model_path = os.environ.get("MAJJJ_TTS_MODEL_PATH") or None
    voices_dir = os.environ.get("MAJJJ_TTS_VOICES_DIR") or str(server_dir.parent / "voices")
    vram_fraction = float(os.environ.get("MAJJJ_TTS_VRAM_FRACTION", str(DEFAULT_VRAM_FRACTION)))
    max_new_tokens = int(os.environ.get("MAJJJ_TTS_MAX_NEW_TOKENS", str(DEFAULT_MAX_NEW_TOKENS)))
    stream_chunk_size = validate_stream_chunk_size(
        os.environ.get("MAJJJ_TTS_STREAM_CHUNK_SIZE", str(DEFAULT_STREAM_CHUNK_SIZE))
    )

    backend = create_backend(
        backend_name,
        model_path=model_path,
        registry=registry,
        voices_dir=voices_dir,
        vram_fraction=vram_fraction,
        max_new_tokens=max_new_tokens,
    )
    return TtsService(
        backend=backend,
        registry=registry,
        max_new_tokens=max_new_tokens,
        stream_chunk_size=stream_chunk_size,
    )


def create_app(service: Optional[TtsService] = None):
    """Create the FastAPI application bound to a TtsService instance."""
    from fastapi import FastAPI  # type: ignore[import-not-found]
    from fastapi.responses import (  # type: ignore[import-not-found]
        JSONResponse,
        Response,
        StreamingResponse,
    )
    from pydantic import BaseModel, Field  # type: ignore[import-not-found]

    active_service = service or build_default_service()

    class SynthesizeBody(BaseModel):
        request_id: str = Field(..., min_length=1)
        persona_id: str = Field(..., min_length=1)
        mood: str = Field(..., min_length=1)
        text: str = Field(..., min_length=1)
        instruct_override: Optional[str] = None
        format: str = "wav"
        stream: bool = False
        deadline_ms: int = DEFAULT_DEADLINE_MS

    class OpenAiSpeechBody(BaseModel):
        model: str = "majjj-tts"
        voice: str = "QuickHothead"
        input: str = Field(..., min_length=1)
        instructions: Optional[str] = None
        response_format: str = "wav"
        mood: str = "Deadpan"

    @asynccontextmanager
    async def lifespan(_app: FastAPI):
        active_service.startup()
        yield

    app = FastAPI(
        title="Majjj LAN Emotional TTS Service",
        version="1.0.0",
        lifespan=lifespan,
    )

    @app.get("/health")
    async def health() -> JSONResponse:
        payload = active_service.health()
        status_code = 200 if payload["ready"] else 503
        return JSONResponse(content=payload, status_code=status_code)

    @app.post("/v1/tts/synthesize")
    async def synthesize(body: SynthesizeBody):
        req = SynthesizeRequest(
            request_id=body.request_id,
            persona_id=body.persona_id,
            mood=body.mood,
            text=body.text,
            instruct_override=body.instruct_override,
            format=body.format,
            stream=body.stream,
            deadline_ms=body.deadline_ms,
        )
        try:
            if req.stream:
                stream_res = await active_service.synthesize_stream(req)

                async def _iter_stream_chunks():
                    try:
                        async for chunk in stream_res.chunks:
                            yield chunk
                    finally:
                        stream_res.cancel()
                        await stream_res.chunks.aclose()

                return StreamingResponse(
                    _iter_stream_chunks(),
                    media_type=stream_res.content_type,
                    headers=stream_res.headers,
                )
            res = await active_service.synthesize(req)
            return Response(
                content=res.body,
                media_type=res.content_type,
                headers=res.headers,
            )
        except TtsServiceError as exc:
            return JSONResponse(
                content=exc.to_payload(),
                status_code=exc.status_code,
                headers=exc.headers,
            )

    @app.post("/v1/audio/speech")
    async def openai_speech(body: OpenAiSpeechBody):
        try:
            res = await active_service.synthesize_openai_speech(
                voice=body.voice,
                input_text=body.input,
                instructions=body.instructions,
                response_format=body.response_format,
                mood=body.mood,
            )
            return Response(
                content=res.body,
                media_type=res.content_type,
                headers=res.headers,
            )
        except TtsServiceError as exc:
            return JSONResponse(
                content=exc.to_payload(),
                status_code=exc.status_code,
                headers=exc.headers,
            )

    return app


def build_default_app():
    return create_app()
