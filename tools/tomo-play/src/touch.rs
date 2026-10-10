// SPDX-License-Identifier: MIT
//! Fixed normalized touch zones; input and painting share the same rectangles.
use crate::Input;
use tomo_render::{CaptureOptions, FrameStyle, OverlayRect, OverlayText};

const CONTROLS: [(&str, [f32; 2], [f32; 2]); 5] = [
    ("UP", [0.14, 0.58], [0.09, 0.09]),
    ("LEFT", [0.04, 0.69], [0.09, 0.09]),
    ("RIGHT", [0.24, 0.69], [0.09, 0.09]),
    ("DOWN", [0.14, 0.80], [0.09, 0.09]),
    ("ACTION", [0.82, 0.72], [0.13, 0.14]),
];

pub fn apply(point: [f32; 2], input: &mut Input) {
    for (i, (_, p, s)) in CONTROLS.iter().enumerate() {
        if (0..2).all(|axis| point[axis] >= p[axis] && point[axis] <= p[axis] + s[axis]) {
            match i {
                0 => input.up = true,
                1 => input.left = true,
                2 => input.right = true,
                3 => input.down = true,
                _ => input.action = true,
            }
        }
    }
}

pub fn overlay(view: &CaptureOptions, style: &mut FrameStyle) {
    let dim = [view.width as f32, view.height as f32];
    for (text, p, s) in CONTROLS {
        let position = [p[0] * dim[0], p[1] * dim[1]];
        let size = [s[0] * dim[0], s[1] * dim[1]];
        let scale = (size[1] * 0.6 / 7.).min(size[0] * 0.8 / (text.len() * 6) as f32);
        style.rectangles.push(OverlayRect {
            position,
            size,
            color: [0.025, 0.06, 0.09],
        });
        style.text.push(OverlayText {
            position: [position[0] + size[0] * 0.1, position[1] + size[1] * 0.2],
            scale,
            text: text.into(),
            color: [0.6, 1., 0.9],
        });
    }
}

#[test]
fn touch_controls_do_not_consume_the_world_or_game_toolbar() {
    let mut input = Input::default();
    for p in [[0.5, 0.5], [0.1, 0.95], [-1., 0.7], [f32::NAN, 0.7]] {
        apply(p, &mut input);
    }
    assert_eq!(input, Input::default());
    apply([0.28, 0.73], &mut input);
    apply([0.88, 0.78], &mut input);
    assert!(input.right && input.action && !input.up && !input.left);
}
