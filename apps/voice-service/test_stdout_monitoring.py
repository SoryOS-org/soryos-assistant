#!/usr/bin/env python3
"""Communication handler helpers regression tests."""

import unittest
from unittest import mock

import psutil

from communication import CommunicationHandler


class ResolveParentProcessTests(unittest.TestCase):
    def setUp(self) -> None:
        self.handler = CommunicationHandler()

    @mock.patch("communication.os.getpid", return_value=500)
    @mock.patch("communication.psutil.Process")
    def test_resolves_bootstrap_parent(self, process_cls: mock.Mock, _getpid: mock.Mock) -> None:
        current_proc = mock.Mock()
        current_proc.name.return_value = "sprite_voice_sidecar.exe"

        bootstrap_proc = mock.Mock()
        bootstrap_proc.name.return_value = "sprite_voice_sidecar.exe"
        bootstrap_proc.parent.return_value = mock.Mock(pid=321)

        host_proc = mock.Mock()
        host_proc.name.return_value = "SpriteHost.exe"

        def side_effect(pid):
            mapping = {
                500: current_proc,
                100: bootstrap_proc,
                321: host_proc,
            }
            return mapping[pid]

        process_cls.side_effect = side_effect

        resolved = self.handler._resolve_parent_process(100)
        self.assertEqual(resolved, 321)

    @mock.patch("communication.os.getpid", return_value=500)
    @mock.patch("communication.psutil.Process")
    def test_returns_same_pid_when_host_already_resolved(
        self, process_cls: mock.Mock, _getpid: mock.Mock
    ) -> None:
        current_proc = mock.Mock()
        current_proc.name.return_value = "sprite_voice_sidecar.exe"

        host_proc = mock.Mock()
        host_proc.name.return_value = "DifferentHost.exe"
        host_proc.parent.return_value = None

        def side_effect(pid):
            mapping = {
                500: current_proc,
                1234: host_proc,
            }
            return mapping[pid]

        process_cls.side_effect = side_effect

        resolved = self.handler._resolve_parent_process(1234)
        self.assertEqual(resolved, 1234)

    @mock.patch("communication.psutil.Process", side_effect=psutil.NoSuchProcess(pid=999))
    def test_returns_original_pid_on_lookup_failure(self, _process_cls: mock.Mock) -> None:
        resolved = self.handler._resolve_parent_process(999)
        self.assertEqual(resolved, 999)


if __name__ == "__main__":
    unittest.main()
