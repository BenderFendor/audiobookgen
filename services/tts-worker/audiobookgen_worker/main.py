from __future__ import annotations

import argparse
import json
import os
import sys
import traceback
from pathlib import Path
from typing import Any

from .engine import KokoroEngine
from .protocol import ProtocolError, parse_generate, require_string


def emit(payload: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(payload, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def handle(engine: KokoroEngine, payload: dict[str, Any]) -> bool:
    request_type = payload.get("type")
    request_id = require_string(payload, "id", maximum=200)
    if request_type == "ping":
        emit({"id": request_id, "type": "ready", "payload": engine.capabilities()})
    elif request_type == "capabilities":
        emit({"id": request_id, "type": "complete", "payload": engine.capabilities()})
    elif request_type == "download_model":
        model_dir = Path(require_string(payload, "model_dir", maximum=4096)).expanduser()
        if not model_dir.is_absolute():
            raise ProtocolError("model_dir must be absolute")
        emit({"id": request_id, "type": "progress", "state": "downloading"})
        emit({"id": request_id, "type": "complete", "payload": engine.ensure_model(model_dir)})
    elif request_type == "generate":
        request = parse_generate(payload)
        duration_ms, sample_rate = engine.generate(
            request.text,
            request.voice,
            request.speed,
            request.output_path,
            request.model_dir,
            lambda state: emit({"id": request_id, "type": "progress", "state": state}),
        )
        emit({
            "id": request_id,
            "type": "complete",
            "duration_ms": duration_ms,
            "sample_rate": sample_rate,
            "payload": {"output_path": str(request.output_path)},
        })
    elif request_type == "shutdown":
        emit({"id": request_id, "type": "complete", "payload": {"shutdown": True}})
        return False
    else:
        raise ProtocolError(f"unknown request type: {request_type!r}")
    return True


def run(mock: bool = False) -> int:
    engine = KokoroEngine(mock=mock)
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
            if not handle(engine, payload):
                return 0
        except (ProtocolError, json.JSONDecodeError) as error:
            emit({"id": request_id, "type": "error", "message": str(error), "payload": {}})
        except Exception as error:
            traceback.print_exc(file=sys.stderr)
            emit({"id": request_id, "type": "error", "message": str(error), "payload": {}})
    return 0


def main() -> None:
    parser = argparse.ArgumentParser(description="AudiobookGen Kokoro worker")
    parser.add_argument("--mock", action="store_true", help="Generate deterministic test WAV files")
    args = parser.parse_args()
    raise SystemExit(run(mock=args.mock or os.environ.get("AUDIOBOOKGEN_WORKER_MOCK") == "1"))


if __name__ == "__main__":
    main()
