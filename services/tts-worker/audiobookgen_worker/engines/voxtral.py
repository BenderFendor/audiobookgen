"""Voxtral 4B TTS engine: client for a vLLM-Omni /v1/audio/speech server.

Mistral's Voxtral-4B-TTS-2603 is only supported through vLLM-Omni, which is
far too heavy to embed in the worker venv. This engine downloads the weights
to the models volume and synthesizes through an OpenAI-compatible audio
endpoint. The Models page shows the exact serve command; an already-running
server (local or remote) works too via the server URL setting.
"""

from __future__ import annotations

import json
import urllib.error
import urllib.request
from pathlib import Path

from .base import (
    SAMPLE_RATE,
    GenerateResult,
    Progress,
    configure_hf_cache,
    estimated_word_timings,
    mock_result,
    trim_silence,
)

MODEL_REPO = "mistralai/Voxtral-4B-TTS-2603"
DEFAULT_SERVER_URL = "http://127.0.0.1:8570"
# Preset speakers documented for Voxtral TTS; free-form voice names are passed
# through so new presets keep working without an app update.
PRESET_VOICES = (
    "casual_male", "casual_female", "calm_male", "calm_female",
    "narrator_male", "narrator_female", "upbeat_male", "upbeat_female",
)


class VoxtralEngine:
    name = "voxtral"

    def __init__(self, mock: bool = False) -> None:
        self.mock = mock

    def capabilities(self) -> dict[str, object]:
        return {
            "engine": self.name,
            "sample_rate": SAMPLE_RATE,
            "languages": ["en", "fr", "es", "de", "it", "pt", "nl", "ar", "hi"],
            "voices": list(PRESET_VOICES),
            "voice_style": "preset",
            "word_timings": False,
            "supports_speed": True,
            "requires_server": True,
            "mock": self.mock,
        }

    def installed(self, model_dir: Path, options: dict[str, object]) -> dict[str, object]:
        weights_ready = model_dir.joinpath("config.json").is_file() or model_dir.joinpath(
            "MOCK_MODEL"
        ).is_file()
        server_ready = self.mock or self._server_reachable(self._server_url(options))
        return {
            "installed": weights_ready,
            "path": str(model_dir),
            "server_url": self._server_url(options),
            "server_reachable": server_ready,
        }

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
            repo_id=MODEL_REPO,
            local_dir=model_dir,
            local_dir_use_symlinks=False,
        )
        return {
            "installed": True,
            "path": str(model_dir),
            "server_url": self._server_url(options),
            "mock": False,
        }

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
        output_path.parent.mkdir(parents=True, exist_ok=True)
        if self.mock:
            progress("synthesizing")
            return mock_result(text, speed, output_path)

        server_url = self._server_url(options)
        progress("synthesizing")
        payload = json.dumps({
            "model": str(options.get("served_model", MODEL_REPO)),
            "input": text,
            "voice": voice.strip() or "narrator_male",
            "response_format": "wav",
            "speed": speed,
        }).encode("utf-8")
        request = urllib.request.Request(
            f"{server_url.rstrip('/')}/v1/audio/speech",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(request, timeout=600) as response:
                audio_bytes = response.read()
        except urllib.error.URLError as error:
            raise RuntimeError(
                f"Voxtral server is not reachable at {server_url}. Start it from the "
                f"Models page command or fix the server URL. ({error})"
            ) from error

        import io

        import numpy as np
        import soundfile as sf

        audio, sample_rate = sf.read(io.BytesIO(audio_bytes), dtype="float32", always_2d=False)
        if getattr(audio, "ndim", 1) > 1:
            audio = audio.mean(axis=1)
        audio, _ = trim_silence(np.asarray(audio, dtype=np.float32), sample_rate)
        progress("writing")
        sf.write(output_path, audio, sample_rate, subtype="PCM_16")
        duration_ms = round(len(audio) / sample_rate * 1000)
        return GenerateResult(
            duration_ms=duration_ms,
            sample_rate=sample_rate,
            word_timings=estimated_word_timings(text, duration_ms),
        )

    @staticmethod
    def _server_url(options: dict[str, object]) -> str:
        return str(options.get("server_url") or DEFAULT_SERVER_URL)

    @staticmethod
    def _server_reachable(server_url: str) -> bool:
        request = urllib.request.Request(f"{server_url.rstrip('/')}/v1/models", method="GET")
        try:
            with urllib.request.urlopen(request, timeout=2):
                return True
        except OSError:
            return False
