from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from audiobookgen_worker.protocol import ProtocolError, parse_generate


class ProtocolValidationTest(unittest.TestCase):
    def test_rejects_relative_paths(self) -> None:
        with self.assertRaisesRegex(ProtocolError, "absolute"):
            parse_generate(
                {
                    "id": "request",
                    "text": "A sentence.",
                    "voice": "af_heart",
                    "speed": 1.0,
                    "output_path": "relative.wav",
                    "model_dir": "model",
                }
            )

    def test_rejects_unreasonable_speed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ProtocolError, "between 0.5 and 2.0"):
                parse_generate(
                    {
                        "id": "request",
                        "text": "A sentence.",
                        "voice": "af_heart",
                        "speed": 4.0,
                        "output_path": str(root / "sentence.wav"),
                        "model_dir": str(root / "model"),
                    }
                )

    def test_accepts_absolute_generation_request(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            request = parse_generate(
                {
                    "id": "request",
                    "text": "A sentence.",
                    "voice": "af_heart",
                    "speed": 0.95,
                    "output_path": str(root / "sentence.wav"),
                    "model_dir": str(root / "model"),
                }
            )
            self.assertEqual(request.voice, "af_heart")
            self.assertEqual(request.speed, 0.95)


if __name__ == "__main__":
    unittest.main()
