#!/usr/bin/env python3
"""Self-termination watchdog regression tests."""

import threading
import unittest
from unittest import mock

import psutil

from communication import CommunicationHandler


class WatchdogTests(unittest.TestCase):
    def setUp(self) -> None:
        self.handler = CommunicationHandler()

    def tearDown(self) -> None:
        self.handler.stop()

    @mock.patch("communication.time.sleep", return_value=None)
    @mock.patch("communication.psutil.Process")
    def test_watchdog_requests_exit_when_parent_missing(
        self, process_cls: mock.Mock, _sleep: mock.Mock
    ) -> None:
        process_cls.side_effect = psutil.NoSuchProcess(pid=4321)

        self.handler.is_running = True
        self.handler.parent_pid = 4321

        self.handler._monitor_parent_process()

        self.assertTrue(self.handler.should_exit)
        self.assertFalse(self.handler.is_running)

    @mock.patch("communication.time.sleep", return_value=None)
    @mock.patch("communication.psutil.Process")
    def test_watchdog_sets_exit_on_pid_reuse(
        self, process_cls: mock.Mock, _sleep: mock.Mock
    ) -> None:
        mock_proc = mock.Mock()
        mock_proc.is_running.return_value = True
        mock_proc.status.return_value = psutil.STATUS_RUNNING
        mock_proc.create_time.return_value = 100.0

        process_cls.return_value = mock_proc

        self.handler.is_running = True
        self.handler.parent_pid = 1111
        self.handler.parent_start_time = 10.0

        self.handler._monitor_parent_process()

        self.assertTrue(self.handler.should_exit)
        self.assertFalse(self.handler.is_running)

    @mock.patch("communication.time.sleep", return_value=None)
    @mock.patch("communication.psutil.Process")
    def test_watchdog_leaves_running_parent_alone(
        self, process_cls: mock.Mock, _sleep: mock.Mock
    ) -> None:
        mock_proc = mock.Mock()
        mock_proc.is_running.return_value = True
        mock_proc.status.return_value = psutil.STATUS_RUNNING
        mock_proc.create_time.return_value = 10.0

        process_cls.return_value = mock_proc

        def stop_after_delay():
            self.handler.is_running = False

        self.handler.is_running = True
        self.handler.parent_pid = 2000
        self.handler.parent_start_time = 10.0

        thread = threading.Thread(target=stop_after_delay)
        thread.start()
        self.handler._monitor_parent_process()
        thread.join()

        self.assertFalse(self.handler.should_exit)


if __name__ == "__main__":
    unittest.main()
