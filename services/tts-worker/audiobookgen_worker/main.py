from __future__ import annotations

import argparse
import json
import os
import sys
import traceback
from pathlib import Path
from typing import Any

from .engines import EngineRegistry
from .protocol import (
    ProtocolError,
    parse_engine,
    parse_generate,
    parse_options,
    require_string,
)


def emit(payload: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(payload, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def progress_emitter(request_id: str):
    def emit_progress(
        state: str, current: int | None = None, total: int | None = None
    ) -> None:
        payload: dict[str, Any] = {"id": request_id, "type": "progress", "state": state}
        if current is not None:
            payload["current"] = int(current)
        if total is not None:
            payload["total"] = int(total)
        emit(payload)

    return emit_progress


def handle(registry: EngineRegistry, payload: dict[str, Any]) -> bool:
    request_type = payload.get("type")
    request_id = require_string(payload, "id", maximum=200)
    if request_type == "ping":
        emit({"id": request_id, "type": "ready", "payload": registry.capabilities()})
    elif request_type == "capabilities":
        emit({"id": request_id, "type": "complete", "payload": registry.capabilities()})
    elif request_type == "model_status":
        engine = registry.get(parse_engine(payload))
        model_dir = Path(
            require_string(payload, "model_dir", maximum=4096)
        ).expanduser()
        emit(
            {
                "id": request_id,
                "type": "complete",
                "payload": engine.installed(model_dir, parse_options(payload)),
            }
        )
    elif request_type == "download_model":
        engine = registry.get(parse_engine(payload))
        model_dir = Path(
            require_string(payload, "model_dir", maximum=4096)
        ).expanduser()
        if not model_dir.is_absolute():
            raise ProtocolError("model_dir must be absolute")
        emit({"id": request_id, "type": "progress", "state": "downloading"})
        result = engine.ensure_model(
            model_dir, parse_options(payload), progress_emitter(request_id)
        )
        emit({"id": request_id, "type": "complete", "payload": result})
    elif request_type == "generate":
        request = parse_generate(payload)
        engine = registry.get(request.engine)
        result = engine.generate(
            request.text,
            request.voice,
            request.speed,
            request.output_path,
            request.model_dir,
            request.options,
            progress_emitter(request_id),
        )
        emit(
            {
                "id": request_id,
                "type": "complete",
                "duration_ms": result.duration_ms,
                "sample_rate": result.sample_rate,
                "word_timings": [timing.as_dict() for timing in result.word_timings],
                "payload": {"output_path": str(request.output_path)},
            }
        )
    elif request_type == "shutdown":
        emit({"id": request_id, "type": "complete", "payload": {"shutdown": True}})
        return False
    else:
        raise ProtocolError(f"unknown request type: {request_type!r}")
    return True


def run(mock: bool = False) -> int:
    registry = EngineRegistry(mock=mock)
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        request_id = "unknown"
        try:
            payload = json.loads(line)
            if not isinstance(payload, dict):
                raise ProtocolError("request must be a JSON object")
            request_id = str(payload.get("id", request_id))
            if not handle(registry, payload):
                return 0
        except (ProtocolError, json.JSONDecodeError) as error:
            emit(
                {
                    "id": request_id,
                    "type": "error",
                    "message": str(error),
                    "payload": {},
                }
            )
        except Exception as error:  # worker stays alive after one bad request
            traceback.print_exc(file=sys.stderr)
            emit(
                {
                    "id": request_id,
                    "type": "error",
                    "code": getattr(error, "code", "worker_failure"),
                    "message": str(error),
                    "payload": {},
                }
            )
    return 0


def main() -> None:
    parser = argparse.ArgumentParser(description="AudiobookGen TTS worker")
    parser.add_argument(
        "--mock", action="store_true", help="Generate deterministic test WAV files"
    )
    args = parser.parse_args()
    raise SystemExit(
        run(mock=args.mock or os.environ.get("AUDIOBOOKGEN_WORKER_MOCK") == "1")
    )


if __name__ == "__main__":
    main()
