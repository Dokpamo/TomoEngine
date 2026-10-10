// SPDX-License-Identifier: MIT
use crate::{
    CaptureOptions, FrameStyle, geometry,
    gpu::{self, Gpu},
};
use tomo_runtime::Snapshot;
use tomo_scene::{Problem, Scene};

/// Persistent desktop surface. Owned window targets (e.g. `Arc<Window>`) avoid unsafe lifetimes.
pub struct WindowRenderer {
    surface: wgpu::Surface<'static>,
    gpu: Gpu,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    minimized: bool,
    #[cfg(feature = "editor")]
    gui: Option<crate::gui::GuiPainter>,
    pub adapter_name: String,
    pub backend: String,
}

impl WindowRenderer {
    pub fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, Problem> {
        pollster::block_on(Self::create(target.into(), width, height))
    }

    async fn create(
        target: wgpu::SurfaceTarget<'static>,
        width: u32,
        height: u32,
    ) -> Result<Self, Problem> {
        let instance = gpu::instance();
        let surface = instance.create_surface(target).map_err(gpu::error)?;
        let adapter = gpu::adapter(&instance, Some(&surface)).await?;
        let info = adapter.get_info();
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .ok_or_else(|| gpu::error("surface does not support an sRGB format"))?;
        let gpu = Gpu::new(&adapter, format).await?;
        let limit = gpu.device.limits().max_texture_dimension_2d;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.clamp(1, limit),
            height: height.clamp(1, limit),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: *capabilities
                .alpha_modes
                .first()
                .ok_or_else(|| gpu::error("surface has no alpha mode"))?,
            view_formats: vec![],
        };
        surface.configure(&gpu.device, &config);
        let depth = gpu.depth(config.width, config.height);
        gpu.check()?;
        Ok(Self {
            surface,
            gpu,
            config,
            depth,
            minimized: width == 0 || height == 0,
            #[cfg(feature = "editor")]
            gui: None,
            adapter_name: info.name,
            backend: format!("{:?}", info.backend),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), Problem> {
        self.minimized = width == 0 || height == 0;
        if self.minimized {
            return Ok(());
        }
        let limit = self.gpu.device.limits().max_texture_dimension_2d;
        self.config.width = width.min(limit);
        self.config.height = height.min(limit);
        self.surface.configure(&self.gpu.device, &self.config);
        self.depth = self.gpu.depth(self.config.width, self.config.height);
        self.gpu.check()
    }

    /// False means a minimized window or recoverable surface timeout/loss. The next frame retries.
    pub fn present(
        &mut self,
        scene: &Scene,
        snapshot: &Snapshot,
        options: &CaptureOptions,
        style: &FrameStyle,
    ) -> Result<bool, Problem> {
        self.gpu.check()?;
        if self.minimized {
            return Ok(false);
        }
        let geometry = geometry::build_styled(scene, snapshot, options, style)?;
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.resize(self.config.width, self.config.height)?;
                return Ok(false);
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(false),
            Err(error) => return Err(gpu::error(error)),
        };
        let view = frame.texture.create_view(&Default::default());
        let encoder = self.gpu.encode(&geometry, &view, &self.depth);
        self.gpu.queue.submit([encoder.finish()]);
        frame.present();
        self.gpu
            .device
            .poll(wgpu::PollType::Poll)
            .map_err(gpu::error)?;
        self.gpu.check()?;
        Ok(true)
    }
}

#[cfg(feature = "editor")]
impl WindowRenderer {
    /// Paint a scene viewport and egui panels on one surface, sharing its device and lifecycle.
    pub fn present_editor(
        &mut self,
        scene: &Scene,
        snapshot: &Snapshot,
        options: &CaptureOptions,
        style: &FrameStyle,
        editor: &crate::EditorFrame<'_>,
    ) -> Result<bool, Problem> {
        self.gpu.check()?;
        let [x, y, w, h] = editor.viewport;
        if w == 0
            || h == 0
            || x >= self.config.width
            || y >= self.config.height
            || w > self.config.width - x
            || h > self.config.height - y
        {
            return Err(gpu::error("editor viewport outside the window"));
        }
        let geometry = geometry::build_styled(scene, snapshot, options, style)?;
        let gui = self
            .gui
            .get_or_insert_with(|| crate::gui::GuiPainter::new(&self.gpu, self.config.format));
        // Consume texture changes even if surface acquisition is recoverably lost.
        gui.prepare(&self.gpu, editor, self.config.width, self.config.height)?;
        if self.minimized {
            gui.free(editor.textures);
            return Ok(false);
        }
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                gui.free(editor.textures);
                self.resize(self.config.width, self.config.height)?;
                return Ok(false);
            }
            Err(wgpu::SurfaceError::Timeout) => {
                gui.free(editor.textures);
                return Ok(false);
            }
            Err(e) => return Err(gpu::error(e)),
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder =
            self.gpu
                .encode_viewport(&geometry, &view, &self.depth, Some(editor.viewport));
        gui.encode(&mut encoder, &view);
        self.gpu.queue.submit([encoder.finish()]);
        frame.present();
        gui.free(editor.textures);
        self.gpu
            .device
            .poll(wgpu::PollType::Poll)
            .map_err(gpu::error)?;
        self.gpu.check()?;
        Ok(true)
    }
}
