"""Hardware-gated Voxtral tests for real CUDA models; never run on mocks."""

import os
import tempfile
import unittest
from pathlib import Path


@unittest.skipUnless(
    os.environ.get("AUDIOBOOKGEN_RUN_VOXTRAL_GPU_TESTS") == "1",
    "set AUDIOBOOKGEN_RUN_VOXTRAL_GPU_TESTS=1 on a supported CUDA host",
)
class VoxtralGpuTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        from audiobookgen_worker.voxtral_int4.runtime import VoxtralInt4Runtime

        model_dir = Path(os.environ["AUDIOBOOKGEN_VOXTRAL_MODEL_DIR"])
        cls.runtime = VoxtralInt4Runtime(model_dir, print)

    def test_real_profiles_cache_reset_and_memory_stability(self) -> None:
        import torch

        text = "The quick brown fox jumps over the lazy dog, and the audiobook begins."
        with tempfile.TemporaryDirectory() as temp_dir:
            first = self.runtime.generate(text, "neutral_female", "compatibility", 0)
            first.write_wav(Path(temp_dir) / "compatibility-first.wav")
            first_allocated = torch.cuda.memory_allocated()
            second = self.runtime.generate(text, "neutral_female", "compatibility", 0)
            second.write_wav(Path(temp_dir) / "compatibility-second.wav")
            repeat_allocated = torch.cuda.memory_allocated()
            quality = self.runtime.generate(text, "neutral_female", "quality", 0)
            quality.write_wav(Path(temp_dir) / "quality.wav")
            balanced = self.runtime.generate(text, "neutral_female", "balanced", 0)
            balanced.write_wav(Path(temp_dir) / "balanced.wav")

            self.assertEqual(first.sample_rate, 48_000)
            self.assertEqual(second.sample_rate, 48_000)
            self.assertEqual(quality.sample_rate, 48_000)
            self.assertEqual(balanced.sample_rate, 48_000)
            self.assertLess(repeat_allocated - first_allocated, 256 * 1024 * 1024)
            self.assertLess(torch.cuda.max_memory_allocated(), 10.5 * 1024**3)


if __name__ == "__main__":
    unittest.main()
