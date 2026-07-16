#!/usr/bin/env python3
"""Exercise real Voxtral generation through the production JSONL worker protocol."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
import wave
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--python", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--profile", default="compatibility")
    args = parser.parse_args()
    worker_root = Path(__file__).resolve().parents[1] / "services/tts-worker"
    env = os.environ.copy()
    env["PYTHONPATH"] = str(worker_root)
    with tempfile.TemporaryDirectory() as temporary:
        output = Path(temporary) / "voxtral-worker.wav"
        with subprocess.Popen(
            [str(args.python), "-m", "audiobookgen_worker.main"],
            cwd=worker_root,
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            bufsize=1,
        ) as process:
            assert process.stdin and process.stdout
            request = {
                "id": "voxtral-e2e",
                "type": "generate",
                "engine": "voxtral",
                "text": "The quick brown fox jumps over the lazy dog, and the audiobook begins.",
                "voice": "neutral_female",
                "speed": 1.0,
                "output_path": str(output),
                "model_dir": str(args.model_dir),
                "options": {"profile": args.profile, "seed": 0},
            }
            process.stdin.write(json.dumps(request) + "\n")
            process.stdin.flush()
            complete = None
            for line in process.stdout:
                event = json.loads(line)
                print(json.dumps(event), flush=True)
                if event.get("id") != request["id"]:
                    continue
                if event.get("type") == "error":
                    raise RuntimeError(f"worker failed: {event}")
                if event.get("type") == "complete":
                    complete = event
                    break
            process.stdin.write(json.dumps({"id": "stop", "type": "shutdown"}) + "\n")
            process.stdin.flush()
            process.wait(timeout=30)
        if complete is None or not output.is_file():
            raise RuntimeError("worker did not produce a completed WAV")
        with wave.open(str(output), "rb") as wav:
            if wav.getframerate() != 48_000 or wav.getnchannels() != 1:
                raise RuntimeError("worker WAV is not mono 48 kHz")
            duration = wav.getnframes() / wav.getframerate()
            if duration <= 0:
                raise RuntimeError("worker WAV is empty")
            if complete.get("sample_rate") != 48_000:
                raise RuntimeError("worker response lost the 48 kHz sample rate")
        print(f"Real Voxtral worker E2E passed: {duration:.3f}s mono 48 kHz")
    return 0


if __name__ == "__main__":
    sys.exit(main())
