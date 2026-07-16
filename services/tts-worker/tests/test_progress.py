"""Regression tests for structured worker progress events."""

import contextlib
import io
import json
import unittest

from audiobookgen_worker.main import progress_emitter


class ProgressEmitterTest(unittest.TestCase):
    def test_emits_optional_byte_counts(self) -> None:
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            progress_emitter("download-1")("downloading", 25, 100)

        self.assertEqual(
            json.loads(stdout.getvalue()),
            {
                "id": "download-1",
                "type": "progress",
                "state": "downloading",
                "current": 25,
                "total": 100,
            },
        )

    def test_omits_unknown_counts(self) -> None:
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            progress_emitter("load-1")("loading")

        self.assertEqual(
            json.loads(stdout.getvalue()),
            {"id": "load-1", "type": "progress", "state": "loading"},
        )
