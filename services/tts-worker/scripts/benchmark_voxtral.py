#!/usr/bin/env python3
"""Benchmark the direct Voxtral INT4 engine and emit JSON plus Markdown evidence.

Single mode loads the real model and measures one utterance. Suite mode
(--suite) loads the model once, runs a fixed five-sentence corpus across one
or more profiles with per-phase timing breakdown (prefill, backbone, acoustic
solver, codec), and emits an aggregate report for the speed ledger.
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

# Fixed corpus: identical text, voice, and seed across every benchmark run so
# before/after comparisons in reports/benchmarks/SPEEDLOG.md stay honest.
CORPUS: dict[str, str] = {
    "short": "The door opened at last.",
    "medium": "The quick brown fox jumps over the lazy dog, and the audiobook begins.",
    "long": (
        "When the expedition finally reached the ridge above the valley, the "
        "narrator paused to describe the slow river, the abandoned mill, and "
        "the long shadows that folded themselves across the orchard walls."
    ),
    "dialogue": '"You cannot be serious," she said quietly. "We leave tonight."',
    "numbers": (
        "On March 14, 1897, the ledger recorded 1,204 barrels, 17 wagons, "
        "and a debt of exactly 96 dollars."
    ),
}


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


def environment_record(runtime: VoxtralInt4Runtime, model_dir: Path) -> dict:
    import torch

    metrics = runtime.metrics()
    return {
        "git_commit": command_output("git", "rev-parse", "HEAD"),
        "model": "mistralai/Voxtral-4B-TTS-2603",
        "model_sha256": sha256(model_dir / "consolidated.safetensors"),
        "gpu": metrics["gpu"],
        "compute_capability": metrics["compute_capability"],
        "driver": command_output(
            "nvidia-smi", "--query-gpu=driver_version", "--format=csv,noheader"
        ),
        "cuda": torch.version.cuda,
        "torch": metrics["torch"],
        "torchao": package_version("torchao"),
        "hqq": package_version("hqq"),
        "python": platform.python_version(),
    }


def run_once(
    runtime: VoxtralInt4Runtime,
    text: str,
    voice: str,
    profile_name: str,
    seed: int,
    wav_path: Path,
) -> dict:
    import torch

    torch.cuda.reset_peak_memory_stats()
    generation_started = time.monotonic()
    generated = runtime.generate(
        text, voice, profile_name, seed, collect_timings=True
    )
    generation_wall_seconds = time.monotonic() - generation_started

    generated.write_wav(wav_path)
    samples, sample_rate = sf.read(wav_path, dtype="float32")
    profile = PROFILES[profile_name]
    metrics = runtime.metrics()
    return {
        "profile": profile_name,
        "flow_steps": profile["flow_steps"],
        "cfg_alpha": profile["cfg_alpha"],
        "compiled": profile["compile"],
        "voice": voice,
        "seed": seed,
        "text": text,
        "character_count": len(text),
        "output_duration_seconds": len(samples) / sample_rate,
        "generation_seconds": generated.generation_seconds,
        "generation_wall_seconds": generation_wall_seconds,
        "time_to_first_audio_seconds": generation_wall_seconds,
        "frames": generated.frame_count,
        "fps": generated.frame_count / generated.generation_seconds,
        "real_time_factor": generation_wall_seconds / generated.duration_seconds,
        "decode_real_time_factor": generated.generation_seconds
        / generated.duration_seconds,
        "timings": generated.timings,
        "peak_allocated_vram_bytes": metrics["peak_allocated_vram_bytes"],
        "sample_rate": sample_rate,
        "channels": 1 if samples.ndim == 1 else samples.shape[1],
        "finite": bool(np.isfinite(samples).all()),
        "peak": float(np.abs(samples).max(initial=0.0)),
        "rms": float(np.sqrt(np.mean(np.square(samples)))),
        "wav_path": str(wav_path),
    }


def suite_markdown(record: dict) -> str:
    lines = [
        "# Voxtral INT4 benchmark suite",
        "",
        f"- git commit: `{record['environment']['git_commit']}`",
        f"- GPU: {record['environment']['gpu']} "
        f"(cc {record['environment']['compute_capability']})",
        f"- torch {record['environment']['torch']}, "
        f"torchao {record['environment']['torchao']}, "
        f"hqq {record['environment']['hqq']}, "
        f"CUDA {record['environment']['cuda']}",
        f"- model load: {record['load_seconds']:.1f} s",
        "",
        "| profile | sentence | frames | FPS | wall RTF | decode RTF |"
        " prefill s | backbone s | acoustic s | loop overhead s | codec s |",
        "|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for run in record["runs"]:
        timings = run["timings"] or {}
        lines.append(
            f"| {run['profile']} | {run['sentence']} | {run['frames']} "
            f"| {run['fps']:.1f} | {run['real_time_factor']:.2f} "
            f"| {run['decode_real_time_factor']:.2f} "
            f"| {timings.get('prefill_seconds', 0.0):.2f} "
            f"| {timings.get('backbone_seconds', 0.0):.2f} "
            f"| {timings.get('acoustic_seconds', 0.0):.2f} "
            f"| {timings.get('loop_overhead_seconds', 0.0):.2f} "
            f"| {timings.get('codec_seconds', 0.0):.2f} |"
        )
    for profile_name, summary in record["profiles"].items():
        lines.extend(
            [
                "",
                f"## {profile_name} aggregate",
                f"- mean FPS: {summary['mean_fps']:.1f}",
                f"- mean wall RTF: {summary['mean_wall_rtf']:.2f}",
                f"- mean decode RTF: {summary['mean_decode_rtf']:.2f}",
                f"- frame-time split: backbone {summary['backbone_share']:.0%}, "
                f"acoustic {summary['acoustic_share']:.0%}, "
                f"loop overhead {summary['overhead_share']:.0%}",
            ]
        )
        if summary.get("compile_warmup_seconds") is not None:
            lines.append(
                f"- one-time compile warmup: "
                f"{summary['compile_warmup_seconds']:.1f} s"
            )
    return "\n".join(lines) + "\n"


def run_suite(args: argparse.Namespace) -> int:
    profile_names = [name.strip() for name in args.profiles.split(",") if name.strip()]
    unknown = sorted(set(profile_names) - set(PROFILES))
    if unknown:
        raise SystemExit(f"unknown profiles: {', '.join(unknown)}")
    # Compiling mutates the model, so compiled profiles must run last.
    profile_names.sort(key=lambda name: PROFILES[name]["compile"])

    started = time.monotonic()
    runtime = VoxtralInt4Runtime(
        args.model_dir, lambda message: print(message, flush=True)
    )
    load_seconds = time.monotonic() - started

    wav_dir = args.output.parent / f"{args.output.stem}-wavs"
    wav_dir.mkdir(parents=True, exist_ok=True)

    runs: list[dict] = []
    profile_summaries: dict[str, dict] = {}
    for profile_name in profile_names:
        compile_warmup_seconds = None
        if PROFILES[profile_name]["compile"] and not runtime.compiled:
            print(f"warming up {profile_name} compilation", flush=True)
            warmup_started = time.monotonic()
            runtime.generate(CORPUS["medium"], args.voice, profile_name, args.seed)
            compile_warmup_seconds = time.monotonic() - warmup_started
        profile_runs = []
        for sentence_name, text in CORPUS.items():
            print(f"[{profile_name}] {sentence_name}", flush=True)
            record = run_once(
                runtime,
                text,
                args.voice,
                profile_name,
                args.seed,
                wav_dir / f"{profile_name}-{sentence_name}.wav",
            )
            record["sentence"] = sentence_name
            runs.append(record)
            profile_runs.append(record)
        decode_total = sum(run["generation_seconds"] for run in profile_runs)
        timing_totals = {
            key: sum((run["timings"] or {}).get(key, 0.0) for run in profile_runs)
            for key in ("backbone_seconds", "acoustic_seconds", "loop_overhead_seconds")
        }
        profile_summaries[profile_name] = {
            "mean_fps": float(np.mean([run["fps"] for run in profile_runs])),
            "mean_wall_rtf": float(
                np.mean([run["real_time_factor"] for run in profile_runs])
            ),
            "mean_decode_rtf": float(
                np.mean([run["decode_real_time_factor"] for run in profile_runs])
            ),
            "backbone_share": timing_totals["backbone_seconds"] / decode_total,
            "acoustic_share": timing_totals["acoustic_seconds"] / decode_total,
            "overhead_share": timing_totals["loop_overhead_seconds"] / decode_total,
            "compile_warmup_seconds": compile_warmup_seconds,
        }

    record = {
        "success": True,
        "suite": True,
        "environment": environment_record(runtime, args.model_dir),
        "load_seconds": load_seconds,
        "voice": args.voice,
        "seed": args.seed,
        "runs": runs,
        "profiles": profile_summaries,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    args.output.with_suffix(".md").write_text(suite_markdown(record), encoding="utf-8")
    print(json.dumps({"profiles": profile_summaries}, indent=2))
    return 0


def run_single(args: argparse.Namespace) -> int:
    started = time.monotonic()
    runtime = VoxtralInt4Runtime(
        args.model_dir, lambda message: print(message, flush=True)
    )
    load_seconds = time.monotonic() - started
    wav_path = args.output.with_suffix(".wav")
    record = run_once(
        runtime, args.text, args.voice, args.profile, args.seed, wav_path
    )
    record = {
        "success": True,
        **environment_record(runtime, args.model_dir),
        "load_seconds": load_seconds,
        **record,
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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="JSON output path")
    parser.add_argument("--profile", choices=sorted(PROFILES), default="compatibility")
    parser.add_argument("--voice", default="neutral_female")
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument(
        "--text",
        default=CORPUS["medium"],
    )
    parser.add_argument(
        "--suite",
        action="store_true",
        help="run the fixed corpus across --profiles with timing breakdown",
    )
    parser.add_argument(
        "--profiles",
        default="compatibility",
        help="comma-separated profiles for --suite (compiled profiles run last)",
    )
    args = parser.parse_args()
    if args.suite:
        return run_suite(args)
    return run_single(args)


if __name__ == "__main__":
    sys.exit(main())
