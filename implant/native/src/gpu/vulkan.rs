//! Vulkan compute shader execution (optional backend).
//!
//! Uses the Vulkan loader to execute compute shaders for payload processing.
//! Falls back to CPU if Vulkan is unavailable.

#[cfg(feature = "vulkan")]
use anyhow::Result;
#[cfg(feature = "vulkan")]
use tracing::{debug, warn};

/// Vulkan compute shader source (GLSL)
const COMPUTE_SHADER_GLSL: &str = r#"
#version 450
layout(local_size_x = 64, local_size_y = 1, local_size_z = 1) in;

layout(set = 0, binding = 0) buffer Payload {
    float data[];
} payload;

layout(set = 0, binding = 1) uniform Params {
    vec4 key;
    float key_len;
} params;

void main() {
    uint idx = gl_GlobalInvocationID.x;
    uint data_len = uint(payload.data.length());
    if (idx >= data_len) return;

    uint key_idx = idx % uint(params.key_len);
    float data_byte = payload.data[idx];
    float key_byte = params.key[key_idx % 4];

    // XOR operation on byte values
    payload.data[idx] = data_byte ^ key_byte;
}
"#;

/// Initialize Vulkan compute pipeline
#[cfg(feature = "vulkan")]
pub fn init() -> Result<()> {
    // Try to load Vulkan loader
    let instance = match vulkano::VulkanLibrary::new() {
        Ok(lib) => lib,
        Err(e) => {
            debug!("Vulkan library not found: {}", e);
            return Err(anyhow::anyhow!("Vulkan not available"));
        }
    };

    // Enumerate physical devices
    let physical_devices = instance.enumerate_physical_devices()?;
    let mut device = None;

    for physical_device in physical_devices {
        // Check for compute queue family
        let queue_families = physical_device.queue_family_properties();
        for (i, queue_family) in queue_families.iter().enumerate() {
            if queue_family.queue_count > 0
                && queue_family.queue_flags.contains(vulkano::QueueFlags::COMPUTE)
            {
                device = Some(physical_device);
                break;
            }
        }
        if device.is_some() {
            break;
        }
    }

    if let Some(physical_device) = device {
        let (_device, mut _queues) = vulkano::device::Device::new(
            physical_device,
            vulkano::device::DeviceExtensions {
                khr_storage_buffer_storage_class: true,
                ..vulkano::device::DeviceExtensions::empty()
            },
            vulkano::device::DeviceCreationInfo {
                enabled_features: vulkano::device::Features {
                    storage_buffer_storage_class: true,
                    ..vulkano::device::Features::empty()
                },
                ..Default::default()
            },
        )?;

        debug!("Vulkan device initialized");
    }

    Ok(())
}

/// Execute Vulkan compute shader on payload
#[cfg(feature = "vulkan")]
pub fn execute(payload: &[u8], _key: &str) -> Result<Vec<u8>> {
    // Load Vulkan library
    let library = vulkano::VulkanLibrary::new()?;
    let mut devices: Vec<_> = library
        .enumerate_physical_devices()?
        .collect();

    let physical_device = devices
        .next()
        .ok_or_else(|| anyhow::anyhow!("No Vulkan physical device found"))?;

    // Create device
    let (device, mut queues) = vulkano::device::Device::new(
        physical_device,
        vulkano::device::DeviceExtensions {
            khr_storage_buffer_storage_class: true,
            ..vulkano::device::DeviceExtensions::empty()
        },
        vulkano::device::DeviceCreationInfo {
            enabled_features: vulkano::device::Features {
                storage_buffer_storage_class: true,
                ..vulkano::device::Features::empty()
            },
            ..Default::default()
        },
    )?;

    // Compile shader
    let mut compiler = vulkano::shaders::ShaderFile::new(COMPUTE_SHADER_GLSL)?;
    let compute_shader = compiler.entry_point("main")?.finalize(&device)?;

    // Create pipeline
    let pipeline = vulkano::compute::pipeline::ComputePipelineStage::new(
        &device,
        compute_shader,
    )?;

    // Create storage buffer
    let payload_f32: Vec<f32> = payload.iter().map(|&b| b as f32).collect();
    let buffer = vulkano::buffer::TypedBuffer::new(
        device.clone(),
        vulkano::buffer::BufferUsage {
            storage: true,
            ..Default::default()
        },
        vulkano::buffer::BufferCreationLayout::CopyToOutput,
        payload_f32,
    )?;

    // Dispatch compute shader
    let mut executor = vulkano::compute::executor::ComputePipelineExecutor::new(
        pipeline.clone(),
        device.clone(),
    )?;

    executor.bind_buffer(buffer.clone());
    executor.dispatch((payload.len() as u32 + 63) / 64, 1, 1);

    let mut queue = queues.next().unwrap();
    queue.submit_synced([&executor])?;

    // Read result
    let result = buffer.into_data();
    let result_bytes: Vec<u8> = result.iter().map(|&f| f as u8).collect();

    debug!("Vulkan compute completed: {} bytes", result_bytes.len());
    Ok(result_bytes)
}

/// Fallback: CPU-based execute when Vulkan is not available
#[cfg(not(feature = "vulkan"))]
pub fn init() -> Result<()> {
    debug!("Vulkan backend disabled, using CPU fallback");
    Ok(())
}

/// Fallback: CPU-based execute when Vulkan is not available
#[cfg(not(feature = "vulkan"))]
pub fn execute(payload: &[u8], key: &str) -> Result<Vec<u8>> {
    // Simple CPU-based XOR as fallback
    let key_bytes = key.as_bytes();
    let result: Vec<u8> = payload
        .iter()
        .enumerate()
        .map(|(i, &b)| b ^ key_bytes[i % key_bytes.len()])
        .collect();
    debug!("CPU fallback compute completed: {} bytes", result.len());
    Ok(result)
}
