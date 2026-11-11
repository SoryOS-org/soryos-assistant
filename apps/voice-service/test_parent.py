#!/usr/bin/env python3
"""CLI argument parsing regression tests."""

import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from cli_args import get_cli_args


class CliArgsTests(unittest.TestCase):
    def tearDown(self) -> None:
        get_cli_args.cache_clear()

    def test_parent_pid_from_flag_takes_precedence(self) -> None:
        with mock.patch.object(
            sys,
            "argv",
            [
                "voice_processor.py",
                "--parent-pid",
                "4242",
                "9999",
                "--app-user-model-id",
                "com.test",
            ],
        ):
            args = get_cli_args()
        self.assertEqual(args.parent_pid, 4242)

    def test_parent_pid_falls_back_to_positional(self) -> None:
        with mock.patch.object(
            sys, "argv", ["voice_processor.py", "1234", "--app-user-model-id", "com.test"]
        ):
            args = get_cli_args()
        self.assertEqual(args.parent_pid, 1234)

    def test_invalid_parent_pid_results_in_none(self) -> None:
        with mock.patch.object(
            sys,
            "argv",
            ["voice_processor.py", "--parent-pid", "not-a-number", "--app-user-model-id", "com.test"],
        ):
            args = get_cli_args()
        self.assertIsNone(args.parent_pid)

    def test_log_directory_is_created(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            log_dir = Path(tmp) / "logs"
            with mock.patch.object(
                sys,
                "argv",
                [
                    "voice_processor.py",
                    "--log-dir",
                    str(log_dir),
                    "--app-user-model-id",
                    "com.test",
                ],
            ):
                args = get_cli_args()
            self.assertTrue(Path(args.log_dir).exists())


if __name__ == "__main__":
    unittest.main()
