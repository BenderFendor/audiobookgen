"""Kokoro-82M engine: fast local narration with per-word timestamps.

Kokoro's misaki G2P reports start/end timestamps per token for English, which
this engine converts into word timings aligned to the trimmed audio.
"""

from __future__ import annotations

from pathlib import Path

from .base import (
    SAMPLE_RATE,
    GenerateResult,
    Progress,
    WordTiming,
    configure_hf_cache,
    estimated_word_timings,
    mock_result,
    trim_silence,
)

REPO_ID = "hexgrad/Kokoro-82M"

# English voices shipped with Kokoro v1.0 (American af_/am_, British bf_/bm_).
VOICES = (
    "af_alloy", "af_aoede", "af_bella", "af_heart", "af_jessica", "af_kore",
    "af_nicole", "af_nova", "af_river", "af_sarah", "af_sky",
    "am_adam", "am_echo", "am_eric", "am_fenrir", "am_liam", "am_michael",
    "am_onyx", "am_puck", "am_santa",
    "bf_alice", "bf_emma", "bf_isabella", "bf_lily",
    "bm_daniel", "bm_fable", "bm_george", "bm_lewis",
)


class KokoroEngine:
    name = "kokoro"

    def __init__(self, mock: bool = False) -> None:
        self.mock = mock
        self._pipeline = None
        self._model_dir: Path | None = None
        self._lang_code: str | None = None

    def capabilities(self) -> dict[str, object]:
        return {
            "engine": self.name,
            "sample_rate": SAMPLE_RATE,
            "languages": ["en-us", "en-gb"],
            "voices": list(VOICES),
            "word_timings": True,
            "supports_speed": True,
            "mock": self.mock,
        }

    def installed(self, model_dir: Path, options: dict[str, object]) -> dict[str, object]:
        ready = (
            model_dir.joinpath("config.json").is_file()
            and model_dir.joinpath("kokoro-v1_0.pth").is_file()
        ) or model_dir.joinpath("MOCK_MODEL").is_file()
        return {"installed": ready, "path": str(model_dir)}

    def ensure_model(
        self, model_dir: Path, options: dict[str, object], progress: Progress
    ) -> dict[str, object]:
        model_dir.mkdir(parents=True, exist_ok=True)
        if self.mock:
            (model_dir / "MOCK_MODEL").write_text("mock", encoding="utf-8")
            return {"installed": True, "path": str(model_dir), "mock": True}
        configure_hf_cache(model_dir)
        from huggingface_hub import snapshot_download

        progress("downloading")
        snapshot_download(
            repo_id=REPO_ID,
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
        options: dict[str, object],
        progress: Progress,
    ) -> GenerateResult:
        if voice not in VOICES:
            raise ValueError(f"unsupported Kokoro voice: {voice}")
        output_path.parent.mkdir(parents=True, exist_ok=True)
        if self.mock:
            progress("synthesizing")
            return mock_result(text, speed, output_path)

        progress("loading")
        pipeline = self._get_pipeline(model_dir, voice)
        voice_path = model_dir / "voices" / f"{voice}.pt"
        if not voice_path.is_file():
            raise RuntimeError(f"Kokoro voice is missing from the installed model: {voice}")
        progress("phonemizing")
        import numpy as np
        import soundfile as sf

        chunks = []
        timings: list[WordTiming] = []
        offset_seconds = 0.0
        for result in pipeline(text, voice=str(voice_path), speed=speed, split_pattern=r"\n+"):
            audio = np.asarray(result.audio, dtype=np.float32)
            for token in getattr(result, "tokens", None) or []:
                start = getattr(token, "start_ts", None)
                end = getattr(token, "end_ts", None)
                word = (getattr(token, "text", "") or "").strip()
                if start is None or end is None or not word:
                    continue
                timings.append(WordTiming(
                    word=word,
                    start_ms=round((offset_seconds + float(start)) * 1000),
                    end_ms=round((offset_seconds + float(end)) * 1000),
                ))
            chunks.append(audio)
            offset_seconds += len(audio) / SAMPLE_RATE
        if not chunks:
            raise RuntimeError("Kokoro produced no audio")
        progress("writing")
        audio = np.concatenate(chunks)
        audio, trimmed_start = trim_silence(audio)
        duration_ms = round(len(audio) / SAMPLE_RATE * 1000)
        shift_ms = round(trimmed_start / SAMPLE_RATE * 1000)
        timings = [
            WordTiming(
                word=timing.word,
                start_ms=max(0, timing.start_ms - shift_ms),
                end_ms=min(duration_ms, max(0, timing.end_ms - shift_ms)),
            )
            for timing in timings
        ]
        timings = [timing for timing in timings if timing.end_ms > timing.start_ms]
        if not timings:
            timings = estimated_word_timings(text, duration_ms)
        sf.write(output_path, audio, SAMPLE_RATE, subtype="PCM_16")
        return GenerateResult(duration_ms=duration_ms, word_timings=timings)

    def _get_pipeline(self, model_dir: Path, voice: str):
        lang_code = "b" if voice.startswith("b") else "a"
        if self._pipeline is None or self._model_dir != model_dir or self._lang_code != lang_code:
            configure_hf_cache(model_dir)
            from kokoro import KModel, KPipeline

            config = model_dir / "config.json"
            weights = model_dir / "kokoro-v1_0.pth"
            if not config.is_file() or not weights.is_file():
                raise RuntimeError("Kokoro model files are incomplete; download the model again")
            model = KModel(repo_id=REPO_ID, config=str(config), model=str(weights))
            self._pipeline = KPipeline(lang_code=lang_code, repo_id=REPO_ID, model=model)
            self._model_dir = model_dir
            self._lang_code = lang_code
        return self._pipeline
