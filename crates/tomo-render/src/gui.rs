// SPDX-License-Identifier: MIT
//! Safe, small egui mesh painter using the same pinned wgpu device as the scene.
use crate::gpu::{Gpu, error};
use egui::{ClippedPrimitive, TextureId, TexturesDelta, epaint::Primitive};
use std::{collections::HashMap, ops::Range};
use tomo_scene::Problem;

/// The viewport uses physical window pixels; GUI primitives use egui logical points.
pub struct EditorFrame<'a> {
    pub viewport: [u32; 4],
    pub primitives: &'a [ClippedPrimitive],
    pub textures: &'a TexturesDelta,
    pub pixels_per_point: f32,
}
struct Texture {
    texture: wgpu::Texture,
    bind: wgpu::BindGroup,
}
struct Draw {
    texture: TextureId,
    scissor: [u32; 4],
    range: Range<u32>,
}
pub(crate) struct GuiPainter {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    textures: HashMap<TextureId, Texture>,
    buffer: wgpu::Buffer,
    capacity: u64,
    draws: Vec<Draw>,
}
impl GuiPainter {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Tomo editor texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Tomo editor layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tomo editor shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gui.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Tomo editor pipeline"), layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4] }] },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() }, depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs_main"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })] }),
            multiview: None, cache: None,
        });
        Self {
            pipeline,
            layout,
            textures: HashMap::new(),
            buffer: Self::buffer(device, 4096),
            capacity: 4096,
            draws: vec![],
        }
    }
    fn buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Tomo editor vertices"),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        frame: &EditorFrame<'_>,
        width: u32,
        height: u32,
    ) -> Result<(), Problem> {
        let ppp = frame.pixels_per_point;
        if !ppp.is_finite() || !(0.1..=8.0).contains(&ppp) {
            return Err(error("invalid GUI pixel scale"));
        }
        for (id, delta) in &frame.textures.set {
            let egui::ImageData::Color(image) = &delta.image;
            let [w, h] = image.size;
            if w == 0 || h == 0 || w > 4096 || h > 4096 || image.pixels.len() != w * h {
                return Err(error("invalid GUI texture dimensions"));
            }
            if delta.pos.is_none() {
                if self.textures.len() >= 64 && !self.textures.contains_key(id) {
                    return Err(error("GUI texture limit exceeded"));
                }
                let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Tomo editor atlas"),
                    size: wgpu::Extent3d {
                        width: w as u32,
                        height: h as u32,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                let filter = |f| match f {
                    egui::TextureFilter::Nearest => wgpu::FilterMode::Nearest,
                    egui::TextureFilter::Linear => wgpu::FilterMode::Linear,
                };
                let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("Tomo editor sampler"),
                    mag_filter: filter(delta.options.magnification),
                    min_filter: filter(delta.options.minification),
                    ..Default::default()
                });
                let view = texture.create_view(&Default::default());
                let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Tomo editor texture"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });
                self.textures.insert(*id, Texture { texture, bind });
            }
            let texture = &self
                .textures
                .get(id)
                .ok_or_else(|| error("partial GUI texture update without an atlas"))?
                .texture;
            let [x, y] = delta.pos.unwrap_or([0, 0]);
            if x > texture.width() as usize
                || y > texture.height() as usize
                || w > texture.width() as usize - x
                || h > texture.height() as usize - y
            {
                return Err(error("GUI atlas patch out of bounds"));
            }
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: x as u32,
                        y: y as u32,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w as u32 * 4),
                    rows_per_image: Some(h as u32),
                },
                wgpu::Extent3d {
                    width: w as u32,
                    height: h as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.draws.clear();
        let mut bytes = Vec::new();
        for primitive in frame.primitives {
            let Primitive::Mesh(mesh) = &primitive.primitive else {
                return Err(error("GUI callbacks are not supported"));
            };
            if mesh.indices.len() % 3 != 0
                || bytes.len() / 32 + mesh.indices.len() > 2_000_000
                || !self.textures.contains_key(&mesh.texture_id)
            {
                return Err(error("invalid or oversized GUI mesh"));
            }
            let clip = primitive.clip_rect;
            if !clip.is_finite() {
                return Err(error("invalid GUI clip rectangle"));
            }
            let x = (clip.min.x * ppp).round().clamp(0., width as f32) as u32;
            let y = (clip.min.y * ppp).round().clamp(0., height as f32) as u32;
            let right = (clip.max.x * ppp).round().clamp(x as f32, width as f32) as u32;
            let bottom = (clip.max.y * ppp).round().clamp(y as f32, height as f32) as u32;
            if x == right || y == bottom {
                continue;
            }
            let start = bytes.len() as u32 / 32;
            for &index in &mesh.indices {
                let vertex = mesh
                    .vertices
                    .get(index as usize)
                    .ok_or_else(|| error("invalid GUI mesh index"))?;
                if !vertex.pos.is_finite() || !vertex.uv.is_finite() {
                    return Err(error("non-finite GUI vertex"));
                }
                let position = [
                    2. * vertex.pos.x * ppp / width as f32 - 1.,
                    1. - 2. * vertex.pos.y * ppp / height as f32,
                ];
                for value in position
                    .into_iter()
                    .chain([vertex.uv.x, vertex.uv.y])
                    .chain(vertex.color.to_array().map(|v| f32::from(v) / 255.))
                {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
            }
            self.draws.push(Draw {
                texture: mesh.texture_id,
                scissor: [x, y, right - x, bottom - y],
                range: start..bytes.len() as u32 / 32,
            });
        }
        if bytes.len() as u64 > self.capacity {
            self.capacity = (bytes.len() as u64).next_power_of_two();
            self.buffer = Self::buffer(&gpu.device, self.capacity);
        }
        if !bytes.is_empty() {
            gpu.queue.write_buffer(&self.buffer, 0, &bytes);
        }
        Ok(())
    }
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Tomo editor panels"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        for draw in &self.draws {
            let [x, y, w, h] = draw.scissor;
            pass.set_scissor_rect(x, y, w, h);
            pass.set_bind_group(0, &self.textures[&draw.texture].bind, &[]);
            pass.draw(draw.range.clone(), 0..1);
        }
    }
    pub fn free(&mut self, delta: &TexturesDelta) {
        for id in &delta.free {
            self.textures.remove(id);
        }
    }
}
