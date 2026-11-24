use crate::{validate, Fragment};
use std::sync::mpsc;
use wgpu::util::DeviceExt;

pub struct Reducer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    high: wgpu::ComputePipeline,
    low: wgpu::ComputePipeline,
    pub adapter: wgpu::AdapterInfo,
}

impl Reducer {
    pub async fn open(software: bool) -> Result<Self, String> {
        let backends = if cfg!(windows) {
            wgpu::Backends::DX12
        } else {
            wgpu::Backends::VULKAN
        };
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                force_fallback_adapter: software,
                ..Default::default()
            })
            .await
            .ok_or_else(|| format!("no compatible {backends:?} adapter (software={software})"))?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("SixBridge"),
                    ..Default::default()
                },
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("64-bit final max"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/image_max.wgsl").into()),
        });
        let pipeline = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Ok(Self {
            high: pipeline("reduce_high"),
            low: pipeline("resolve_low"),
            device,
            queue,
            adapter: adapter.get_info(),
        })
    }

    pub fn reduce(&self, texels: u32, fragments: &[Fragment]) -> Result<Vec<u64>, String> {
        validate(texels, fragments)?;
        let output_bytes = u64::from(texels) * 8;
        let limits = self.device.limits();
        let max_binding =
            u64::from(limits.max_storage_buffer_binding_size).min(limits.max_buffer_size);
        let groups = fragments.len().div_ceil(64);
        if output_bytes > max_binding
            || std::mem::size_of_val(fragments) as u64 > max_binding
            || groups > limits.max_compute_workgroups_per_dimension as usize
        {
            return Err("batch exceeds this device's single-dispatch limits".into());
        }
        if fragments.is_empty() {
            return Ok(vec![0; texels as usize]);
        }
        let input = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("fragments"),
                contents: bytemuck::cast_slice(fragments),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let buffer = |label, usage| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: output_bytes,
                usage,
                mapped_at_creation: false,
            })
        };
        let keys = buffer(
            "texel keys",
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
        );
        let readback = buffer(
            "readback",
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
        let mut commands = self.device.create_command_encoder(&Default::default());
        commands.clear_buffer(&keys, 0, None);
        for pipeline in [&self.high, &self.low] {
            let bindings = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: input.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: keys.as_entire_binding(),
                    },
                ],
            });
            let mut pass = commands.begin_compute_pass(&Default::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bindings, &[]);
            pass.dispatch_workgroups(groups as u32, 1, 1);
        }
        commands.copy_buffer_to_buffer(&keys, 0, &readback, 0, output_bytes);
        self.queue.submit(Some(commands.finish()));
        let (tx, rx) = mpsc::channel();
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let mapped = slice.get_mapped_range();
        let values = mapped
            .chunks_exact(8)
            .map(|bytes| {
                let high = u32::from_le_bytes(bytes[..4].try_into().unwrap());
                let low = u32::from_le_bytes(bytes[4..].try_into().unwrap());
                (u64::from(high) << 32) | u64::from(low)
            })
            .collect();
        drop(mapped);
        readback.unmap();
        Ok(values)
    }
}
