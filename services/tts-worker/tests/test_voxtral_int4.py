"""CPU regressions for the repaired Voxtral INT4 integration."""

import tempfile
import unittest
import wave
from pathlib import Path

import numpy as np

from audiobookgen_worker.voxtral_int4.audio import GeneratedAudio, postprocess_audio
from audiobookgen_worker.voxtral_int4.runtime import PROFILES
from audiobookgen_worker.protocol import ProtocolError, parse_generate


class VoxtralAudioContractTest(unittest.TestCase):
    def test_postprocessing_propagates_48khz_rate_and_duration(self) -> None:
        source = np.sin(np.linspace(0, 2 * np.pi * 440, 24_000, endpoint=False)).astype(
            np.float32
        )
        processed, sample_rate = postprocess_audio(source)
        generated = GeneratedAudio(
            processed, sample_rate, 0.5, 13, "test", "voice", "balanced"
        )

        self.assertEqual(sample_rate, 48_000)
        self.assertEqual(len(processed), 48_000)
        self.assertAlmostEqual(generated.duration_seconds, 1.0, places=3)

        with tempfile.TemporaryDirectory() as temp_dir:
            output = Path(temp_dir) / "sample.wav"
            generated.write_wav(output)
            with wave.open(str(output), "rb") as wav:
                self.assertEqual(wav.getframerate(), 48_000)
                self.assertEqual(wav.getnframes(), 48_000)

    def test_every_production_profile_keeps_quality_safe_cfg(self) -> None:
        self.assertGreater(len(PROFILES), 0)
        for name, profile in PROFILES.items():
            with self.subTest(profile=name):
                self.assertGreaterEqual(profile["cfg_alpha"], 1.2)

    def test_worker_rejects_parent_path_traversal(self) -> None:
        with self.assertRaisesRegex(ProtocolError, "parent traversal"):
            parse_generate(
                {
                    "id": "unsafe",
                    "engine": "voxtral",
                    "text": "test",
                    "voice": "neutral_female",
                    "speed": 1.0,
                    "output_path": "/tmp/cache/../../etc/audio.wav",
                    "model_dir": "/tmp/model",
                }
            )
