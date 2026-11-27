"""CLI entrypoint for launching the Majjj Windows LAN emotional TTS server.

Sets PYTORCH_CUDA_ALLOC_CONF BEFORE importing torch or uvicorn so Guardrail #3
remains active even when detached via Win32_Process.Create or SYSTEM Scheduled Task.
"""

from __future__ import annotations

import argparse
import logging
import os
import pathlib
import sys

SERVER_DIR = pathlib.Path(__file__).resolve().parent
if str(SERVER_DIR) not in sys.path:
    sys.path.insert(0, str(SERVER_DIR))

from vram_guard import (  # noqa: E402
    DEFAULT_CUDA_ALLOC_CONF,
    DEFAULT_MAX_NEW_TOKENS,
    DEFAULT_VRAM_FRACTION,
    configure_cuda_alloc_env,
)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Majjj Windows LAN Emotional TTS Server")
    parser.add_argument("--host", default="0.0.0.0", help="Bind host (default: 0.0.0.0)")
    parser.add_argument("--port", type=int, default=8081, help="Bind port (default: 8081)")
    parser.add_argument(
        "--backend",
        default=os.environ.get("MAJJJ_TTS_BACKEND", "qwen3-custom-voice"),
        choices=("qwen3-custom-voice", "cosyvoice3", "qwen3-base-clone"),
        help="Pluggable TTS inference backend",
    )
    parser.add_argument("--model-path", default="", help="Local snapshot directory or HF model id")
    parser.add_argument("--voices-dir", default="", help="Reference audio directory for clone/CosyVoice3 backends")
    parser.add_argument("--personas-path", default="", help="Path to personas.json mapping")
    parser.add_argument(
        "--vram-fraction",
        type=float,
        default=DEFAULT_VRAM_FRACTION,
        help="PyTorch per-process CUDA memory fraction cap (default: 0.30)",
    )
    parser.add_argument(
        "--max-new-tokens",
        type=int,
        default=DEFAULT_MAX_NEW_TOKENS,
        help="Hard cap on generated codec frames per utterance (default: 120)",
    )
    parser.add_argument(
        "--cuda-alloc-conf",
        default=DEFAULT_CUDA_ALLOC_CONF,
        help="Windows WDDM-safe PYTORCH_CUDA_ALLOC_CONF value",
    )
    parser.add_argument("--log-file", default="", help="Optional log file path")
    return parser.parse_args(argv)


def _configure_logging(log_file: str) -> None:
    handlers: list[logging.Handler] = [logging.StreamHandler(sys.stdout)]
    if log_file:
        log_path = pathlib.Path(log_file)
        log_path.parent.mkdir(parents=True, exist_ok=True)
        handlers.append(logging.FileHandler(log_path, encoding="utf-8"))
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
        handlers=handlers,
        force=True,
    )


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)

    # Guardrail #3 MUST be configured before torch is imported anywhere.
    validated_alloc = configure_cuda_alloc_env(alloc_conf=args.cuda_alloc_conf)

    os.environ["MAJJJ_TTS_BACKEND"] = args.backend
    if args.model_path:
        os.environ["MAJJJ_TTS_MODEL_PATH"] = args.model_path
        if pathlib.Path(args.model_path).exists():
            os.environ.setdefault("HF_HUB_OFFLINE", "1")
    if args.voices_dir:
        os.environ["MAJJJ_TTS_VOICES_DIR"] = args.voices_dir
    if args.personas_path:
        os.environ["MAJJJ_TTS_PERSONAS_PATH"] = args.personas_path
    os.environ["MAJJJ_TTS_VRAM_FRACTION"] = str(args.vram_fraction)
    os.environ["MAJJJ_TTS_MAX_NEW_TOKENS"] = str(args.max_new_tokens)

    _configure_logging(args.log_file)
    logging.getLogger("majjj.tts").info(
        "Starting Majjj TTS server (backend=%s, bind=%s:%d, vram_fraction=%.2f, max_new_tokens=%d, PYTORCH_CUDA_ALLOC_CONF=%s)",
        args.backend,
        args.host,
        args.port,
        args.vram_fraction,
        args.max_new_tokens,
        validated_alloc,
    )

    import uvicorn  # type: ignore[import-not-found]
    from app import build_default_app

    app = build_default_app()
    uvicorn.run(
        app,
        host=args.host,
        port=args.port,
        workers=1,
        log_level="info",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
