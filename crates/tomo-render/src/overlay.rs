// SPDX-License-Identifier: MIT
use crate::CaptureOptions;
use std::collections::BTreeMap;
use tomo_scene::{Problem, Scene};

/// Presentation data stays outside the physics scene. Colors are linear RGB in 0..1.
#[derive(Clone, Default)]
pub struct FrameStyle {
    pub body_colors: BTreeMap<String, [f32; 3]>,
    pub rectangles: Vec<OverlayRect>,
    pub text: Vec<OverlayText>,
}

#[derive(Clone)]
pub struct OverlayRect {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 3],
}

#[derive(Clone)]
pub struct OverlayText {
    pub position: [f32; 2],
    /// Size of one pixel of the built-in 5x7 ASCII font, in viewport pixels.
    pub scale: f32,
    pub text: String,
    pub color: [f32; 3],
}

fn valid_color(color: &[f32; 3]) -> bool {
    color
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}
fn valid_position(position: &[f32; 2]) -> bool {
    position.iter().all(|v| v.is_finite() && v.abs() <= 16384.0)
}

impl FrameStyle {
    pub(crate) fn validate(&self, scene: &Scene) -> Result<(), Problem> {
        if self.body_colors.len() > tomo_scene::MAX_BODIES
            || self.rectangles.len() > 128
            || self.text.iter().map(|t| t.text.len()).sum::<usize>() > 2048
            || self.text.len() > 64
        {
            return Err(Problem::new(
                "LIMIT",
                "/style",
                "too many colors, rectangles or text characters",
            ));
        }
        for (id, color) in &self.body_colors {
            if !valid_color(color) || !scene.bodies.iter().any(|body| body.id == *id) {
                return Err(Problem::new(
                    "RENDER_INPUT",
                    "/style/body_colors",
                    "unknown body or invalid color",
                ));
            }
        }
        for rect in &self.rectangles {
            if !valid_position(&rect.position)
                || !valid_color(&rect.color)
                || !rect
                    .size
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=16384.0).contains(v))
            {
                return Err(Problem::new(
                    "RENDER_INPUT",
                    "/style/rectangles",
                    "invalid rectangle",
                ));
            }
        }
        for text in &self.text {
            if !valid_position(&text.position)
                || !valid_color(&text.color)
                || !text.scale.is_finite()
                || !(0.001..=32.0).contains(&text.scale)
                || !text
                    .text
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || b" .:/-+".contains(&c))
            {
                return Err(Problem::new(
                    "RENDER_INPUT",
                    "/style/text",
                    "invalid text; use uppercase ASCII, digits and .:/-+",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn vertices(&self, options: &CaptureOptions) -> Vec<u8> {
        let mut output = Vec::new();
        let mut rectangle = |position: [f32; 2], size: [f32; 2], color: [f32; 3]| {
            for [dx, dy] in [[0., 0.], [1., 0.], [1., 1.], [0., 0.], [1., 1.], [0., 1.]] {
                let clip = [
                    (position[0] + dx * size[0]) / options.width as f32 * 2. - 1.,
                    1. - (position[1] + dy * size[1]) / options.height as f32 * 2.,
                    0.,
                    1.,
                ];
                for value in clip.into_iter().chain([0.; 3]).chain(color) {
                    output.extend_from_slice(&value.to_le_bytes());
                }
            }
        };
        for rect in &self.rectangles {
            rectangle(rect.position, rect.size, rect.color);
        }
        for text in &self.text {
            for (index, character) in text.text.bytes().enumerate() {
                for (row, bits) in glyph(character).into_iter().enumerate() {
                    for column in 0..5 {
                        if bits & (1 << (4 - column)) != 0 {
                            rectangle(
                                [
                                    text.position[0] + (index * 6 + column) as f32 * text.scale,
                                    text.position[1] + row as f32 * text.scale,
                                ],
                                [text.scale; 2],
                                text.color,
                            );
                        }
                    }
                }
            }
        }
        output
    }
}

// Original, deliberately small bitmap alphabet; no external font or texture assets.
fn glyph(c: u8) -> [u8; 7] {
    match c {
        b'A' => [14, 17, 17, 31, 17, 17, 17],
        b'B' => [30, 17, 17, 30, 17, 17, 30],
        b'C' => [14, 17, 16, 16, 16, 17, 14],
        b'D' => [30, 17, 17, 17, 17, 17, 30],
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'F' => [31, 16, 16, 30, 16, 16, 16],
        b'G' => [14, 17, 16, 23, 17, 17, 15],
        b'H' => [17, 17, 17, 31, 17, 17, 17],
        b'I' => [31, 4, 4, 4, 4, 4, 31],
        b'J' => [7, 2, 2, 2, 2, 18, 12],
        b'K' => [17, 18, 20, 24, 20, 18, 17],
        b'L' => [16, 16, 16, 16, 16, 16, 31],
        b'M' => [17, 27, 21, 21, 17, 17, 17],
        b'N' => [17, 25, 21, 19, 17, 17, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'P' => [30, 17, 17, 30, 16, 16, 16],
        b'Q' => [14, 17, 17, 17, 21, 18, 13],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'T' => [31, 4, 4, 4, 4, 4, 4],
        b'U' => [17, 17, 17, 17, 17, 17, 14],
        b'V' => [17, 17, 17, 17, 17, 10, 4],
        b'W' => [17, 17, 17, 21, 21, 21, 10],
        b'X' => [17, 17, 10, 4, 10, 17, 17],
        b'Y' => [17, 17, 10, 4, 4, 4, 4],
        b'Z' => [31, 1, 2, 4, 8, 16, 31],
        b'0' => [14, 17, 19, 21, 25, 17, 14],
        b'1' => [4, 12, 4, 4, 4, 4, 14],
        b'2' => [14, 17, 1, 2, 4, 8, 31],
        b'3' => [30, 1, 1, 14, 1, 1, 30],
        b'4' => [2, 6, 10, 18, 31, 2, 2],
        b'5' => [31, 16, 16, 30, 1, 1, 30],
        b'6' => [14, 16, 16, 30, 17, 17, 14],
        b'7' => [31, 1, 2, 4, 8, 8, 8],
        b'8' => [14, 17, 17, 14, 17, 17, 14],
        b'9' => [14, 17, 17, 15, 1, 1, 14],
        b'.' => [0, 0, 0, 0, 0, 6, 6],
        b':' => [0, 6, 6, 0, 6, 6, 0],
        b'/' => [1, 2, 2, 4, 8, 8, 16],
        b'-' => [0, 0, 0, 31, 0, 0, 0],
        b'+' => [0, 4, 4, 31, 4, 4, 0],
        _ => [0; 7],
    }
}
