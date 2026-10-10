// SPDX-License-Identifier: MIT
use egui::{Context, Event, FullOutput, PointerButton, Pos2, RawInput, Rect, Vec2};
use tomo_editor::{
    Editor,
    ui::{EditorUi, theme},
};
use tomo_game::Input;

#[test]
fn preview_pause_and_stop_preserve_authored_scene_and_undo_history() {
    let mut editor = Editor::new(None).unwrap();
    editor.document.add(true).unwrap();
    let authored = editor.document.project().clone();
    let snapshot = editor.document.snapshot().clone();
    editor.start().unwrap();
    for _ in 0..30 {
        editor
            .step(Input {
                right: true,
                ..Default::default()
            })
            .unwrap();
    }
    assert_ne!(editor.game.as_ref().unwrap().snapshot(), &snapshot);
    editor.paused = true;
    let paused = editor.game.as_ref().unwrap().snapshot().clone();
    editor
        .step(Input {
            up: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(editor.game.as_ref().unwrap().snapshot(), &paused);
    editor.stop();
    assert_eq!(editor.document.project(), &authored);
    assert_eq!(editor.document.snapshot(), &snapshot);
    assert!(editor.document.undo().unwrap());
    assert_eq!(editor.document.project().scene.bodies.len(), 9);
}
struct Ui {
    ctx: Context,
    ui: EditorUi,
    editor: Editor,
    time: f64,
}
impl Ui {
    fn new() -> Self {
        let ctx = Context::default();
        theme(&ctx);
        Self {
            ctx,
            ui: EditorUi::default(),
            editor: Editor::new(None).unwrap(),
            time: 0.,
        }
    }
    fn frame(&mut self, events: Vec<Event>) -> FullOutput {
        self.time += 1. / 60.;
        self.ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280., 820.))),
                time: Some(self.time),
                events,
                focused: true,
                ..Default::default()
            },
            |ctx| self.ui.show(ctx, &mut self.editor),
        )
    }
    fn click(&mut self, label: &str) {
        self.frame(vec![]);
        let output = self.frame(vec![]);
        let p = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + text.galley.size() / 2.)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("visible label not found: {label}"));
        self.frame(vec![
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ]);
        self.frame(vec![Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
    }
}
#[test]
fn actual_widgets_add_duplicate_undo_play_stop_and_unsaved_close_guard() {
    let mut app = Ui::new();
    app.click("+ Box");
    assert_eq!(app.editor.document.project().scene.bodies.len(), 10);
    app.click("Duplicate");
    assert_eq!(app.editor.document.project().scene.bodies.len(), 11);
    app.click("Undo");
    assert_eq!(app.editor.document.project().scene.bodies.len(), 10);
    app.click("Play");
    assert!(app.editor.game.is_some());
    app.click("Pause");
    assert!(app.editor.paused);
    app.click("Stop");
    assert!(app.editor.game.is_none());
    app.ui.close_requested = true;
    app.frame(vec![]);
    assert!(!app.ui.exit);
    app.click("Cancel");
    assert!(!app.ui.close_requested);
    app.click("Save");
    assert!(app.ui.blocked());
    app.click("Cancel");
    assert!(!app.ui.blocked());
}

#[test]
fn viewport_pointer_drag_moves_body_and_single_undo_restores_start() {
    let mut app = Ui::new();
    app.frame(vec![]);
    app.frame(vec![]);
    let bodies = tomo_render::project_bodies(
        &app.editor.document.project().scene,
        app.editor.document.snapshot(),
        &app.ui.options(),
    )
    .unwrap();
    let p = bodies
        .iter()
        .find(|b| b.id == "player")
        .unwrap()
        .center_pixel
        .unwrap();
    let options = app.ui.options();
    let rect = app.ui.viewport;
    let pos = rect.min
        + Vec2::new(
            p[0] / options.width as f32 * rect.width(),
            p[1] / options.height as f32 * rect.height(),
        );
    let initial = app
        .editor
        .document
        .project()
        .scene
        .bodies
        .iter()
        .find(|b| b.id == "player")
        .unwrap()
        .position;
    app.frame(vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
    ]);
    for dx in [10., 30., 60., 100.] {
        app.frame(vec![Event::PointerMoved(pos + Vec2::new(dx, 0.))]);
    }
    let end = pos + Vec2::new(100., 0.);
    app.frame(vec![Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);
    assert!(
        app.editor
            .document
            .project()
            .scene
            .bodies
            .iter()
            .find(|b| b.id == "player")
            .unwrap()
            .position[0]
            > initial[0] + 1.
    );
    assert!(app.editor.document.undo().unwrap());
    assert_eq!(
        app.editor
            .document
            .project()
            .scene
            .bodies
            .iter()
            .find(|b| b.id == "player")
            .unwrap()
            .position,
        initial
    );
}

#[test]
fn coalesced_pointer_events_keep_the_complete_drag_and_one_undo() {
    let mut app = Ui::new();
    app.frame(vec![]);
    app.frame(vec![]);
    let options = app.ui.options();
    let rect = app.ui.viewport;
    let bodies = tomo_render::project_bodies(
        &app.editor.document.project().scene,
        app.editor.document.snapshot(),
        &options,
    )
    .unwrap();
    let p = bodies
        .iter()
        .find(|b| b.id == "player")
        .unwrap()
        .center_pixel
        .unwrap();
    let start = rect.min
        + Vec2::new(
            p[0] / options.width as f32 * rect.width(),
            p[1] / options.height as f32 * rect.height(),
        );
    let end = start + Vec2::new(100., 0.);
    app.frame(vec![
        Event::PointerMoved(start),
        Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
        Event::PointerMoved(end),
        Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        },
        Event::PointerMoved(end + Vec2::new(300., 0.)),
    ]);
    assert!(
        app.editor
            .document
            .project()
            .scene
            .bodies
            .iter()
            .find(|b| b.id == "player")
            .unwrap()
            .position[0]
            > -4.
    );
    assert!(
        app.editor
            .document
            .project()
            .scene
            .bodies
            .iter()
            .find(|b| b.id == "player")
            .unwrap()
            .position[0]
            < 0.
    );
    assert!(app.editor.document.undo().unwrap());
    assert!(!app.editor.document.can_undo());
}
