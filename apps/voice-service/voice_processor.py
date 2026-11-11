import json
import logging
import sys
import time
from typing import Optional

from cli_args import get_cli_args
from communication import CommunicationHandler
from wake_word_detector import WakeWordDetector


class VoiceProcessorError(Exception):
    def __init__(self, message: str, code: int, detail: Optional[str] = None) -> None:
        super().__init__(message)
        self.code = code
        self.detail = detail


def _fatal_exit(code: int, message: str, detail: Optional[str] = None) -> None:
    payload = {"error": message, "detail": detail, "code": code}
    print(f"VOICE_PROCESSOR_FATAL {json.dumps(payload)}", flush=True)
    sys.exit(code)


def _set_app_user_model_id(app_user_model_id: Optional[str]) -> None:
    if sys.platform != "win32":
        return

    from ctypes import windll  # type: ignore[attr-defined]

    value = (app_user_model_id or "").strip()
    if not value:
        raise VoiceProcessorError(
            "app_user_model_id_missing",
            code=10,
            detail="AppUserModelID missing; voice sidecar cannot continue.",
        )

    try:
        # Ensure consistent grouping in the Windows taskbar/jump lists.
        windll.shell32.SetCurrentProcessExplicitAppUserModelID(value)
    except Exception as exc:  # pragma: no cover - platform specific
        raise VoiceProcessorError(
            "app_user_model_id_set_failed",
            code=11,
            detail=str(exc),
        ) from exc

class VoiceProcessor:
    def __init__(self):
        self.wake_word_detector = None
        self.communication = CommunicationHandler()
        
    def start(self, parent_pid=None):
        """Initialize and start wake word detection only"""
        try:
            # Start communication with parent PID monitoring
            self.communication.start(parent_pid)
            
            # Initialize wake-word detector ONLY
            self.wake_word_detector = WakeWordDetector()
            self.wake_word_detector.set_callback(self.on_wake_word_detected)
            self.wake_word_detector.start()
            
            print("VOICE_PROCESSOR_READY", flush=True)

        except Exception as exc:
            raise VoiceProcessorError(
                "startup_failed",
                code=20,
                detail=str(exc),
            ) from exc
    
    def on_wake_word_detected(self):
        """Called when wake word is detected"""
        self.communication.send_wake_word_detected()
    
    def shutdown(self):
        """Clean shutdown"""
        if self.wake_word_detector:
            self.wake_word_detector.stop()
        if self.communication:
            self.communication.stop()
        sys.exit(0)

if __name__ == "__main__":
    try:
        cli_args = get_cli_args()
        _set_app_user_model_id(cli_args.app_user_model_id)
        parent_pid = cli_args.parent_pid
        if parent_pid is None and len(sys.argv) > 1:
            logging.warning(
                "Parent PID missing or invalid; child will monitor os.getppid()"
            )

        processor = VoiceProcessor()
        processor.start(parent_pid)

        # Keep the process running and check for exit flag
        try:
            while True:
                if processor.communication.should_exit:
                    break
                time.sleep(0.1)
        except KeyboardInterrupt:
            pass  # Graceful shutdown
        finally:
            processor.shutdown()
    except VoiceProcessorError as exc:
        logging.getLogger(__name__).error("%s", exc)
        _fatal_exit(exc.code, str(exc), exc.detail)
    except Exception as exc:
        logging.getLogger(__name__).exception("Unexpected voice processor error")
        _fatal_exit(1, "unexpected_error", str(exc))
