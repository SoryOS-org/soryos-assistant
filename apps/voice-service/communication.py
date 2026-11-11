import sys
import threading
import time
import json
import psutil
import os
import logging
from typing import Optional

from cli_args import get_cli_args

cli_args = get_cli_args()
log_file = os.path.join(cli_args.log_dir, 'sprite_voice_sidecar.log')

logging.basicConfig(
    level=logging.WARNING,
    format='%(asctime)s - %(levelname)s - %(message)s',
    handlers=[
        logging.FileHandler(log_file),
        logging.StreamHandler(sys.stderr)  # Use stderr for errors
    ]
)

class CommunicationHandler:
    def __init__(self):
        self.is_running = False
        self.parent_pid = None
        self.monitor_thread = None
        self.should_exit = False
        self.parent_start_time = None
        self._monitor_lock = threading.Lock()
        self._last_log_ts = 0.0
        
    def start(self, parent_pid=None):
        """Start communication handler with parent process monitoring"""
        self.is_running = True
        
        # Get parent process ID (the Rust app that spawned this Python process)
        if parent_pid is not None:
            self.parent_pid = parent_pid
        else:
            # Fallback to os.getppid() when CLI argument is missing
            self.parent_pid = os.getppid()

        # PyInstaller one-file builds spawn a bootstrap process that in turn
        # launches the real Python payload. In that scenario os.getppid()
        # points to another copy of sprite_voice_sidecar.exe, not the Rust host.
        # Walk the parent chain until we find the first process that does not
        # look like our own binary so we always monitor the actual host.
        resolved_parent = self._resolve_parent_process(self.parent_pid)
        if resolved_parent != self.parent_pid:
            logging.warning(
                "Remapped parent PID from bootstrap %s to host %s",
                self.parent_pid,
                resolved_parent,
            )
            self.parent_pid = resolved_parent

        self._record_parent_start_time()
        logging.warning(
            "Sidecar watchdog starting; parent_pid=%s start_time=%s",
            self.parent_pid,
            self.parent_start_time,
        )
        
        # Parent PID is now set
        
        # Start parent process monitoring thread
        self.monitor_thread = threading.Thread(target=self._monitor_parent_process, daemon=True)
        self.monitor_thread.start()
        
        # Communication handler started
    
    def send_wake_word_detected(self):
        """Send wake word detected signal"""
        print("WAKE_WORD_DETECTED", flush=True)
    
    def _monitor_parent_process(self):
        """Monitor parent process PID and self-terminate if it's no longer running"""
        while self.is_running:
            time.sleep(2.0)  # Check every 2 seconds
            
            if self.parent_pid is None:
                logging.warning("Parent PID unknown; exiting voice processor")
                self._request_exit()
                break

            try:
                parent_process = psutil.Process(self.parent_pid)
                if not parent_process.is_running():
                    logging.warning(f"Parent process (PID {self.parent_pid}) no longer running, self-terminating")
                    self._request_exit()
                    break

                # Detect PID reuse by comparing start times
                if self.parent_start_time is not None:
                    try:
                        if abs(parent_process.create_time() - self.parent_start_time) > 0.5:
                            logging.warning(
                                f"Parent PID {self.parent_pid} has been reused (create_time changed); exiting sidecar"
                            )
                            self._request_exit()
                            break
                    except (psutil.Error, OSError):
                        logging.warning(
                            f"Unable to read parent process start time for PID {self.parent_pid}; exiting to be safe"
                        )
                        self._request_exit()
                        break

                now = time.time()
                if now - self._last_log_ts >= 10.0:
                    self._last_log_ts = now
                    try:
                        create_time_val = parent_process.create_time()
                    except (psutil.Error, OSError):
                        create_time_val = None
                    logging.info(
                        "Watchdog heartbeat: parent_pid=%s status=%s is_running=%s create_time=%s",
                        self.parent_pid,
                        parent_process.status(),
                        parent_process.is_running(),
                        create_time_val,
                    )

                status = parent_process.status()
                zombie_status = getattr(psutil, "STATUS_ZOMBIE", "zombie")
                dead_status = getattr(psutil, "STATUS_DEAD", "dead")
                if status in (zombie_status, dead_status):
                    logging.warning(
                        f"Parent process (PID {self.parent_pid}) status={status}; self-terminating"
                    )
                    self._request_exit()
                    break

            except (psutil.NoSuchProcess, psutil.ZombieProcess):
                logging.warning(f"Parent process (PID {self.parent_pid}) vanished; self-terminating")
                self._request_exit()
                break
            except Exception as e:
                logging.error(f"Error monitoring parent process: {e}")
                # Don't exit on monitoring errors, just log them
                time.sleep(5.0)  # Wait longer before retrying

    def stop(self):
        """Stop communication handler"""
        self.is_running = False
        self.should_exit = True
        if self.monitor_thread and self.monitor_thread.is_alive():
            self.monitor_thread.join(timeout=3.0)
        self.monitor_thread = None
        self.parent_pid = None
        self.parent_start_time = None
        logging.warning("Communication handler stopped; watchdog thread joined")

    def _record_parent_start_time(self):
        if self.parent_pid is None:
            return
        try:
            parent_process = psutil.Process(self.parent_pid)
            self.parent_start_time = parent_process.create_time()
        except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
            logging.warning(f"Unable to capture start time for parent PID {self.parent_pid}")
            self.parent_start_time = None

    def _request_exit(self):
        with self._monitor_lock:
            self.should_exit = True
            self.is_running = False
            logging.warning("Watchdog requesting sidecar shutdown")

    def _resolve_parent_process(self, initial_pid: Optional[int]) -> Optional[int]:
        """Best-effort detection of the real host process when running from a PyInstaller stub."""
        if initial_pid is None:
            return initial_pid

        try:
            current_name = psutil.Process(os.getpid()).name()
        except psutil.Error:
            current_name = None

        visited = set()
        candidate_pid = initial_pid

        while candidate_pid and candidate_pid not in visited:
            visited.add(candidate_pid)
            try:
                candidate_proc = psutil.Process(candidate_pid)
            except psutil.NoSuchProcess:
                return candidate_pid
            except psutil.Error:
                return candidate_pid

            try:
                candidate_name = candidate_proc.name()
            except psutil.Error:
                return candidate_pid

            if current_name and candidate_name.lower() == current_name.lower():
                parent = candidate_proc.parent()
                if parent is None:
                    return candidate_pid
                candidate_pid = parent.pid
                continue

            return candidate_pid

        return candidate_pid

