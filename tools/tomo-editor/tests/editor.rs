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
    size: Vec2,
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
            size: Vec2::new(1280., 820.),
        }
    }
    fn frame(&mut self, events: Vec<Event>) -> FullOutput {
        self.time += 1. / 60.;
        self.ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
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
            .unwrap_or_else(|| {
                let labels: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) => Some((&text.galley.job.text, text.pos)),
                        _ => None,
                    })
                    .collect();
                panic!("visible label not found: {label}; labels: {labels:?}")
            });
        assert!(
            Rect::from_min_size(Pos2::ZERO, self.size).contains(p),
            "button outside the window: {label} at {p:?}"
        );
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

#[test]
fn actual_logic_widgets_apply_jump_and_score_then_play_save_reload_and_undo() {
    let mut app = Ui::new();
    app.click("Logic");
    app.click("+ Space jump");
    app.click("+ Rule");
    assert_eq!(
        app.editor.document.project().game.logic.rules.len(),
        2,
        "draft is not live"
    );
    app.click("Apply rules");
    assert!(!app.editor.error, "{}", app.editor.message);
    app.click("Done");
    assert!(!app.ui.blocked());
    assert_eq!(app.editor.document.project().game.logic.rules.len(), 4);
    let authored = app.editor.document.project().clone();
    let initial = app.editor.document.snapshot().clone();
    app.click("Play");
    let input = Input {
        action: true,
        ..Default::default()
    };
    for _ in 0..12 {
        app.editor.step(input).unwrap();
    }
    let game = app.editor.game.as_ref().unwrap();
    assert_eq!(game.score(), 1);
    assert!(game.player().position[1] > 1.);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("tomo-logic-ui-{}-{nonce}", std::process::id()));
    tomo_project::ProjectFile::create(&directory, &authored).unwrap();
    let saved = tomo_project::ProjectFile::open(&directory).unwrap();
    let mut independent = saved.project().game().unwrap();
    for _ in 0..12 {
        independent.step(input).unwrap();
    }
    assert_eq!(independent.snapshot(), game.snapshot());
    assert_eq!(independent.score(), game.score());
    std::fs::remove_dir_all(directory).unwrap();
    app.click("Stop");
    assert_eq!(app.editor.document.snapshot(), &initial);
    assert_eq!(app.editor.document.project(), &authored);
    app.click("Undo");
    assert_eq!(app.editor.document.project().game.logic.rules.len(), 2);
    app.click("Redo");
    assert_eq!(app.editor.document.project().game.logic.rules.len(), 4);
}

#[test]
fn logic_drafts_reject_invalid_actions_conflicting_revisions_and_accidental_close() {
    let mut app = Ui::new();
    let original = app.editor.document.project().clone();
    app.click("Logic");
    app.click("+ Rule");
    app.click("Remove"); // the only action in the selected new rule
    app.click("Apply rules");
    assert!(app.editor.error);
    assert_eq!(app.editor.document.project(), &original);
    assert!(!app.editor.document.can_undo());
    app.ui.close_requested = true;
    app.frame(vec![]);
    assert!(!app.ui.exit);
    assert!(app.ui.blocked());
    app.click("+ Action");
    app.editor.document.add(true).unwrap(); // another client changed the document revision
    let newer = app.editor.document.project().clone();
    app.click("Apply rules");
    assert!(app.editor.error);
    assert!(app.editor.message.starts_with("CONFLICT"));
    assert_eq!(app.editor.document.project(), &newer);
    app.click("Reload current");
    app.click("Done");
    assert!(!app.ui.blocked());
    app.click("Logic");
    app.click("+ Time limit");
    app.click("Discard & close");
    assert_eq!(app.editor.document.project(), &newer);
}

#[test]
fn logic_controls_remain_usable_at_minimum_window_size_after_validation_error() {
    let mut app = Ui::new();
    app.size = Vec2::new(1000., 640.);
    app.click("Logic");
    app.click("+ Rule");
    app.click("Remove");
    app.click("Apply rules");
    assert!(app.editor.error);
    app.click("Reload current");
    app.click("Done");
    assert!(!app.ui.blocked());
}

#[test]
fn dropdowns_author_a_conditional_keyboard_win_without_template_code_changes() {
    let mut app = Ui::new();
    app.click("Logic");
    app.click("+ Rule"); // start score +1
    app.click("+ Time limit");
    app.click("Timer");
    app.click("Key pressed");
    app.click("Lose game");
    app.click("Win game");
    app.click("+ Condition"); // score >= 1
    app.click("Apply rules");
    assert!(!app.editor.error, "{}", app.editor.message);
    app.click("Done");
    app.click("Play");
    app.editor
        .step(Input {
            action: true,
            ..Default::default()
        })
        .unwrap();
    let game = app.editor.game.as_ref().unwrap();
    assert_eq!(game.status(), tomo_game::Status::Won);
    assert_eq!(game.score(), 1);
    assert_eq!(game.elapsed_ticks(), 1);
    assert!(
        (game.player().position[0] + 5.).abs() < 0.001,
        "winning did not require reaching the goal"
    );
}

#[test]
fn studio_widgets_create_visual_prefab_ui_sound_and_scene_in_one_undoable_document() {
    let mut app = Ui::new();
    app.click("Studio");
    app.click("+ Box visual");
    assert!(app.editor.document.project().content.entities.is_empty());
    app.click("Apply changes");
    app.click("Done");
    assert_eq!(app.editor.document.project().content.entities.len(), 1);
    app.click("Studio");
    app.click("Prefabs");
    app.click("Capture prefab");
    app.click("Instantiate at +2m X");
    app.click("Game UI");
    app.click("+ UI element");
    app.click("Audio");
    app.click("+ Sound cue");
    app.click("Scenes");
    app.click("Duplicate scene");
    app.click("Apply changes");
    app.click("Done");
    let p = app.editor.document.project();
    assert_eq!(p.content.entities.len(), 2);
    assert_eq!(p.prefabs.len(), 1);
    assert_eq!(p.scenes.len(), 1);
    assert_eq!(p.content.ui.len(), 1);
    assert_eq!(p.content.sounds.len(), 1);
    app.click("Undo");
    assert_eq!(app.editor.document.project().content.entities.len(), 1);
    assert!(app.editor.document.project().prefabs.is_empty());
    app.click("Redo");
    app.click("Play");
    assert_eq!(
        app.editor.game.as_ref().unwrap().content().entities.len(),
        2
    );
    app.click("Stop");
}
