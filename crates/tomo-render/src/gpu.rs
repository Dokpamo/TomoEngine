// SPDX-License-Identifier: MIT
use crate::{Capture, CaptureMetadata, CaptureOptions, geometry::Geometry};
use std::sync::{Arc, Mutex, mpsc};
use tomo_runtime::Snapshot;
use tomo_scene::Problem;

pub(crate) fn error(value: impl std::fmt::Display) -> Problem {
    Problem::new("RENDER", "/gpu", value.to_string())
}

pub(crate) fn instance() -> wgpu::Instance {
    wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL | wgpu::Backends::VULKAN | wgpu::Backends::DX12,
        ..Default::default()
    })
}

pub(crate) async fn adapter(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter, Problem> {
    instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: surface,
        })
        .await
        .map_err(|e| Problem::new("GPU_UNAVAILABLE", "/gpu", e.to_string()))
}

fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    lines: bool,
    overlay: bool,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Tomo primitive pipeline"), layout: None,
        vertex: wgpu::VertexState {
            module: shader, entry_point: Some("vs_main"), compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 40, step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x3, 2 => Float32x3],
            }],
        },
        primitive: wgpu::PrimitiveState {
            topology: if lines { wgpu::PrimitiveTopology::LineList } else { wgpu::PrimitiveTopology::TriangleList },
            cull_mode: None, ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: !lines && !overlay,
            depth_compare: if overlay { wgpu::CompareFunction::Always } else { wgpu::CompareFunction::LessEqual },
            stencil: Default::default(),
            bias: wgpu::DepthBiasState { constant: if lines { -2 } else { 0 }, ..Default::default() },
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader, entry_point: Some(if lines || overlay { "fs_line" } else { "fs_solid" }),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview: None, cache: None,
    })
}

struct Vertices {
    buffer: wgpu::Buffer,
    capacity: u64,
    count: u32,
}
impl Vertices {
    fn new(device: &wgpu::Device, capacity: u64) -> Self {
        Self {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Tomo reusable vertices"),
                size: capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            capacity,
            count: 0,
        }
    }
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, bytes: &[u8]) {
        if bytes.len() as u64 > self.capacity {
            *self = Self::new(device, (bytes.len() as u64).next_power_of_two());
        }
        self.count = (bytes.len() / 40) as u32;
        if !bytes.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytes);
        }
    }
}

/// One device, pipelines and grow-only vertex buffers shared by window and capture paths.
pub(crate) struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pipelines: [wgpu::RenderPipeline; 3],
    vertices: [Vertices; 3],
    failure: Arc<Mutex<Option<String>>>,
}

impl Gpu {
    pub async fn new(
        adapter: &wgpu::Adapter,
        format: wgpu::TextureFormat,
    ) -> Result<Self, Problem> {
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Tomo renderer"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(error)?;
        let failure = Arc::new(Mutex::new(None));
        let callback_failure = failure.clone();
        device.on_uncaptured_error(Box::new(move |e| {
            if let Ok(mut slot) = callback_failure.lock() {
                *slot = Some(e.to_string());
            }
        }));
        let lost_failure = failure.clone();
        device.set_device_lost_callback(move |reason, message| {
            if let Ok(mut slot) = lost_failure.lock() {
                *slot = Some(format!("device lost ({reason:?}): {message}"));
            }
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tomo primitives"),
            source: wgpu::ShaderSource::Wgsl(include_str!("primitive.wgsl").into()),
        });
        let pipelines = [
            pipeline(&device, &shader, format, false, false),
            pipeline(&device, &shader, format, true, false),
            pipeline(&device, &shader, format, false, true),
        ];
        let vertices = std::array::from_fn(|_| Vertices::new(&device, 4096));
        let result = Self {
            device,
            queue,
            pipelines,
            vertices,
            failure,
        };
        result.check()?;
        Ok(result)
    }

    pub fn check(&self) -> Result<(), Problem> {
        let failure = self
            .failure
            .lock()
            .map_err(|_| error("GPU error state unavailable"))?;
        if let Some(message) = failure.as_ref() {
            return Err(error(message));
        }
        Ok(())
    }

    pub fn depth(&self, width: u32, height: u32) -> wgpu::TextureView {
        self.device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("Tomo depth"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }

    pub fn encode(
        &mut self,
        geometry: &Geometry,
        view: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) -> wgpu::CommandEncoder {
        self.encode_viewport(geometry, view, depth, None)
    }

    pub fn encode_viewport(
        &mut self,
        geometry: &Geometry,
        view: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        viewport: Option<[u32; 4]>,
    ) -> wgpu::CommandEncoder {
        for (vertices, bytes) in
            self.vertices
                .iter_mut()
                .zip([&geometry.triangles, &geometry.lines, &geometry.overlay])
        {
            vertices.upload(&self.device, &self.queue, bytes);
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Tomo frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Tomo primitives and HUD"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            if let Some([x, y, w, h]) = viewport {
                pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0., 1.);
                pass.set_scissor_rect(x, y, w, h);
            }
            for (pipeline, vertices) in self.pipelines.iter().zip(&self.vertices) {
                if vertices.count > 0 {
                    pass.set_pipeline(pipeline);
                    pass.set_vertex_buffer(0, vertices.buffer.slice(..));
                    pass.draw(0..vertices.count, 0..1);
                }
            }
        }
        encoder
    }
}

pub(crate) async fn render(
    geometry: Geometry,
    snapshot: &Snapshot,
    options: &CaptureOptions,
) -> Result<Capture, Problem> {
    let instance = instance();
    let adapter = adapter(&instance, None).await?;
    let info = adapter.get_info();
    let mut gpu = Gpu::new(&adapter, wgpu::TextureFormat::Rgba8UnormSrgb).await?;
    let size = wgpu::Extent3d {
        width: options.width,
        height: options.height,
        depth_or_array_layers: 1,
    };
    let color = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Tomo capture color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = color.create_view(&Default::default());
    let depth = gpu.depth(options.width, options.height);
    let row_bytes = options.width * 4;
    let padded_row =
        row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Tomo readback"),
        size: u64::from(padded_row) * u64::from(options.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.encode(&geometry, &view, &depth);
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
    gpu.queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    gpu.device.poll(wgpu::PollType::Wait).map_err(error)?;
    gpu.check()?;
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
