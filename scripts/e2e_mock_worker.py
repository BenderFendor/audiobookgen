#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKER = ROOT / "services" / "tts-worker"

with tempfile.TemporaryDirectory() as temp_dir:
    temp = Path(temp_dir)
    output = temp / "chapter.wav"
    env = os.environ.copy()
    env["PYTHONPATH"] = str(WORKER)
    process = subprocess.Popen([sys.executable, "-m", "audiobookgen_worker.main", "--mock"], cwd=WORKER, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
    assert process.stdin and process.stdout
    for payload in [
        {"id":"1","type":"ping"},
        {"id":"2","type":"generate","text":"The first chapter begins here.","voice":"af_heart","speed":1.0,"output_path":str(output),"model_dir":str(temp / "model")},
        {"id":"3","type":"shutdown"},
    ]:
        process.stdin.write(json.dumps(payload) + "\n")
        process.stdin.flush()
    events = []
    while True:
        line = process.stdout.readline()
        if not line:
            break
        events.append(json.loads(line))
        if events[-1].get("id") == "3":
            break
    process.wait(timeout=5)
    if process.returncode != 0 or not output.exists():
        raise SystemExit("mock worker failed")
    with wave.open(str(output), "rb") as wav:
        if wav.getframerate() != 24_000 or wav.getnframes() < 1000:
            raise SystemExit("invalid generated WAV")
    print(f"Mock worker E2E passed: {output.stat().st_size} byte WAV, {len(events)} protocol events.")
