"""Minimal Tekken vocabulary wrapper with embedding-range protection."""

import base64
import json
from pathlib import Path


class TekkenTokenizer:
    def __init__(self, path: Path) -> None:
        import tiktoken

        data = json.loads(path.read_text(encoding="utf-8"))
        ranks = {
            base64.b64decode(item["token_bytes"]): item["rank"]
            for item in data.get("vocab", [])
        }
        config = data.get("config", {})
        self._offset = int(config.get("default_num_special_tokens", 1000))
        special = {
            item["token_str"]: len(ranks) + item["rank"]
            for item in data.get("special_tokens", [])
        }
        pattern = config.get(
            "pattern",
            r"(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}{1,3}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+",
        )
        self._encoding = tiktoken.Encoding(
            name="tekken",
            pat_str=pattern,
            mergeable_ranks=ranks,
            special_tokens=special,
        )

    def encode(self, text: str) -> list[int]:
        encoded = self._encoding.encode(text, allowed_special="all")
        return [min(item + self._offset, 131_071) for item in encoded]
