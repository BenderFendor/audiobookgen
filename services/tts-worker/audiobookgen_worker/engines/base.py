"""Shared engine contract and audio helpers for all TTS engines.

Every engine synthesizes one sentence per request into a 24 kHz mono PCM16
WAV file and reports duration plus optional per-word timings. Engines with
heavy dependencies import them lazily so the base worker install stays small.
"""

from __future__ import annotations

import math
import struct
import wave
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Protocol

SAMPLE_RATE = 24_000

# progress(state) or progress(state, current_bytes, total_bytes)
Progress = Callable[..., None]


@dataclass(frozen=True)
class WordTiming:
    word: str
    start_ms: int
    end_ms: int

    def as_dict(self) -> dict[str, object]:
        return {"word": self.word, "start_ms": self.start_ms, "end_ms": self.end_ms}


@dataclass(frozen=True)
class GenerateResult:
    duration_ms: int
    sample_rate: int = SAMPLE_RATE
    word_timings: list[WordTiming] = field(default_factory=list)


class Engine(Protocol):
    name: str

    def capabilities(self) -> dict[str, object]: ...

    def installed(
        self, model_dir: Path, options: dict[str, object]
    ) -> dict[str, object]: ...

    def ensure_model(
        self, model_dir: Path, options: dict[str, object], progress: Progress
    ) -> dict[str, object]: ...

    def generate(
        self,
        text: str,
        voice: str,
        speed: float,
        output_path: Path,
        model_dir: Path,
        options: dict[str, object],
        progress: Progress,
    ) -> GenerateResult: ...


def trim_silence(audio, sample_rate: int = SAMPLE_RATE):
    """Trim leading/trailing silence; returns (audio, start_offset_samples)."""
    import numpy as np

    threshold = max(float(np.max(np.abs(audio))) * 0.01, 0.0005)
    active = np.flatnonzero(np.abs(audio) > threshold)
    if len(active) == 0:
        return audio, 0
    padding = round(sample_rate * 0.045)
    start = max(0, int(active[0]) - padding)
    end = min(len(audio), int(active[-1]) + padding + 1)
    return audio[start:end], start


def estimated_word_timings(text: str, duration_ms: int) -> list[WordTiming]:
    """Length-proportional fallback timings for engines without alignment."""
    words = [word for word in text.split() if word]
    if not words or duration_ms <= 0:
        return []
    weights = [len(word) + 2 for word in words]
    total = sum(weights)
    timings: list[WordTiming] = []
    cursor = 0.0
    for word, weight in zip(words, weights):
        start = cursor
        cursor += duration_ms * weight / total
        timings.append(
            WordTiming(word=word, start_ms=round(start), end_ms=round(cursor))
        )
    return timings


def write_mock_wave(text: str, speed: float, output_path: Path) -> int:
    """Deterministic tone used by --mock mode and tests."""
    duration = max(0.18, min(4.0, len(text.split()) * 0.16 / max(speed, 0.1)))
    frames = round(duration * SAMPLE_RATE)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(output_path), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(SAMPLE_RATE)
        for index in range(frames):
            envelope = min(1.0, index / 240) * min(1.0, (frames - index) / 240)
            value = int(
                2_400 * envelope * math.sin(2 * math.pi * 220 * index / SAMPLE_RATE)
            )
            wav.writeframesraw(struct.pack("<h", value))
    return round(frames / SAMPLE_RATE * 1000)


def mock_result(text: str, speed: float, output_path: Path) -> GenerateResult:
    duration_ms = write_mock_wave(text, speed, output_path)
    return GenerateResult(
        duration_ms=duration_ms,
        word_timings=estimated_word_timings(text, duration_ms),
    )


def directory_size(path: Path) -> int:
    total = 0
    for entry in path.rglob("*"):
        try:
            if entry.is_file():
                total += entry.stat().st_size
        except OSError:
            continue
    return total


def download_with_progress(
    repo_id: str,
    model_dir: Path,
    progress: Progress,
    filename: str | None = None,
    revision: str | None = None,
) -> None:
    """Hugging Face download with byte-level progress.

    Total size comes from the repo metadata; downloaded bytes are measured by
    polling the target directory (partial files included), which works for
    both single files and full snapshots without hooking tqdm internals.
    """
    import threading

    from huggingface_hub import HfApi, hf_hub_download, snapshot_download

    total = 0
    try:
        info = HfApi().model_info(repo_id, revision=revision, files_metadata=True)
        for sibling in info.siblings or []:
            if sibling.size and (filename is None or sibling.rfilename == filename):
                total += sibling.size
    except Exception:
        total = 0

    already = directory_size(model_dir) if model_dir.exists() else 0
    done = threading.Event()

    def poll() -> None:
        while not done.wait(2.0):
            if total:
                current = max(0, directory_size(model_dir) - already)
                progress("downloading", min(current, total), total)

    watcher = threading.Thread(target=poll, daemon=True)
    watcher.start()
    try:
        if filename is None:
            snapshot_download(repo_id=repo_id, revision=revision, local_dir=model_dir)
        else:
            hf_hub_download(
                repo_id=repo_id,
                filename=filename,
                revision=revision,
                local_dir=model_dir,
            )
    finally:
        done.set()
        watcher.join(timeout=1.0)
    if total:
        progress("downloading", total, total)


def configure_hf_cache(model_dir: Path) -> None:
    """Keep Hugging Face caches next to the models so everything lands on the
    storage volume the user picked instead of the home directory."""
    import os

    cache_root = model_dir.parent
    os.environ.setdefault("HF_HOME", str(cache_root / "huggingface"))
    os.environ.setdefault("HF_HUB_CACHE", str(cache_root / "huggingface" / "hub"))
