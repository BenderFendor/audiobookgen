from __future__ import annotations

import math
import os
import struct
import wave
from pathlib import Path
from typing import Callable

SAMPLE_RATE = 24_000
VOICES = ("af_heart", "af_bella", "af_nicole", "am_adam", "am_michael", "bf_emma", "bm_george")


class KokoroEngine:
    def __init__(self, mock: bool = False) -> None:
        self.mock = mock
        self._pipeline = None
        self._model_dir: Path | None = None
        self._lang_code: str | None = None

    def capabilities(self) -> dict[str, object]:
        return {
            "engine": "kokoro",
            "sample_rate": SAMPLE_RATE,
            "languages": ["en-us", "en-gb"],
            "voices": list(VOICES),
            "mock": self.mock,
        }

    def ensure_model(self, model_dir: Path) -> dict[str, object]:
        model_dir.mkdir(parents=True, exist_ok=True)
        if self.mock:
            (model_dir / "MOCK_MODEL").write_text("mock", encoding="utf-8")
            return {"installed": True, "path": str(model_dir), "mock": True}
        self._configure_cache(model_dir)
        from huggingface_hub import snapshot_download

        snapshot_download(
            repo_id="hexgrad/Kokoro-82M",
            local_dir=model_dir,
            local_dir_use_symlinks=False,
        )
        return {"installed": True, "path": str(model_dir), "mock": False}

    def generate(
        self,
        text: str,
        voice: str,
        speed: float,
        output_path: Path,
        model_dir: Path,
        progress: Callable[[str], None],
    ) -> tuple[int, int]:
        if voice not in VOICES:
            raise ValueError(f"unsupported Kokoro voice: {voice}")
        output_path.parent.mkdir(parents=True, exist_ok=True)
        if self.mock:
            progress("synthesizing")
            return self._write_mock_wave(text, speed, output_path), SAMPLE_RATE

        progress("loading")
        pipeline = self._get_pipeline(model_dir, voice)
        voice_path = model_dir / "voices" / f"{voice}.pt"
        if not voice_path.is_file():
            raise RuntimeError(f"Kokoro voice is missing from the installed model: {voice}")
        progress("phonemizing")
        chunks = []
        import numpy as np
        import soundfile as sf

        for _graphemes, _phonemes, audio in pipeline(text, voice=str(voice_path), speed=speed, split_pattern=r"\n+"):
            chunks.append(np.asarray(audio, dtype=np.float32))
        if not chunks:
            raise RuntimeError("Kokoro produced no audio")
        progress("writing")
        audio = np.concatenate(chunks)
        audio = self._trim_silence(audio)
        sf.write(output_path, audio, SAMPLE_RATE, subtype="PCM_16")
        return round(len(audio) / SAMPLE_RATE * 1000), SAMPLE_RATE

    def _get_pipeline(self, model_dir: Path, voice: str):
        lang_code = "b" if voice.startswith("b") else "a"
        if self._pipeline is None or self._model_dir != model_dir or self._lang_code != lang_code:
            self._configure_cache(model_dir)
            from kokoro import KModel, KPipeline

            config = model_dir / "config.json"
            weights = model_dir / "kokoro-v1_0.pth"
            if not config.is_file() or not weights.is_file():
                raise RuntimeError("Kokoro model files are incomplete; download the model again")
            model = KModel(repo_id="hexgrad/Kokoro-82M", config=str(config), model=str(weights))
            self._pipeline = KPipeline(lang_code=lang_code, repo_id="hexgrad/Kokoro-82M", model=model)
            self._model_dir = model_dir
            self._lang_code = lang_code
        return self._pipeline

    @staticmethod
    def _configure_cache(model_dir: Path) -> None:
        cache_root = model_dir.parent
        os.environ.setdefault("HF_HOME", str(cache_root / "huggingface"))
        os.environ.setdefault("HF_HUB_CACHE", str(cache_root / "huggingface" / "hub"))

    @staticmethod
    def _trim_silence(audio):
        import numpy as np

        threshold = max(float(np.max(np.abs(audio))) * 0.01, 0.0005)
        active = np.flatnonzero(np.abs(audio) > threshold)
        if len(active) == 0:
            return audio
        padding = round(SAMPLE_RATE * 0.045)
        start = max(0, int(active[0]) - padding)
        end = min(len(audio), int(active[-1]) + padding + 1)
        return audio[start:end]

    @staticmethod
    def _write_mock_wave(text: str, speed: float, output_path: Path) -> int:
        duration = max(0.18, min(4.0, len(text.split()) * 0.16 / speed))
        frames = round(duration * SAMPLE_RATE)
        with wave.open(str(output_path), "wb") as wav:
            wav.setnchannels(1)
            wav.setsampwidth(2)
            wav.setframerate(SAMPLE_RATE)
            for index in range(frames):
                envelope = min(1.0, index / 240) * min(1.0, (frames - index) / 240)
                value = int(2_400 * envelope * math.sin(2 * math.pi * 220 * index / SAMPLE_RATE))
                wav.writeframesraw(struct.pack("<h", value))
        return round(frames / SAMPLE_RATE * 1000)
