"""Typed 48 kHz audio post-processing and serialization."""

from dataclasses import dataclass
from pathlib import Path

import numpy as np


@dataclass(frozen=True)
class GeneratedAudio:
    samples: np.ndarray
    sample_rate: int
    generation_seconds: float
    frame_count: int
    text: str
    voice: str
    engine_profile: str
    timings: dict | None = None

    @property
    def duration_seconds(self) -> float:
        return len(self.samples) / self.sample_rate

    def write_wav(self, output_path: Path) -> None:
        import wave

        pcm = (np.clip(self.samples, -1.0, 1.0) * 32_767).astype("<i2")
        with wave.open(str(output_path), "wb") as wav:
            wav.setnchannels(1)
            wav.setsampwidth(2)
            wav.setframerate(self.sample_rate)
            wav.writeframes(pcm.tobytes())


def postprocess_audio(
    samples: np.ndarray,
    input_sample_rate: int = 24_000,
    output_sample_rate: int = 48_000,
    target_peak: float = 0.95,
) -> tuple[np.ndarray, int]:
    from scipy.signal import butter, resample_poly, sosfilt

    if samples.size < 2:
        return samples.astype(np.float32), output_sample_rate
    filter_sos = butter(6, 10_000, btype="low", fs=input_sample_rate, output="sos")
    filtered = sosfilt(filter_sos, samples).astype(np.float32)
    if output_sample_rate != input_sample_rate:
        divisor = np.gcd(output_sample_rate, input_sample_rate)
        filtered = resample_poly(
            filtered,
            up=output_sample_rate // divisor,
            down=input_sample_rate // divisor,
        ).astype(np.float32)
    peak = float(np.abs(filtered).max(initial=0.0))
    if peak > 1e-6:
        filtered *= target_peak / peak
    return filtered, output_sample_rate


def trim_warmup_frames(frames: list) -> list:
    if len(frames) <= 2:
        return frames
    first_code = frames[0][0, 0].item()
    if frames[1][0, 0].item() != first_code:
        return frames
    for index in range(2, min(len(frames), 30)):
        if frames[index][0, 0].item() != first_code:
            return frames[index:]
    return frames
