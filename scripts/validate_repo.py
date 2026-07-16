#!/usr/bin/env python3
from __future__ import annotations

import json
import py_compile
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REQUIRED = [
    "package.json",
    "Cargo.toml",
    "apps/desktop/src-tauri/tauri.conf.json",
    "apps/desktop/src-tauri/src/commands.rs",
    "crates/audiobookgen-core/src/epub.rs",
    "services/tts-worker/audiobookgen_worker/main.py",
    "src/components/ReaderStudio.tsx",
]


def main() -> int:
    errors: list[str] = []
    for relative in REQUIRED:
        if not (ROOT / relative).is_file():
            errors.append(f"missing required file: {relative}")
    for path in ROOT.rglob("*.json"):
        if any(part in {"node_modules", ".next"} for part in path.parts):
            continue
        try:
            json.loads(path.read_text(encoding="utf-8"))
        except Exception as error:
            errors.append(f"invalid JSON {path.relative_to(ROOT)}: {error}")
    for path in ROOT.rglob("*.toml"):
        if any(part in {"node_modules", ".next"} for part in path.parts):
            continue
        try:
            tomllib.loads(path.read_text(encoding="utf-8"))
        except Exception as error:
            errors.append(f"invalid TOML {path.relative_to(ROOT)}: {error}")
    for path in ROOT.rglob("*.py"):
        if any(part in {"node_modules", ".next", ".venv"} for part in path.parts):
            continue
        try:
            py_compile.compile(path, doraise=True)
        except Exception as error:
            errors.append(f"invalid Python {path.relative_to(ROOT)}: {error}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("Repository structure and static manifests are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
