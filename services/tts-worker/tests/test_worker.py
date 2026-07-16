from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
import wave
from pathlib import Path


class WorkerProtocolTest(unittest.TestCase):
    def test_mock_worker_generates_valid_wav_and_stays_alive(self) -> None:
        root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = Path(temp_dir)
            env = os.environ.copy()
            env["PYTHONPATH"] = str(root)
            with subprocess.Popen(
                [sys.executable, "-m", "audiobookgen_worker.main", "--mock"],
                cwd=root,
                env=env,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                text=True,
                bufsize=1,
            ) as process:
                assert process.stdin and process.stdout
                output = temp / "sample.wav"
                requests = [
                    {"id": "ping", "type": "ping"},
                    {"id": "gen", "type": "generate", "text": "A reliable test sentence.", "voice": "af_heart", "speed": 1.0, "output_path": str(output), "model_dir": str(temp / "model")},
                    {"id": "stop", "type": "shutdown"},
                ]
                for request in requests:
                    process.stdin.write(json.dumps(request) + "\n")
                    process.stdin.flush()
                events = []
                while True:
                    line = process.stdout.readline()
                    if not line:
                        break
                    events.append(json.loads(line))
                    if events[-1].get("id") == "stop":
                        break
                process.wait(timeout=5)
                self.assertEqual(process.returncode, 0)
                self.assertTrue(output.is_file())
                with wave.open(str(output), "rb") as wav:
                    self.assertEqual(wav.getframerate(), 24_000)
                    self.assertEqual(wav.getnchannels(), 1)
                    self.assertGreater(wav.getnframes(), 1_000)
                self.assertTrue(any(event.get("id") == "ping" and event.get("type") == "ready" for event in events))
                complete = next(event for event in events if event.get("id") == "gen" and event.get("type") == "complete")
                timings = complete.get("word_timings")
                self.assertIsInstance(timings, list)
                self.assertEqual(len(timings), 4)
                self.assertEqual(timings[0]["start_ms"], 0)
                self.assertLessEqual(timings[-1]["end_ms"], complete["duration_ms"])


class MultiEngineMockTest(unittest.TestCase):
    def test_mock_worker_serves_all_engines(self) -> None:
        root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = Path(temp_dir)
            env = os.environ.copy()
            env["PYTHONPATH"] = str(root)
            with subprocess.Popen(
                [sys.executable, "-m", "audiobookgen_worker.main", "--mock"],
                cwd=root,
                env=env,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                text=True,
                bufsize=1,
            ) as process:
                assert process.stdin and process.stdout
                requests = [
                    {"id": "caps", "type": "capabilities"},
                    {"id": "dl-maya1", "type": "download_model", "engine": "maya1", "model_dir": str(temp / "maya1")},
                    {"id": "status-maya1", "type": "model_status", "engine": "maya1", "model_dir": str(temp / "maya1")},
                    {"id": "gen-maya1", "type": "generate", "engine": "maya1", "text": "Voice design test.", "voice": "Older male, gravelly", "speed": 1.0, "output_path": str(temp / "maya1.wav"), "model_dir": str(temp / "maya1")},
                    {"id": "gen-voxtral", "type": "generate", "engine": "voxtral", "text": "Preset voice test.", "voice": "narrator_female", "speed": 1.0, "output_path": str(temp / "voxtral.wav"), "model_dir": str(temp / "voxtral")},
                    {"id": "stop", "type": "shutdown"},
                ]
                for request in requests:
                    process.stdin.write(json.dumps(request) + "\n")
                    process.stdin.flush()
                events = []
                while True:
                    line = process.stdout.readline()
                    if not line:
                        break
                    events.append(json.loads(line))
                    if events[-1].get("id") == "stop":
                        break
                process.wait(timeout=5)
            by_id = {event["id"]: event for event in events if event.get("type") == "complete"}
            engine_names = [engine["engine"] for engine in by_id["caps"]["payload"]["engines"]]
            self.assertEqual(engine_names, ["kokoro", "maya1", "voxtral"])
            self.assertTrue(by_id["dl-maya1"]["payload"]["installed"])
            self.assertTrue(by_id["status-maya1"]["payload"]["installed"])
            self.assertTrue((temp / "maya1.wav").is_file())
            self.assertTrue((temp / "voxtral.wav").is_file())
            self.assertGreater(len(by_id["gen-maya1"]["word_timings"]), 0)


if __name__ == "__main__":
    unittest.main()
