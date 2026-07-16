"""Direct selective-HQQ-INT4 Voxtral engine for 12 GB NVIDIA GPUs."""

from __future__ import annotations

import hashlib
from pathlib import Path
from typing import Any

from .base import (
    GenerateResult,
    Progress,
    configure_hf_cache,
    download_with_progress,
    estimated_word_timings,
    mock_result,
)

MODEL_REPO = "mistralai/Voxtral-4B-TTS-2603"
MODEL_REVISION = "b81be46c3777f88621676791b512bb01dc1cb970"
EXPECTED_WEIGHTS_SHA256 = (
    "66c4fd998db10e1a6d9cc5baa10e6264bf10701ec22ccdc0822c7dcc45dbe55b"
)


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(8 * 1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


class VoxtralEngine:
    name = "voxtral"

    def __init__(self, mock: bool = False) -> None:
        self.mock = mock
        self._runtime: Any | None = None

    def capabilities(self) -> dict[str, object]:
        return {
            "engine": self.name,
            "sample_rate": 48_000,
            "languages": ["en", "fr", "es", "de", "it", "pt", "nl", "ar", "hi"],
            "voices": [],
            "voice_style": "installed_presets",
            "word_timings": False,
            "supports_speed": False,
            "requires_server": False,
            "profiles": ["balanced", "quality", "compatibility"],
            "mock": self.mock,
        }

    def installed(
        self, model_dir: Path, options: dict[str, object]
    ) -> dict[str, object]:
        ready = (
            model_dir.joinpath("consolidated.safetensors").is_file()
            and model_dir.joinpath("tekken.json").is_file()
            and model_dir.joinpath("voice_embedding").is_dir()
        ) or model_dir.joinpath("MOCK_MODEL").is_file()
        voices = (
            sorted(
                path.stem for path in model_dir.joinpath("voice_embedding").glob("*.pt")
            )
            if ready
            else []
        )
        return {"installed": ready, "path": str(model_dir), "voices": voices}

    def ensure_model(
        self, model_dir: Path, options: dict[str, object], progress: Progress
    ) -> dict[str, object]:
        model_dir.mkdir(parents=True, exist_ok=True)
        if self.mock:
            (model_dir / "MOCK_MODEL").write_text("mock", encoding="utf-8")
            return {"installed": True, "path": str(model_dir), "mock": True}
        configure_hf_cache(model_dir)
        progress("downloading")
        download_with_progress(MODEL_REPO, model_dir, progress, revision=MODEL_REVISION)
        progress("verifying-checksum")
        actual = file_sha256(model_dir / "consolidated.safetensors")
        if actual != EXPECTED_WEIGHTS_SHA256:
            raise RuntimeError(
                f"Voxtral weights checksum mismatch: expected {EXPECTED_WEIGHTS_SHA256}, got {actual}"
            )
        return self.installed(model_dir, options)

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
            return mock_result(text, speed, output_path)
        if speed != 1.0:
            raise ValueError("Voxtral INT4 currently supports speed 1.0 only")
        if self._runtime is None:
            progress("loading-int4")
            from audiobookgen_worker.voxtral_int4.runtime import VoxtralInt4Runtime

            self._runtime = VoxtralInt4Runtime(model_dir, progress)
        progress("synthesizing")
        profile = str(options.get("profile") or "balanced")
        seed = int(options.get("seed") or 0)
        if profile == "balanced" and not self._runtime.compiled:
            progress("compiling-balanced")
        generated = self._runtime.generate(text, voice, profile, seed)
        progress("writing")
        generated.write_wav(output_path)
        duration_ms = round(generated.duration_seconds * 1000)
        return GenerateResult(
            duration_ms=duration_ms,
            sample_rate=generated.sample_rate,
            word_timings=estimated_word_timings(text, duration_ms),
        )
