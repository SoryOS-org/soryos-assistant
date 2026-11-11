import os
import struct
import threading

import pvporcupine
import pyaudio

from cli_args import get_cli_args


class WakeWordDetector:
    def __init__(self, keyword=None, sensitivity=None, access_key=None):
        cli_args = get_cli_args()
        cli_keyword, cli_sensitivity = cli_args.wakeword, cli_args.sensitivity

        self.keyword = (
            keyword
            if keyword is not None
            else cli_keyword
            if cli_keyword is not None
            else "hey google"
        )
        self.sensitivity = (
            sensitivity
            if sensitivity is not None
            else cli_sensitivity
            if cli_sensitivity is not None
            else 0.5
        )
        self.audio_sample_rate = 16000  # Standard for Porcupine
        self.audio_channels = 1
        self.audio_frame_length = 512

        if access_key is None:
            access_key = cli_args.access_key
        if access_key is None:
            access_key = os.getenv("PICOVOICE_ACCESS_KEY")

        if not access_key or not access_key.strip():
            raise RuntimeError(
                "Picovoice access key missing; wake-word detection requires a valid key."
            )

        try:
            self.porcupine = pvporcupine.create(
                access_key=access_key,
                keywords=[self.keyword],
                sensitivities=[self.sensitivity],
            )
        except Exception as exc:  # pragma: no cover - Picovoice failure intentionally fatal
            raise RuntimeError(f"Failed to initialise Porcupine wake word engine: {exc}") from exc

        try:
            self.audio = pyaudio.PyAudio()
        except Exception as exc:
            raise RuntimeError(f"Failed to initialise PyAudio input: {exc}") from exc

        self.stream = None
        self.is_running = False
        self.thread = None
        self.callback = None
    
    def start(self):
        """Start wake-word detection"""
        try:
            self.stream = self.audio.open(
                rate=16000,  # Standard rate
                channels=1,
                format=pyaudio.paInt16,
                input=True,
                frames_per_buffer=512,
            )

            self.is_running = True
            self.thread = threading.Thread(target=self._real_detection_loop)
            self.thread.start()
        except Exception as e:
            print(f"WAKE_WORD_ERROR: {str(e)}", flush=True)
            import traceback

            print(f"DEBUG: Traceback: {traceback.format_exc()}", flush=True)
    
    def _detection_loop(self):
        """Main detection loop"""
        self._real_detection_loop()
    
    def _real_detection_loop(self):
        """Real Porcupine detection loop"""
        while self.is_running:
            try:
                pcm = self.stream.read(self.porcupine.frame_length)
                pcm = struct.unpack_from("h" * self.porcupine.frame_length, pcm)
                
                keyword_index = self.porcupine.process(pcm)
                
                if keyword_index >= 0:
                    if self.callback:
                        self.callback()
            except Exception as e:
                print(f"WAKE_WORD_ERROR: {str(e)}", flush=True)
                break
    
    def set_callback(self, callback):
        """Set callback for wake word detection"""
        self.callback = callback
    
    def stop(self):
        """Stop wake-word detection"""
        self.is_running = False
        if self.thread:
            self.thread.join()
        if self.stream:
            self.stream.stop_stream()
            self.stream.close()
        if self.audio:
            self.audio.terminate()
        if self.porcupine:
            self.porcupine.delete()
    
    def _load_cli_overrides(self):
        args = get_cli_args()
        return args.wakeword, args.sensitivity
