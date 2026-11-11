#!/usr/bin/env python3
"""Voice processor fatal output regression tests."""

import io
import json
import sys
import types
import unittest
from contextlib import redirect_stdout
from unittest import mock


def _install_dummy_modules() -> None:
    if "pvporcupine" not in sys.modules:
        pv_module = types.ModuleType("pvporcupine")
        pv_module.create = lambda **_: None  # type: ignore[attr-defined]
        sys.modules["pvporcupine"] = pv_module
    if "pyaudio" not in sys.modules:
        pa_module = types.ModuleType("pyaudio")

        class DummyPyAudio:  # pragma: no cover - instantiation not exercised
            pass

        pa_module.PyAudio = DummyPyAudio  # type: ignore[attr-defined]
        sys.modules["pyaudio"] = pa_module


class FatalExitTests(unittest.TestCase):
    def setUp(self) -> None:
        _install_dummy_modules()
        import importlib

        self.voice_processor = importlib.import_module("voice_processor")

    def test_fatal_exit_emits_structured_payload(self) -> None:
        buffer = io.StringIO()
        with mock.patch("sys.exit", side_effect=SystemExit), redirect_stdout(buffer):
            with self.assertRaises(SystemExit) as raised:
                self.voice_processor._fatal_exit(42, "app_user_model_id_missing", "detail")

        self.assertEqual(raised.exception.code, 42)
        line = buffer.getvalue().strip()
        self.assertTrue(line.startswith("VOICE_PROCESSOR_FATAL "))
        payload = json.loads(line.split(" ", 1)[1])
        self.assertEqual(payload["code"], 42)
        self.assertEqual(payload["error"], "app_user_model_id_missing")
        self.assertEqual(payload["detail"], "detail")


if __name__ == "__main__":
    unittest.main()
