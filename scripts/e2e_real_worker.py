#!/usr/bin/env python3
"""End-to-end test of the real Kokoro worker.

Uses the app's managed venv and downloaded model when they exist, so it
exercises the exact runtime path the desktop app uses (imports included).
Skips with exit 0 when the venv or model is absent (e.g. in CI), so the
mock E2E remains the portable check. Override locations with
AUDIOBOOKGEN_PYTHON and AUDIOBOOKGEN_MODEL_DIR.
"""
from __future__ import annotations

import json
import os
import subprocess
import tempfile
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKER = ROOT / "services" / "tts-worker"
APP_DATA = Path.home() / ".local/share/io.audiobookgen.desktop"

python = Path(os.environ.get("AUDIOBOOKGEN_PYTHON", APP_DATA / "worker-venv/bin/python"))
model_dir = Path(os.environ.get("AUDIOBOOKGEN_MODEL_DIR", APP_DATA / "models/kokoro-82m"))

if not python.exists():
    print(f"SKIP: managed worker Python not found at {python}")
    raise SystemExit(0)
if not (model_dir / "config.json").exists() and not (model_dir / "MOCK_MODEL").exists():
    print(f"SKIP: Kokoro model not found at {model_dir}")
    raise SystemExit(0)

with tempfile.TemporaryDirectory() as temp_dir:
    output = Path(temp_dir) / "sentence.wav"
    process = subprocess.Popen(
        [str(python), "-m", "audiobookgen_worker.main"],
        cwd=WORKER,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    assert process.stdin and process.stdout
    requests = [
        {"id": "1", "type": "ping"},
        {
            "id": "2",
            "type": "generate",
            "text": "AudiobookGen end to end narration check.",
            "voice": "af_heart",
            "speed": 1.0,
            "output_path": str(output),
            "model_dir": str(model_dir),
        },
        {"id": "3", "type": "shutdown"},
    ]
    for payload in requests:
        process.stdin.write(json.dumps(payload) + "\n")
        process.stdin.flush()
    events: list[dict[str, object]] = []
    failure: str | None = None
    while True:
        line = process.stdout.readline()
        if not line:
            break
        event = json.loads(line)
        events.append(event)
        if event.get("type") == "error":
            failure = str(event.get("message"))
            break
        if event.get("id") == "3":
            break
    process.wait(timeout=120)
    if failure:
        raise SystemExit(f"real worker returned an error: {failure}")
    complete = next((e for e in events if e.get("id") == "2" and e.get("type") == "complete"), None)
    if complete is None or not output.exists():
        raise SystemExit(f"real worker did not complete generation; events: {events}")
    with wave.open(str(output), "rb") as wav:
        frames, rate = wav.getnframes(), wav.getframerate()
    if rate != 24_000 or frames < 4_000:
        raise SystemExit(f"generated WAV looks wrong: {rate} Hz, {frames} frames")
    duration = complete.get("duration_ms")
    print(f"Real worker E2E passed: {frames} frames at {rate} Hz, reported {duration} ms.")
