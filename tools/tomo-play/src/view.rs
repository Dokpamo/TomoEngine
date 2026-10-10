// SPDX-License-Identifier: MIT
use crate::{Game, Status};
use tomo_render::{Camera, CaptureOptions, FrameStyle, OverlayRect, OverlayText};

pub fn options(width: u32, height: u32, colliders: bool) -> CaptureOptions {
    let scale = (tomo_render::MAX_DIMENSION as f32 / width.max(height).max(1) as f32).min(1.0);
    let width = (width as f32 * scale).max(1.0) as u32;
    let height = (height as f32 * scale).max(1.0) as u32;
    let zoom = (1.35 / (width as f32 / height as f32)).max(1.0);
    CaptureOptions {
        width,
        height,
        colliders,
        camera: Camera {
            eye: [0.0, 18.0 * zoom, 12.0 * zoom],
            target: [0.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 46.0,
            near: 0.1,
            far: 200.0,
        },
        ..CaptureOptions::default()
    }
}

pub fn style(game: &Game, view: &CaptureOptions, paused: bool) -> FrameStyle {
    let mut style = FrameStyle::default();
    for body in &game.scene().bodies {
        let color = match body.id.as_str() {
            id if id == game.rules().player => [1.0, 0.38, 0.07],
            id if id == game.rules().goal => [0.08, 0.95, 0.55],
            "floor" => [0.105, 0.16, 0.22],
            "gate_a" | "gate_b" => [0.34, 0.46, 0.57],
            _ => [0.19, 0.29, 0.38],
        };
        style.body_colors.insert(body.id.clone(), color);
    }
    let s = (view.width as f32 / 1120.0).min(view.height as f32 / 760.0);
    let white = [0.9, 0.95, 1.0];
    let muted = [0.43, 0.59, 0.72];
    let mint = [0.1, 0.95, 0.6];
    let panel = [0.018, 0.031, 0.052];
    let h = view.height as f32 / s;
    let w = view.width as f32 / s;
    let mut rect = |x, y, width, height, color| {
        style.rectangles.push(OverlayRect {
            position: [x * s, y * s],
            size: [width * s, height * s],
            color,
        })
    };
    rect(0., 0., w, 102., panel);
    rect(30., 28., 5., 47., mint);
    rect(0., h - 75., w, 75., panel);
    let mut text = |x, y, scale, value: String, color| {
        style.text.push(OverlayText {
            position: [x * s, y * s],
            scale: scale * s,
            text: value,
            color,
        })
    };
    let title: String = game
        .scene()
        .name
        .to_ascii_uppercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || " .:/-+".contains(c) {
                c
            } else {
                ' '
            }
        })
        .take(18)
        .collect();
    text(51., 27., 4., title, white);
    text(51., 67., 1.5, "TOMO ENGINE / 01".into(), muted);
    text(w - 330., 28., 1.7, "TIME".into(), muted);
    text(
        w - 330.,
        52.,
        3.,
        format!("{:05.2} S", game.seconds()),
        white,
    );
    text(w - 155., 28., 1.7, "SESSION BEST".into(), muted);
    text(
        w - 155.,
        57.,
        2.,
        game.best_seconds()
            .map_or("--.-- S".into(), |t| format!("{t:05.2} S")),
        mint,
    );
    text(31., h - 52., 2., "WASD / ARROWS".into(), white);
    text(31., h - 27., 1.5, "MOVE THE ORANGE BOX".into(), muted);
    text(
        w - 360.,
        h - 51.,
        1.7,
        "R RESTART  /  ESC QUIT".into(),
        white,
    );
    text(
        w - 360.,
        h - 25.,
        1.5,
        "C COLLIDERS  /  P PAUSE".into(),
        muted,
    );
    let headline = match game.status() {
        Status::Ready => "REACH THE GREEN PAD",
        Status::Playing => "FIND YOUR WAY THROUGH THE GATES",
        Status::Won => "DESTINATION REACHED",
    };
    let title = if paused { "PAUSED" } else { headline };
    text(
        (w - title.len() as f32 * 12.) * 0.5,
        124.,
        2.,
        title.into(),
        mint,
    );
    if game.status() == Status::Won {
        let cx = w * 0.5;
        let cy = h * 0.5;
        style.rectangles.push(OverlayRect {
            position: [(cx - 237.) * s, (cy - 80.) * s],
            size: [474. * s, 160. * s],
            color: panel,
        });
        style.rectangles.push(OverlayRect {
            position: [(cx - 237.) * s, (cy - 80.) * s],
            size: [474. * s, 4. * s],
            color: mint,
        });
        style.text.push(OverlayText {
            position: [(cx - 126.) * s, (cy - 43.) * s],
            scale: 6. * s,
            text: "YOU WIN".into(),
            color: white,
        });
        style.text.push(OverlayText {
            position: [(cx - 174.) * s, (cy + 23.) * s],
            scale: 2. * s,
            text: "PRESS R TO BEAT YOUR TIME".into(),
            color: mint,
        });
    }
    style
}
