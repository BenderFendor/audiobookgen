from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any


class ProtocolError(ValueError):
    pass


@dataclass(frozen=True)
class GenerateRequest:
    request_id: str
    text: str
    voice: str
    speed: float
    output_path: Path
    model_dir: Path


def require_string(payload: dict[str, Any], key: str, *, maximum: int) -> str:
    value = payload.get(key)
    if not isinstance(value, str) or not value.strip():
        raise ProtocolError(f"{key} must be a non-empty string")
    if len(value) > maximum:
        raise ProtocolError(f"{key} exceeds {maximum} characters")
    return value


def parse_generate(payload: dict[str, Any]) -> GenerateRequest:
    request_id = require_string(payload, "id", maximum=200)
    text = require_string(payload, "text", maximum=12_000)
    voice = require_string(payload, "voice", maximum=80)
    output_path = Path(require_string(payload, "output_path", maximum=4096)).expanduser()
    model_dir = Path(require_string(payload, "model_dir", maximum=4096)).expanduser()
    try:
        speed = float(payload.get("speed", 1.0))
    except (TypeError, ValueError) as error:
        raise ProtocolError("speed must be numeric") from error
    if not 0.5 <= speed <= 2.0:
        raise ProtocolError("speed must be between 0.5 and 2.0")
    if not output_path.is_absolute() or not model_dir.is_absolute():
        raise ProtocolError("worker paths must be absolute")
    return GenerateRequest(request_id, text, voice, speed, output_path, model_dir)
