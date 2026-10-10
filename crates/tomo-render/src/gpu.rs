// SPDX-License-Identifier: MIT
use std::sync::mpsc;

use tomo_runtime::Snapshot;
use tomo_scene::Problem;
use wgpu::util::DeviceExt;

use crate::{Capture, CaptureMetadata, CaptureOptions, geometry::Geometry};

fn error(value: impl std::fmt::Display) -> Problem {
    Problem::new("RENDER", "/gpu", value.to_string())
}

fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    lines: bool,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Tomo primitive pipeline"),
        layout: None,
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 40,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x3, 2 => Float32x3],
            }],
        },
        primitive: wgpu::PrimitiveState {
            topology: if lines { wgpu::PrimitiveTopology::LineList } else { wgpu::PrimitiveTopology::TriangleList },
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: !lines,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: Default::default(),
            bias: wgpu::DepthBiasState { constant: if lines { -2 } else { 0 }, ..Default::default() },
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(if lines { "fs_line" } else { "fs_solid" }),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

pub(crate) async fn render(
    geometry: Geometry,
    snapshot: &Snapshot,
    options: &CaptureOptions,
) -> Result<Capture, Problem> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL | wgpu::Backends::VULKAN | wgpu::Backends::DX12,
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .map_err(|e| Problem::new("GPU_UNAVAILABLE", "/gpu", e.to_string()))?;
    let info = adapter.get_info();
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Tomo capture"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        })
        .await
        .map_err(error)?;
    device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    device.push_error_scope(wgpu::ErrorFilter::Internal);
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let size = wgpu::Extent3d {
        width: options.width,
        height: options.height,
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Tomo capture color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Tomo capture depth"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = color.create_view(&Default::default());
    let depth_view = depth.create_view(&Default::default());
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Tomo primitives"),
        source: wgpu::ShaderSource::Wgsl(include_str!("primitive.wgsl").into()),
    });
    let solid = pipeline(&device, &shader, false);
    let lines = pipeline(&device, &shader, true);
    let buffer = |contents: &[u8]| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tomo vertices"),
            contents,
            usage: wgpu::BufferUsages::VERTEX,
        })
    };
    let triangle_buffer = (!geometry.triangles.is_empty()).then(|| buffer(&geometry.triangles));
    let line_buffer = (!geometry.lines.is_empty()).then(|| buffer(&geometry.lines));
    let row_bytes = options.width * 4;
    let padded_row =
        row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Tomo readback"),
        size: u64::from(padded_row) * u64::from(options.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Tomo capture"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Tomo primitives"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.025,
                        g: 0.04,
                        b: 0.065,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        if let Some(buffer) = &triangle_buffer {
            pass.set_pipeline(&solid);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..(geometry.triangles.len() / 40) as u32, 0..1);
        }
        if let Some(buffer) = &line_buffer {
            pass.set_pipeline(&lines);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..(geometry.lines.len() / 40) as u32, 0..1);
        }
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &color,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(options.height),
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    device.poll(wgpu::PollType::Wait).map_err(error)?;
    for _ in 0..3 {
        if let Some(problem) = device.pop_error_scope().await {
            return Err(error(problem));
        }
    }
    receiver.recv().map_err(error)?.map_err(error)?;
    let mapped = readback.slice(..).get_mapped_range();
    let mut rgba = Vec::with_capacity(row_bytes as usize * options.height as usize);
    for row in mapped.chunks_exact(padded_row as usize) {
        rgba.extend_from_slice(&row[..row_bytes as usize]);
    }
    drop(mapped);
    readback.unmap();
    Ok(Capture {
        metadata: CaptureMetadata {
            schema_version: 1,
            tick: snapshot.tick,
            simulation_time: snapshot.simulation_time,
            options: options.clone(),
            adapter: info.name,
            backend: format!("{:?}", info.backend),
            device_type: format!("{:?}", info.device_type),
            bodies: geometry.bodies,
        },
        rgba,
    })
}
