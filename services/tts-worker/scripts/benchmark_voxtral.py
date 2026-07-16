#!/usr/bin/env python3
"""Benchmark the direct Voxtral INT4 engine and emit JSON plus Markdown evidence.

This command loads the real model, generates one utterance, validates its WAV,
and records software, GPU, timing, audio, and PyTorch memory measurements.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
import soundfile as sf

from audiobookgen_worker.voxtral_int4.runtime import PROFILES, VoxtralInt4Runtime


def command_output(*command: str) -> str | None:
    try:
        return subprocess.check_output(
            command, text=True, stderr=subprocess.DEVNULL
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(8 * 1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def package_version(name: str) -> str | None:
    from importlib.metadata import PackageNotFoundError, version

    try:
        return version(name)
    except PackageNotFoundError:
        return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="JSON output path")
    parser.add_argument("--profile", choices=sorted(PROFILES), default="compatibility")
    parser.add_argument("--voice", default="neutral_female")
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument(
        "--text",
        default="The quick brown fox jumps over the lazy dog, and the audiobook begins.",
    )
    args = parser.parse_args()

    started = time.monotonic()
    runtime = VoxtralInt4Runtime(
        args.model_dir, lambda message: print(message, flush=True)
    )
    load_seconds = time.monotonic() - started
    generation_started = time.monotonic()
    generated = runtime.generate(args.text, args.voice, args.profile, args.seed)
    generation_wall_seconds = time.monotonic() - generation_started

    wav_path = args.output.with_suffix(".wav")
    generated.write_wav(wav_path)
    samples, sample_rate = sf.read(wav_path, dtype="float32")
    profile = PROFILES[args.profile]
    external_memory = command_output(
        "nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"
    )
    metrics = runtime.metrics()
    record = {
        "success": True,
        "git_commit": command_output("git", "rev-parse", "HEAD"),
        "model": "mistralai/Voxtral-4B-TTS-2603",
        "model_sha256": sha256(args.model_dir / "consolidated.safetensors"),
        "gpu": metrics["gpu"],
        "compute_capability": metrics["compute_capability"],
        "driver": command_output(
            "nvidia-smi", "--query-gpu=driver_version", "--format=csv,noheader"
        ),
        "cuda": __import__("torch").version.cuda,
        "torch": metrics["torch"],
        "torchao": package_version("torchao"),
        "hqq": package_version("hqq"),
        "python": platform.python_version(),
        "profile": args.profile,
        "flow_steps": profile["flow_steps"],
        "cfg_alpha": profile["cfg_alpha"],
        "compiled": profile["compile"],
        "voice": args.voice,
        "seed": args.seed,
        "text": args.text,
        "character_count": len(args.text),
        "output_duration_seconds": len(samples) / sample_rate,
        "load_seconds": load_seconds,
        "generation_seconds": generated.generation_seconds,
        "generation_wall_seconds": generation_wall_seconds,
        "time_to_first_audio_seconds": generation_wall_seconds,
        "frames": generated.frame_count,
        "fps": generated.frame_count / generated.generation_seconds,
        "real_time_factor": generated.generation_seconds / generated.duration_seconds,
        "allocated_vram_bytes": metrics["allocated_vram_bytes"],
        "reserved_vram_bytes": metrics["reserved_vram_bytes"],
        "peak_allocated_vram_bytes": metrics["peak_allocated_vram_bytes"],
        "external_gpu_memory_mib": int(external_memory.splitlines()[0])
        if external_memory
        else None,
        "sample_rate": sample_rate,
        "channels": 1 if samples.ndim == 1 else samples.shape[1],
        "finite": bool(np.isfinite(samples).all()),
        "peak": float(np.abs(samples).max(initial=0.0)),
        "rms": float(np.sqrt(np.mean(np.square(samples)))),
        "wav_path": str(wav_path),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    markdown = (
        "# Voxtral INT4 benchmark\n\n"
        + "\n".join(f"- {key}: `{value}`" for key, value in record.items())
        + "\n"
    )
    args.output.with_suffix(".md").write_text(markdown, encoding="utf-8")
    print(json.dumps(record, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
