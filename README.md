# GhostRAT v5 "Chameleon" — GPU VRAM Compute Shader RAT

## Architecture

```
┌─────────────────────┐         ┌──────────────────┐         ┌─────────────────┐
│  GitHub Pages       │         │  Victim Browser  │         │  C2 Channels    │
│  (hosting)          │         │                  │         │                 │
│                     │         │  WebGPU Implant  │         │  - WiFi Beacon  │
│  ghostrat-v5.github │         │                  │         │  - BLE Adv      │
│  .io/implant.html   │         │  GPU VRAM        │         │  - Telegram     │
│                     │         │  Execution       │         │  - DNS Tunnel   │
│  C2 Server          │         │                  │         │  - PNG Stego    │
│  (command dispatcher)│        │  Multi-channel   │         │                 │
│                     │◄────────┤  C2              │         │  Victim Machine │
│  Command receiver   │         │                  │         │                 │
└─────────────────────┘         └──────────────────┘         └─────────────────┘
```

## Features

- **WebGPU implant** — runs just by visiting a webpage
- **GPU VRAM execution** — no CPU memory, no files on disk
- **Multi-channel C2** — WiFi beacon, BLE, Telegram, DNS tunnel, PNG stego
- **Keylogger, file enumerator, screenshot modules**
- **XOR encryption + zlib compression** for C2 traffic
- **CRC-8 integrity check** for all commands

## Quick Start

1. Clone repository
2. Create GitHub Pages repository
3. Push `implant/ghostrat-webgpu.js` to `gh-pages` branch
4. Send link to victim: `https://yourusername.github.io/ghostrat-v5/implant.html`
5. Victim visits page → implant runs in GPU VRAM
6. Commands sent via C2 channel
7. Results sent back via C2 channel

## C2 Channels

| Channel | Trigger | Command Encoding | Result Encoding |
|---|---|---|---|
| WiFi Beacon | Scan for SSID | SSID string | SSID string |
| BLE Advertising | Scan for adv packet | Adv data | Adv data |
| Telegram Bot | Poll API | Message text | Message text |
| DNS Tunnel | DNS query | Subdomain | DNS response |
| PNG Stego | Fetch image | Image metadata | Image metadata |

## File Structure

```
ghostrat-v5/
├── core/                      # Core C++ implementation
│   ├── gpu_rat_core.hpp       # Main core header
│   ├── win/                   # Windows platform-specific
│   └── linux/                 # Linux platform-specific
├── implant/                   # WebGPU implant
│   ├── ghostrat-webgpu.js     # Main WebGPU implant
│   └── implant.html           # Delivery webpage
├── c2/                        # C2 channels
│   ├── wifi_beacon_c2.cpp     # WiFi Beacon C2
│   ├── ble_adv_c2.cpp         # BLE Advertising C2
│   ├── telegram_c2.cpp        # Telegram Bot C2
│   ├── dns_tunnel_c2.cpp      # DNS Tunnel C2
│   └── png_stego_c2.cpp       # PNG Stego C2
├── modules/                   # RAT modules
│   ├── keylogger.cpp          # Keylogger module
│   ├── file_enumerator.cpp    # File enumerator module
│   └── screenshot.cpp         # Screenshot module
├── server/                    # C2 server
│   ├── c2_server.cpp          # Main C2 server
│   └── commands.json          # Command templates
├── kb/                        # Knowledge base
└── README.md                  # This file
```

## EDR Evasion

| EDR Technique | GhostRAT v5 Bypass |
|---|---|
| File monitoring | No files on host disk |
| Process monitoring | No new processes |
| API hooking | Uses WebGPU APIs — not monitored |
| Network beaconing | C2 looks like normal WiFi/BLE/Telegram |
| AMSI | No PowerShell needed |
| ETW | No ETW events emitted |
| Memory scanning | Payload in GPU VRAM, not CPU memory |

## Money-Making Potential

1. **Browser-based implant** — Sell as a "visit-a-page" implant for pentesters
2. **Multi-channel C2** — Works in air-gapped environments (WiFi/BLE)
3. **Zero-network solution** — C2 via WiFi/BLE, works offline
4. **Cross-platform** — WebGPU works on all modern browsers

## License

MIT
