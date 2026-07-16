#!/usr/bin/env python3
"""Report duration, RMS level, and peak level for generated PCM WAV files."""

from __future__ import annotations

import argparse
from array import array
import math
from pathlib import Path
import sys
import wave


def decibels(value: float) -> float:
    return 20 * math.log10(value / 32767) if value > 0 else float("-inf")


def analyze(path: Path) -> tuple[float, float, float]:
    with wave.open(str(path), "rb") as source:
        if source.getsampwidth() != 2:
            raise ValueError(f"expected 16-bit PCM, found {source.getsampwidth() * 8}-bit audio")
        frames = source.readframes(source.getnframes())
        samples = array("h", frames)
        if sys.byteorder != "little":
            samples.byteswap()
        if not samples:
            return 0.0, float("-inf"), float("-inf")
        duration = source.getnframes() / source.getframerate()
        peak = max(abs(sample) for sample in samples)
        rms = math.sqrt(sum(sample * sample for sample in samples) / len(samples))
        return duration, decibels(rms), decibels(peak)


def default_files() -> list[Path]:
    cache = Path.home() / ".local/share/io.audiobookgen.desktop/cache/segments"
    return sorted(cache.glob("*.wav"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("files", nargs="*", type=Path, help="WAV files to inspect; defaults to the Linux AudiobookGen sentence cache")
    parser.add_argument("--require-signal", action="store_true", help="fail if every inspected file is silent")
    args = parser.parse_args()
    files = args.files or default_files()
    if not files:
        parser.error("no WAV files found")

    signaled = 0
    failed = 0
    for path in files:
        try:
            duration, rms, peak = analyze(path)
        except (OSError, ValueError, wave.Error) as error:
            failed += 1
            print(f"ERROR {path}: {error}")
            continue
        status = "SILENT" if math.isinf(peak) else "LOW" if peak < -30 else "OK"
        if status != "SILENT":
            signaled += 1
        print(f"{status:6} duration={duration:7.3f}s rms={rms:6.1f} dBFS peak={peak:6.1f} dBFS {path}")

    if failed or (args.require_signal and not signaled):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
