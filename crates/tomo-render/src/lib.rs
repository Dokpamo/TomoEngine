// SPDX-License-Identifier: MIT
//! Read-only primitive visualization of a fixed-tick runtime snapshot.
#![forbid(unsafe_code)]

mod geometry;
#[cfg(feature = "editor")]
mod gui;
#[cfg(feature = "editor")]
pub use gui::EditorFrame;
mod gpu;
mod overlay;
mod window;

use serde::{Deserialize, Serialize};
use tomo_runtime::Snapshot;
use tomo_scene::{Problem, Scene};

pub use geometry::{ProjectedBody, pick_body, plane_position, project_bodies};
#[cfg(feature = "audio")]
pub mod audio;
mod content;
pub use content::*;

pub use overlay::{FrameStyle, OverlayRect, OverlayText};
pub use window::WindowRenderer;

pub const MAX_DIMENSION: u32 = 2048;

/// Camera coordinates use the scene's right-handed, +Y-up, metre convention.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Camera {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    pub vertical_fov_degrees: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: [10.0, 8.0, 14.0],
            target: [0.0, 2.0, 0.0],
            up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 50.0,
            near: 0.1,
            far: 1000.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureOptions {
    pub schema_version: u32,
    pub width: u32,
    pub height: u32,
    pub camera: Camera,
    pub colliders: bool,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            schema_version: 1,
            width: 960,
            height: 640,
            camera: Camera::default(),
            colliders: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CaptureMetadata {
    pub schema_version: u32,
    pub tick: u32,
    pub simulation_time: f64,
    pub options: CaptureOptions,
    pub adapter: String,
    pub backend: String,
    pub device_type: String,
    pub bodies: Vec<ProjectedBody>,
}

pub struct Capture {
    pub metadata: CaptureMetadata,
    /// Tightly packed, top-to-bottom RGBA8 sRGB pixels.
    pub rgba: Vec<u8>,
}

impl Capture {
    pub fn png(&self) -> Result<Vec<u8>, Problem> {
        let expected =
            self.metadata.options.width as usize * self.metadata.options.height as usize * 4;
        if self.rgba.len() != expected {
            return Err(Problem::new("RENDER_INPUT", "/rgba", "invalid image size"));
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(
                &mut bytes,
                self.metadata.options.width,
                self.metadata.options.height,
            );
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
            let mut writer = encoder
                .write_header()
                .map_err(|error| Problem::new("RENDER", "/png", error.to_string()))?;
            writer
                .write_image_data(&self.rgba)
                .map_err(|error| Problem::new("RENDER", "/png", error.to_string()))?;
        }
        Ok(bytes)
    }
}

/// Render the supplied snapshot without advancing or modifying its physics world.
/// Adapter/device/readback failure is an error, never an empty successful image.
pub fn capture(
    scene: &Scene,
    snapshot: &Snapshot,
    options: &CaptureOptions,
) -> Result<Capture, Problem> {
    let geometry = geometry::build(scene, snapshot, options)?;
    pollster::block_on(gpu::render(geometry, snapshot, options))
}

/// Capture the same game colors and HUD used by the window, without a window server.
pub fn capture_styled(
    scene: &Scene,
    snapshot: &Snapshot,
    options: &CaptureOptions,
    style: &FrameStyle,
) -> Result<Capture, Problem> {
    let geometry = geometry::build_styled(scene, snapshot, options, style)?;
    pollster::block_on(gpu::render(geometry, snapshot, options))
}
