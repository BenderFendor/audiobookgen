"""Engine registry: one lazily constructed engine instance per name."""

from __future__ import annotations

from .base import Engine, GenerateResult, WordTiming
from .kokoro import KokoroEngine
from .maya1 import Maya1Engine
from .voxtral import VoxtralEngine

ENGINE_NAMES = ("kokoro", "maya1", "voxtral")

_CONSTRUCTORS = {
    "kokoro": KokoroEngine,
    "maya1": Maya1Engine,
    "voxtral": VoxtralEngine,
}


class EngineRegistry:
    def __init__(self, mock: bool = False) -> None:
        self.mock = mock
        self._engines: dict[str, Engine] = {}

    def get(self, name: str) -> Engine:
        if name not in _CONSTRUCTORS:
            raise ValueError(f"unknown engine: {name!r}")
        if name not in self._engines:
            self._engines[name] = _CONSTRUCTORS[name](mock=self.mock)
        return self._engines[name]

    def capabilities(self) -> dict[str, object]:
        return {
            "engines": [self.get(name).capabilities() for name in ENGINE_NAMES],
            "mock": self.mock,
        }


__all__ = [
    "Engine",
    "EngineRegistry",
    "ENGINE_NAMES",
    "GenerateResult",
    "WordTiming",
    "KokoroEngine",
    "Maya1Engine",
    "VoxtralEngine",
]
