// SPDX-License-Identifier: MIT
use crate::Editor;
use egui::{ComboBox, DragValue, Ui};
use tomo_project::Project;
use tomo_scene::{
    CameraSpec, CueTrigger, Entity, SoundCue, UiAction, Visibility, Widget, WidgetKind,
};

#[derive(Default)]
pub(crate) struct StudioUi {
    pub open: bool,
    draft: Option<Project>,
    revision: u64,
    tab: usize,
    selected: usize,
    prefab: usize,
    message: String,
}
fn xyz(ui: &mut Ui, label: &str, v: &mut [f32; 3], speed: f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        for n in v {
            ui.add(DragValue::new(n).speed(speed));
        }
    });
}
fn choose(ui: &mut Ui, label: &str, value: &mut Option<String>, ids: &[String]) {
    ComboBox::from_label(label)
        .selected_text(value.as_deref().unwrap_or("None"))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "None");
            for id in ids {
                ui.selectable_value(value, Some(id.clone()), id);
            }
        });
}
fn unique(prefix: &str, ids: impl Iterator<Item = String>) -> String {
    let ids: Vec<_> = ids.collect();
    (1..)
        .map(|i| format!("{prefix}_{i}"))
        .find(|s| !ids.contains(s))
        .expect("available ID")
}
impl StudioUi {
    pub fn open(&mut self, e: &Editor) {
        self.draft = Some(e.document.project().clone());
        self.revision = e.document.project().revision;
        self.open = true;
        self.selected = 0;
        self.message.clear();
    }
    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor, camera: &tomo_render::Camera) {
        if !self.open {
            return;
        }
        let Some(p) = &mut self.draft else {
            return;
        };
        egui::Window::new("Game Studio")
            .id(egui::Id::new("studio"))
            .default_size([860., 650.])
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (i, label) in [
                        "Objects", "Cameras", "Scenes", "Prefabs", "Game UI", "Audio",
                    ]
                    .iter()
                    .enumerate()
                    {
                        if ui.selectable_value(&mut self.tab, i, *label).clicked() {
                            self.selected = 0;
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height((ctx.screen_rect().height() - 220.).clamp(160., 505.))
                    .show(ui, |ui| match self.tab {
                        0 => objects(ui, p, &mut self.selected),
                        1 => cameras(ui, p, &mut self.selected, camera),
                        2 => scenes(ui, p, &mut self.message),
                        3 => prefabs(
                            ui,
                            p,
                            &mut self.selected,
                            &mut self.prefab,
                            &mut self.message,
                        ),
                        4 => widgets(ui, p, &mut self.selected),
                        _ => sounds(ui, p, &mut self.selected),
                    });
                ui.separator();
                ui.label(&self.message);
                ui.horizontal(|ui| {
                    if ui.button("Apply changes").clicked() {
                        match editor.document.apply(p.clone(), self.revision) {
                            Ok(_) => {
                                *p = editor.document.project().clone();
                                self.revision = p.revision;
                                self.message =
                                    "Applied. Undo restores the previous document.".into();
                                editor.selected = None;
                            }
                            Err(e) => self.message = e.to_string(),
                        }
                    }
                    let clean = *p == *editor.document.project();
                    if ui.add_enabled(clean, egui::Button::new("Done")).clicked() {
                        self.open = false;
                    }
                    if ui.button("Discard draft / Close").clicked() {
                        self.open = false;
                    }
                    ui.small(
                        "Apply validates the whole project. Close this window before Play or Save.",
                    );
                });
            });
    }
}
fn objects(ui: &mut Ui, p: &mut Project, selected: &mut usize) {
    ui.horizontal(|ui| {
        for (label, sphere, empty) in [
            ("+ Box visual", false, false),
            ("+ Sphere visual", true, false),
            ("+ Empty", false, true),
        ] {
            if ui.button(label).clicked() {
                let id = unique("object", p.content.entities.iter().map(|e| e.id.clone()));
                let mut e = Entity::decoration(&id, sphere);
                if empty {
                    e.visual = None;
                }
                p.content.entities.push(e);
                *selected = p.content.entities.len() - 1;
            }
        }
    });
    if p.content.entities.is_empty() {
        ui.label(
            "Objects can render without colliders. Bind a body to replace its default appearance.",
        );
        return;
    }
    *selected = (*selected).min(p.content.entities.len() - 1);
    ComboBox::from_label("Object")
        .selected_text(&p.content.entities[*selected].name)
        .show_ui(ui, |ui| {
            for (i, e) in p.content.entities.iter().enumerate() {
                ui.selectable_value(selected, i, &e.name);
            }
        });
    let ids = p
        .content
        .entities
        .iter()
        .map(|e| e.id.clone())
        .collect::<Vec<_>>();
    let e = &mut p.content.entities[*selected];
    ui.horizontal(|ui| {
        ui.label("ID");
        ui.label(&e.id);
        ui.label("Name");
        ui.text_edit_singleline(&mut e.name);
    });
    choose(
        ui,
        "Parent",
        &mut e.parent,
        &ids.iter()
            .filter(|id| **id != e.id)
            .cloned()
            .collect::<Vec<_>>(),
    );
    choose(
        ui,
        "Physics body",
        &mut e.body,
        &p.scene
            .bodies
            .iter()
            .map(|b| b.id.clone())
            .collect::<Vec<_>>(),
    );
    xyz(ui, "Local position", &mut e.transform.position, 0.05);
    let mut angles = crate::ui::to_euler(e.transform.rotation);
    let before = angles;
    xyz(ui, "Rotation degrees", &mut angles, 1.);
    if angles != before {
        e.transform.rotation = crate::ui::from_euler(angles);
    }
    ui.horizontal(|ui| {
        ui.label("Uniform scale");
        ui.add(
            DragValue::new(&mut e.transform.scale)
                .speed(0.02)
                .range(0.001..=100.),
        );
    });
    xyz(ui, "Spin degrees/sec", &mut e.spin_degrees_per_second, 1.);
    let mut visible = e.visual.is_some();
    if ui.checkbox(&mut visible, "Visual component").changed() {
        e.visual = if visible {
            Entity::decoration("new", false).visual
        } else {
            None
        };
    }
    if let Some(v) = &mut e.visual {
        ui.checkbox(&mut v.visible, "Visible");
        ui.color_edit_button_rgb(&mut v.color);
        let mut sphere = matches!(v.shape, tomo_scene::ShapeSpec::Sphere { .. });
        if ui.checkbox(&mut sphere, "Sphere").changed() {
            v.shape = if sphere {
                tomo_scene::ShapeSpec::Sphere { radius: 0.5 }
            } else {
                tomo_scene::ShapeSpec::Box {
                    half_extents: [0.5; 3],
                }
            };
        }
        match &mut v.shape {
            tomo_scene::ShapeSpec::Box { half_extents } => {
                xyz(ui, "Half extents", half_extents, 0.02)
            }
            tomo_scene::ShapeSpec::Sphere { radius } => {
                ui.add(
                    DragValue::new(radius)
                        .speed(0.02)
                        .range(0.0001..=10000.)
                        .prefix("Radius "),
                );
            }
        }
    }
    let mut tags = e.tags.join(",");
    ui.horizontal(|ui| {
        ui.label("Tags, comma separated");
        if ui.text_edit_singleline(&mut tags).changed() {
            e.tags = tags
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        }
    });
    if ui.button("Delete object").clicked() {
        p.content.entities.remove(*selected);
    }
    ui.small("Deleting a referenced parent or camera target is rejected on Apply. Detach references first.");
}
fn cameras(ui: &mut Ui, p: &mut Project, selected: &mut usize, view: &tomo_render::Camera) {
    if ui.button("+ Camera").clicked() {
        let id = unique("camera", p.content.cameras.iter().map(|c| c.id.clone()));
        p.content.cameras.push(CameraSpec {
            id,
            ..Default::default()
        });
        *selected = p.content.cameras.len() - 1;
    }
    *selected = (*selected).min(p.content.cameras.len().saturating_sub(1));
    ComboBox::from_label("Camera")
        .selected_text(&p.content.cameras[*selected].id)
        .show_ui(ui, |ui| {
            for (i, c) in p.content.cameras.iter().enumerate() {
                ui.selectable_value(selected, i, &c.id);
            }
        });
    let c = &mut p.content.cameras[*selected];
    if ui.button("Use editor view").clicked() {
        c.eye = view.eye;
        c.target = view.target;
        c.up = view.up;
        c.vertical_fov_degrees = view.vertical_fov_degrees;
        c.follow = None;
    }
    if ui.button("Set as game camera").clicked() {
        p.content.active_camera = c.id.clone();
    }
    ui.label(format!("Active: {}", p.content.active_camera));
    xyz(ui, "Eye", &mut c.eye, 0.1);
    xyz(ui, "Target", &mut c.target, 0.1);
    xyz(ui, "Up", &mut c.up, 0.01);
    ui.horizontal(|ui| {
        ui.add(
            DragValue::new(&mut c.vertical_fov_degrees)
                .range(5. ..=150.)
                .prefix("FOV "),
        );
        ui.add(DragValue::new(&mut c.near).speed(0.01).prefix("Near "));
        ui.add(DragValue::new(&mut c.far).speed(1.).prefix("Far "));
    });
    choose(
        ui,
        "Follow entity",
        &mut c.follow,
        &p.content
            .entities
            .iter()
            .map(|e| e.id.clone())
            .collect::<Vec<_>>(),
    );
    if p.content.cameras.len() > 1 && ui.button("Delete camera").clicked() {
        p.content.cameras.remove(*selected);
        p.content.active_camera = p.content.cameras[0].id.clone();
    }
}
fn scenes(ui: &mut Ui, p: &mut Project, message: &mut String) {
    ui.label(format!("Editing scene: {}", p.scene_id));
    let mut id = p.scene_id.clone();
    ComboBox::from_label("Switch scene")
        .selected_text(&id)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut id, p.scene_id.clone(), &p.scene_id);
            for s in &p.scenes {
                ui.selectable_value(&mut id, s.id.clone(), &s.id);
            }
        });
    if id != p.scene_id {
        if let Err(e) = p.switch_scene(&id) {
            *message = e.to_string();
        }
    }
    if ui.button("Duplicate scene").clicked() {
        let id = unique(
            "scene",
            std::iter::once(p.scene_id.clone()).chain(p.scenes.iter().map(|s| s.id.clone())),
        );
        if let Err(e) = p.duplicate_scene(&id) {
            *message = e.to_string();
        }
    }
    choose(
        ui,
        "Next scene",
        &mut p.next_scene,
        &p.scenes.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
    );
    ui.label("Next scene is loaded by a game UI button or N in the player. Each scene starts with fresh physics and rules.");
    if ui.button("Use role-free rules").clicked() {
        p.game.player.clear();
        p.game.goal.clear();
        p.game.logic.rules.clear();
        p.game.logic.start = tomo_game::StartMode::Immediate;
    }
    ui.small("Role-free mode removes the default movement/goal rules. Other events and Rust SDK behavior remain available.");
}
fn prefabs(
    ui: &mut Ui,
    p: &mut Project,
    selected: &mut usize,
    prefab: &mut usize,
    message: &mut String,
) {
    ui.label("Capture a root object and its descendants, including bound physics bodies.");
    if !p.content.entities.is_empty() {
        *selected = (*selected).min(p.content.entities.len() - 1);
        ComboBox::from_label("Root object")
            .selected_text(&p.content.entities[*selected].name)
            .show_ui(ui, |ui| {
                for (i, e) in p.content.entities.iter().enumerate() {
                    ui.selectable_value(selected, i, &e.name);
                }
            });
        if ui.button("Capture prefab").clicked() {
            let root = p.content.entities[*selected].id.clone();
            let id = unique("prefab", p.prefabs.iter().map(|p| p.id.clone()));
            if let Err(e) = p.capture_prefab(&root, &id) {
                *message = e.to_string();
            }
        }
    }
    if !p.prefabs.is_empty() {
        *prefab = (*prefab).min(p.prefabs.len() - 1);
        ComboBox::from_label("Prefab")
            .selected_text(&p.prefabs[*prefab].id)
            .show_ui(ui, |ui| {
                for (i, e) in p.prefabs.iter().enumerate() {
                    ui.selectable_value(prefab, i, &e.id);
                }
            });
        if ui.button("Instantiate at +2m X").clicked() {
            let id = p.prefabs[*prefab].id.clone();
            let prefix = (1..)
                .map(|i| format!("instance_{i}"))
                .find(|prefix| {
                    !p.content
                        .entities
                        .iter()
                        .any(|e| e.id.starts_with(&format!("{prefix}_")))
                        && !p
                            .scene
                            .bodies
                            .iter()
                            .any(|e| e.id.starts_with(&format!("{prefix}_")))
                })
                .unwrap();
            if let Err(e) = p.instantiate(&id, &prefix, [2., 0., 0.]) {
                *message = e.to_string();
            }
        }
        if ui.button("Delete template").clicked() {
            p.prefabs.remove(*prefab);
        }
    }
    ui.small("Instances are independent copies. Template changes affect future instances.");
}
fn widgets(ui: &mut Ui, p: &mut Project, selected: &mut usize) {
    ui.checkbox(&mut p.content.default_hud, "Show built-in game HUD");
    if ui.button("+ UI element").clicked() {
        let id = unique("ui", p.content.ui.iter().map(|w| w.id.clone()));
        p.content.ui.push(Widget::label(&id));
        *selected = p.content.ui.len() - 1;
    }
    if p.content.ui.is_empty() {
        return;
    }
    *selected = (*selected).min(p.content.ui.len() - 1);
    ComboBox::from_label("Element")
        .selected_text(&p.content.ui[*selected].id)
        .show_ui(ui, |ui| {
            for (i, w) in p.content.ui.iter().enumerate() {
                ui.selectable_value(selected, i, &w.id);
            }
        });
    let w = &mut p.content.ui[*selected];
    let old = match w.kind {
        WidgetKind::Label {} => 0,
        WidgetKind::Score {} => 1,
        WidgetKind::Timer {} => 2,
        WidgetKind::Status {} => 3,
        WidgetKind::Progress { .. } => 4,
        WidgetKind::Button { .. } => 5,
    };
    let mut kind = old;
    ComboBox::from_label("Type")
        .selected_text(["Label", "Score", "Timer", "Status", "Progress", "Button"][kind])
        .show_ui(ui, |ui| {
            for (i, t) in ["Label", "Score", "Timer", "Status", "Progress", "Button"]
                .iter()
                .enumerate()
            {
                ui.selectable_value(&mut kind, i, *t);
            }
        });
    if old != kind {
        w.kind = match kind {
            0 => WidgetKind::Label {},
            1 => WidgetKind::Score {},
            2 => WidgetKind::Timer {},
            3 => WidgetKind::Status {},
            4 => WidgetKind::Progress { max_score: 100 },
            _ => WidgetKind::Button {
                action: UiAction::Restart,
            },
        };
    }
    ui.horizontal(|ui| {
        ui.label("Text (uppercase ASCII)");
        ui.text_edit_singleline(&mut w.text);
    });
    for (label, v) in [("Position", &mut w.position), ("Size", &mut w.size)] {
        ui.horizontal(|ui| {
            ui.label(label);
            for n in v {
                ui.add(DragValue::new(n).speed(0.005).range(0. ..=1.));
            }
        });
    }
    ui.horizontal(|ui| {
        ui.label("Text/fill");
        ui.color_edit_button_rgb(&mut w.color);
        ui.label("Background");
        ui.color_edit_button_rgb(&mut w.background);
    });
    ComboBox::from_label("Visible when")
        .selected_text(format!("{:?}", w.visibility))
        .show_ui(ui, |ui| {
            for v in [
                Visibility::Always,
                Visibility::Ready,
                Visibility::Playing,
                Visibility::Paused,
                Visibility::Won,
                Visibility::Lost,
            ] {
                ui.selectable_value(&mut w.visibility, v, format!("{v:?}"));
            }
        });
    match &mut w.kind {
        WidgetKind::Progress { max_score } => {
            ui.add(
                DragValue::new(max_score)
                    .range(1..=1_000_000)
                    .prefix("Full at score "),
            );
        }
        WidgetKind::Button { action } => {
            ComboBox::from_label("Action")
                .selected_text(format!("{action:?}"))
                .show_ui(ui, |ui| {
                    for a in [
                        UiAction::Restart,
                        UiAction::Pause,
                        UiAction::Resume,
                        UiAction::Save,
                        UiAction::Load,
                        UiAction::NextScene,
                    ] {
                        ui.selectable_value(action, a, format!("{a:?}"));
                    }
                });
        }
        _ => {}
    }
    if ui.button("Delete element").clicked() {
        p.content.ui.remove(*selected);
    }
}
fn sounds(ui: &mut Ui, p: &mut Project, selected: &mut usize) {
    ui.label("Synthesized effects need no audio or model files. Playback uses the default output device.");
    if ui.button("+ Sound cue").clicked() {
        let id = unique("sound", p.content.sounds.iter().map(|s| s.id.clone()));
        p.content.sounds.push(SoundCue {
            id,
            trigger: CueTrigger::Score {},
            frequency: 660.,
            seconds: 0.15,
            volume: 0.2,
        });
        *selected = p.content.sounds.len() - 1;
    }
    if p.content.sounds.is_empty() {
        return;
    }
    *selected = (*selected).min(p.content.sounds.len() - 1);
    ComboBox::from_label("Cue")
        .selected_text(&p.content.sounds[*selected].id)
        .show_ui(ui, |ui| {
            for (i, s) in p.content.sounds.iter().enumerate() {
                ui.selectable_value(selected, i, &s.id);
            }
        });
    let s = &mut p.content.sounds[*selected];
    ComboBox::from_label("When")
        .selected_text(format!("{:?}", s.trigger))
        .show_ui(ui, |ui| {
            for (label, t) in [
                ("Game starts", CueTrigger::Start {}),
                ("Score changes", CueTrigger::Score {}),
                ("Win", CueTrigger::Won {}),
                ("Lose", CueTrigger::Lost {}),
            ] {
                ui.selectable_value(&mut s.trigger, t, label);
            }
            for r in &p.game.logic.rules {
                ui.selectable_value(
                    &mut s.trigger,
                    CueTrigger::Rule { id: r.id.clone() },
                    format!("Rule: {}", r.name),
                );
            }
        });
    ui.add(
        egui::Slider::new(&mut s.frequency, 20. ..=12000.)
            .logarithmic(true)
            .text("Hz"),
    );
    ui.add(egui::Slider::new(&mut s.seconds, 0.01..=5.).text("seconds"));
    ui.add(egui::Slider::new(&mut s.volume, 0. ..=1.).text("volume"));
    if ui.button("Delete sound").clicked() {
        p.content.sounds.remove(*selected);
    }
}
