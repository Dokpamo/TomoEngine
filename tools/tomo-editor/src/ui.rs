// SPDX-License-Identifier: MIT
use crate::Editor;
use egui::{Color32, Context, DragValue, Key, Pos2, Rect, RichText, Sense, Vec2};
use tomo_project::{Document, Project, ProjectFile};
use tomo_render::{Camera, CaptureOptions, FrameStyle};
use tomo_scene::{Motion, ShapeSpec};

const ACCENT: Color32 = Color32::from_rgb(80, 219, 174);
#[derive(Clone, Copy, PartialEq)]
enum FileAction {
    New,
    Open,
    SaveAs,
}
struct FileDialog {
    action: FileAction,
    path: String,
    name: String,
    discard: bool,
    close_after: bool,
}
struct Drag {
    id: String,
    origin: [f32; 3],
    hit: [f32; 3],
    pointer: Pos2,
}
pub struct EditorUi {
    pub viewport: Rect,
    pub colliders: bool,
    pub exit: bool,
    pub close_requested: bool,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: [f32; 3],
    snap: bool,
    file_dialog: Option<FileDialog>,
    drag: Option<Drag>,
    filter: String,
}
impl Default for EditorUi {
    fn default() -> Self {
        Self {
            viewport: Rect::from_min_size(Pos2::ZERO, Vec2::new(640., 640.)),
            colliders: false,
            exit: false,
            close_requested: false,
            yaw: 0.,
            pitch: 0.98,
            distance: 24.,
            target: [0.; 3],
            snap: true,
            file_dialog: None,
            drag: None,
            filter: String::new(),
        }
    }
}
pub fn theme(ctx: &Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(20, 27, 37);
    visuals.window_fill = Color32::from_rgb(27, 36, 48);
    visuals.extreme_bg_color = Color32::from_rgb(14, 20, 29);
    visuals.selection.bg_fill = Color32::from_rgb(30, 85, 74);
    visuals.selection.stroke = egui::Stroke::new(1., ACCENT);
    visuals.widgets.active.bg_fill = Color32::from_rgb(33, 112, 88);
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8., 9.);
        style.spacing.button_padding = Vec2::new(10., 6.);
    });
}
impl EditorUi {
    pub fn options(&self) -> CaptureOptions {
        let w = self.viewport.width().max(1.);
        let h = self.viewport.height().max(1.);
        let scale = (2048. / w.max(h)).min(1.);
        let horizontal = self.distance * self.pitch.cos();
        CaptureOptions {
            width: (w * scale).max(1.) as u32,
            height: (h * scale).max(1.) as u32,
            colliders: self.colliders,
            camera: Camera {
                eye: [
                    self.target[0] + horizontal * self.yaw.sin(),
                    self.target[1] + self.distance * self.pitch.sin(),
                    self.target[2] + horizontal * self.yaw.cos(),
                ],
                target: self.target,
                up: [0., 1., 0.],
                vertical_fov_degrees: 46.,
                near: 0.1,
                far: 1000.,
            },
            ..Default::default()
        }
    }
    pub fn style(&self, editor: &Editor) -> FrameStyle {
        let mut style = FrameStyle::default();
        for body in &editor.document.project().scene.bodies {
            let rules = &editor.document.project().game;
            let color = if editor.game.is_none() && editor.selected.as_deref() == Some(&body.id) {
                [0.2, 0.95, 0.9]
            } else if body.id == rules.player {
                [1., 0.38, 0.07]
            } else if body.id == rules.goal {
                [0.08, 0.95, 0.55]
            } else if body.id == "floor" {
                [0.105, 0.16, 0.22]
            } else if body.is_sensor {
                [0.4, 0.35, 0.7]
            } else {
                [0.25, 0.35, 0.46]
            };
            style.body_colors.insert(body.id.clone(), color);
        }
        style
    }
    pub fn blocked(&self) -> bool {
        self.file_dialog.is_some() || self.close_requested
    }
    fn file(&mut self, action: FileAction, editor: &Editor) {
        let path = editor
            .file
            .as_ref()
            .and_then(|f| f.path().parent())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .map(std::path::PathBuf::from)
                    .or_else(|| std::env::current_dir().ok())
                    .unwrap_or_default()
                    .join("MyGame")
                    .display()
                    .to_string()
            });
        let path = if action != FileAction::Open && editor.file.is_some() {
            format!("{path}-copy")
        } else {
            path
        };
        self.file_dialog = Some(FileDialog {
            action,
            path,
            name: editor.document.project().name.clone(),
            discard: false,
            close_after: false,
        });
    }
    fn save(&mut self, editor: &mut Editor) {
        if editor.file.is_none() {
            self.file(FileAction::SaveAs, editor);
        } else {
            let result = editor.save();
            editor.report(result, "Project saved.");
        }
    }
    fn focus(&mut self, editor: &Editor) {
        if let Some(body) = editor
            .document
            .project()
            .scene
            .bodies
            .iter()
            .find(|b| Some(&b.id) == editor.selected.as_ref())
        {
            self.target = body.position;
            let radius = match body.shape {
                ShapeSpec::Box { half_extents } => half_extents.into_iter().fold(0., f32::max),
                ShapeSpec::Sphere { radius } => radius,
            };
            self.distance = (radius * 6.).clamp(4., 80.);
        }
    }
    pub fn show(&mut self, ctx: &Context, editor: &mut Editor) {
        if ctx.input(|i| i.pointer.primary_pressed()) && editor.game.is_none() && !self.blocked() {
            editor.document.begin_gesture();
        }
        let editing = editor.game.is_none();
        if !self.blocked() {
            let save = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::S));
            if save {
                self.save(editor);
            }
            if editing && !ctx.wants_keyboard_input() {
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Z)) {
                    let r = editor.document.undo().map(|_| ());
                    editor.report(r, "Undo.");
                }
                if ctx.input_mut(|i| {
                    i.consume_key(
                        egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                        Key::Z,
                    )
                }) {
                    let r = editor.document.redo().map(|_| ());
                    editor.report(r, "Redo.");
                }
                if ctx.input(|i| i.key_pressed(Key::F)) {
                    self.focus(editor);
                }
            }
        }
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(4.);
            ui.horizontal(|ui| {
                ui.label(RichText::new("TOMO").strong().size(21.).color(ACCENT));
                ui.label(RichText::new("EDITOR").size(12.).weak());
                ui.separator();
                ui.add_enabled_ui(editing && !self.blocked(), |ui| {
                    if ui.button("New").clicked() {
                        self.file(FileAction::New, editor);
                    }
                    if ui.button("Open").clicked() {
                        self.file(FileAction::Open, editor);
                    }
                    if ui.button("Save").on_hover_text("Cmd/Ctrl + S").clicked() {
                        self.save(editor);
                    }
                    if ui.button("Save as").clicked() {
                        self.file(FileAction::SaveAs, editor);
                    }
                });
                ui.add_enabled_ui(editing && !self.blocked(), |ui| {
                    if ui
                        .add_enabled(editor.document.can_undo(), egui::Button::new("Undo"))
                        .clicked()
                    {
                        let r = editor.document.undo().map(|_| ());
                        editor.report(r, "Undo.");
                    }
                    if ui
                        .add_enabled(editor.document.can_redo(), egui::Button::new("Redo"))
                        .clicked()
                    {
                        let r = editor.document.redo().map(|_| ());
                        editor.report(r, "Redo.");
                    }
                });
                ui.separator();
                if editing {
                    if ui
                        .add_enabled(
                            !self.blocked(),
                            egui::Button::new(RichText::new("Play").color(ACCENT)),
                        )
                        .clicked()
                    {
                        let result = editor.start();
                        editor.report(
                            result,
                            "Play preview: WASD / arrows move. Stop restores the edit scene.",
                        );
                        ctx.memory_mut(|m| m.surrender_focus(egui::Id::NULL));
                    }
                } else {
                    if ui
                        .add_enabled(
                            !editor.failed,
                            egui::Button::new(if editor.paused { "Resume" } else { "Pause" }),
                        )
                        .clicked()
                    {
                        editor.paused = !editor.paused;
                    }
                    if ui.button("Stop").clicked() {
                        editor.stop();
                        editor.report(Ok(()), "Preview stopped. Edit scene restored.");
                    }
                    if let Some(game) = &editor.game {
                        ui.label(format!("{:?}   {:.2}s", game.status(), game.seconds()));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{}{}",
                            editor.document.project().name,
                            if editor.dirty() { " *" } else { "" }
                        ))
                        .strong(),
                    );
                });
            });
            ui.add_space(4.);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&editor.message)
                        .color(if editor.error {
                            Color32::LIGHT_RED
                        } else {
                            Color32::from_gray(180)
                        })
                        .size(12.),
                );
            });
        });
        egui::SidePanel::left("hierarchy")
            .default_width(210.)
            .width_range(170. ..=320.)
            .show(ctx, |ui| {
                ui.add_space(8.);
                ui.label(RichText::new("SCENE").strong().size(12.).color(ACCENT));
                ui.label(format!(
                    "{} bodies",
                    editor.document.project().scene.bodies.len()
                ));
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("Filter objects...")
                        .desired_width(f32::INFINITY),
                );
                ui.add_enabled_ui(editing && !self.blocked(), |ui| {
                    ui.horizontal(|ui| {
                        for (label, sphere) in [("+ Box", false), ("+ Sphere", true)] {
                            if ui.button(label).clicked() {
                                editor.document.end_gesture();
                                match editor.document.add(sphere) {
                                    Ok(id) => {
                                        editor.selected = Some(id);
                                        editor.report(Ok(()), "Body added.");
                                    }
                                    Err(e) => editor.report(Err(e), ""),
                                }
                            }
                        }
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("body_list")
                    .show(ui, |ui| {
                        for body in &editor.document.project().scene.bodies {
                            if !body.id.to_lowercase().contains(&self.filter.to_lowercase()) {
                                continue;
                            }
                            let tag = if body.id == editor.document.project().game.player {
                                "PLAYER"
                            } else if body.id == editor.document.project().game.goal {
                                "GOAL"
                            } else {
                                match body.shape {
                                    ShapeSpec::Box { .. } => "BOX",
                                    ShapeSpec::Sphere { .. } => "SPHERE",
                                }
                            };
                            let text = RichText::new(format!("{}   {}", body.id, tag)).size(13.);
                            if ui
                                .selectable_label(
                                    editor.selected.as_deref() == Some(&body.id),
                                    text,
                                )
                                .clicked()
                            {
                                editor.selected = Some(body.id.clone());
                            }
                        }
                    });
            });
        egui::SidePanel::right("inspector").default_width(285.).width_range(265. ..=400.).show(ctx,|ui| {
            egui::ScrollArea::vertical().show(ui,|ui| {
                ui.add_space(8.);ui.label(RichText::new("INSPECTOR").strong().size(12.).color(ACCENT));
                ui.add_enabled_ui(editing && !self.blocked(),|ui| {self.inspector(ui,editor);});
                ui.add_space(16.);ui.separator();
                ui.collapsing("Game settings",|ui| {
                    ui.label("Template: reach the goal");
                    ui.add_enabled_ui(editing && !self.blocked(),|ui| {
                        let mut candidate=editor.document.project().clone();
                        let mut changed=false;
                        ui.label("Project name");
                        changed |= ui.text_edit_singleline(&mut candidate.name).changed();
                        for (label,selected) in [("Player",&mut candidate.game.player),("Goal",&mut candidate.game.goal)] {
                            egui::ComboBox::from_label(label).selected_text(&*selected).show_ui(ui,|ui| {
                                for body in &candidate.scene.bodies {changed |= ui.selectable_value(selected,body.id.clone(),&body.id).changed();}
                            });
                        }
                        ui.horizontal(|ui| {ui.label("Speed (m/s)");changed |= ui.add(DragValue::new(&mut candidate.game.speed).speed(0.1).range(0.1..=30.)).changed();});
                        if changed {
                            candidate.scene.name = candidate.name.clone();
                            let expected = candidate.revision;
                            let r = editor.document.apply(candidate, expected).map(|_|());
                            editor.report(r, "Game settings updated.");
                        }
                    });
                    ui.small("The player is dynamic and solid. The goal is a static, unrotated sensor box large enough to contain the player.");
                });
                ui.add_space(16.);ui.small(format!("Project v1 / revision {}",editor.document.project().revision));
                if let Some(file)=&editor.file {ui.small(file.path().display().to_string());} else {ui.small("Unsaved project");}
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui| {
            ui.horizontal(|ui| {
                ui.add_space(8.);ui.label(RichText::new(if editing {"SCENE VIEW"} else {"PLAY PREVIEW"}).strong().size(12.).color(ACCENT));
                ui.separator();ui.checkbox(&mut self.colliders,"Colliders");ui.checkbox(&mut self.snap,"Snap 0.25m");
                if ui.button("Focus").clicked() {self.focus(editor);}
                if ui.button("Reset view").clicked() {self.yaw=0.;self.pitch=0.98;self.distance=24.;self.target=[0.;3];}
            });
            ui.add_space(4.);
            let (rect,response)=ui.allocate_exact_size(ui.available_size().max(Vec2::splat(1.)),Sense::click_and_drag());
            self.viewport=rect;
            if !self.blocked() { self.interact(ctx,editor,&response); }
            let painter=ui.painter_at(rect);
            let hint=if editing {"Drag object: move X/Z   |   Right drag: orbit   |   Middle drag: pan   |   Scroll: zoom"} else {"WASD / arrows: move   |   Pause to inspect   |   Stop to return to editing"};
            painter.text(rect.left_bottom()+Vec2::new(14.,-18.),egui::Align2::LEFT_BOTTOM,hint,egui::FontId::proportional(12.),Color32::from_gray(210));
            if let Some(game)=&editor.game {if game.status()==tomo_game::Status::Won {
                painter.text(rect.center(),egui::Align2::CENTER_CENTER,"DESTINATION REACHED",egui::FontId::proportional(30.),ACCENT);
            }}
        });
        if ctx.input(|i| i.pointer.primary_released()) {
            editor.document.end_gesture();
            self.drag = None;
        }
        self.dialogs(ctx, editor);
    }
    fn inspector(&mut self, ui: &mut egui::Ui, editor: &mut Editor) {
        let Some(id) = editor.selected.clone() else {
            ui.label("Select an object in the scene.");
            return;
        };
        let mut candidate = editor.document.project().clone();
        let Some(body) = candidate.scene.bodies.iter_mut().find(|b| b.id == id) else {
            editor.selected = None;
            return;
        };
        ui.heading(&id);
        ui.horizontal(|ui| {
            if ui.button("Duplicate").clicked() {
                editor.document.end_gesture();
                match editor.document.duplicate(&id) {
                    Ok(id) => {
                        editor.selected = Some(id);
                        editor.report(Ok(()), "Body duplicated.");
                    }
                    Err(e) => editor.report(Err(e), ""),
                }
            }
            if ui.button("Delete").clicked() {
                editor.document.end_gesture();
                let r = editor.document.remove(&id);
                if r.is_ok() {
                    editor.selected = None;
                }
                editor.report(r, "Body removed.");
            }
        });
        ui.separator();
        ui.strong("Transform");
        let mut changed = vector(ui, "Position", &mut body.position, 0.05, -10000. ..=10000.);
        let mut angles = to_euler(body.rotation);
        if vector(ui, "Rotation (degrees)", &mut angles, 1., -180. ..=180.) {
            body.rotation = from_euler(angles);
            changed = true;
        }
        ui.add_space(8.);
        ui.strong("Shape");
        let mut sphere = matches!(body.shape, ShapeSpec::Sphere { .. });
        ui.horizontal(|ui| {
            if ui.selectable_value(&mut sphere, false, "Box").changed() {
                body.shape = ShapeSpec::Box {
                    half_extents: [0.5; 3],
                };
                changed = true;
            }
            if ui.selectable_value(&mut sphere, true, "Sphere").changed() {
                body.shape = ShapeSpec::Sphere { radius: 0.5 };
                changed = true;
            }
        });
        match &mut body.shape {
            ShapeSpec::Box { half_extents } => {
                let mut size = half_extents.map(|v| v * 2.);
                if vector(ui, "Size (metres)", &mut size, 0.05, 0.02..=200.) {
                    *half_extents = size.map(|v| v / 2.);
                    changed = true;
                }
            }
            ShapeSpec::Sphere { radius } => {
                ui.horizontal(|ui| {
                    ui.label("Radius");
                    changed |= ui
                        .add(DragValue::new(radius).speed(0.05).range(0.01..=100.))
                        .changed();
                });
            }
        }
        ui.add_space(8.);
        ui.strong("Physics");
        egui::ComboBox::from_label("Motion")
            .selected_text(format!("{:?}", body.motion))
            .show_ui(ui, |ui| {
                for value in [Motion::Static, Motion::Dynamic, Motion::Kinematic] {
                    changed |= ui
                        .selectable_value(&mut body.motion, value, format!("{value:?}"))
                        .changed();
                }
            });
        ui.horizontal(|ui| {
            ui.label("Mass (kg)");
            changed |= ui
                .add(
                    DragValue::new(&mut body.mass)
                        .range(0.01..=10000.)
                        .speed(0.1),
                )
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label("Friction");
            changed |= ui
                .add(
                    DragValue::new(&mut body.friction)
                        .range(0. ..=1.)
                        .speed(0.01),
                )
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label("Restitution");
            changed |= ui
                .add(
                    DragValue::new(&mut body.restitution)
                        .range(0. ..=1.)
                        .speed(0.01),
                )
                .changed();
        });
        changed |= ui
            .checkbox(&mut body.is_sensor, "Sensor (no collision response)")
            .changed();
        changed |= ui.checkbox(&mut body.allow_sleep, "Allow sleep").changed();
        ui.collapsing("Initial velocity", |ui| {
            changed |= vector(
                ui,
                "Linear (m/s)",
                &mut body.linear_velocity,
                0.1,
                -100. ..=100.,
            );
            changed |= vector(
                ui,
                "Angular (rad/s)",
                &mut body.angular_velocity,
                0.1,
                -100. ..=100.,
            );
        });
        if changed {
            let expected = candidate.revision;
            let r = editor.document.apply(candidate, expected).map(|_| ());
            editor.report(r, "Properties updated.");
        }
    }
    fn interact(&mut self, ctx: &Context, editor: &mut Editor, response: &egui::Response) {
        if response.dragged_by(egui::PointerButton::Secondary) {
            let d = response.drag_delta();
            self.yaw -= d.x * 0.008;
            self.pitch = (self.pitch + d.y * 0.008).clamp(0.12, 1.5);
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            let d = response.drag_delta() * self.distance * 0.0015;
            self.target[0] -= d.x * self.yaw.cos() + d.y * self.yaw.sin();
            self.target[2] += d.x * self.yaw.sin() - d.y * self.yaw.cos();
        }
        if response.hovered() {
            self.distance = (self.distance
                * (-ctx.input(|i| i.smooth_scroll_delta.y) * 0.002).exp())
            .clamp(2., 300.);
        }
        if editor.game.is_some() {
            return;
        }
        let options = self.options();
        let rect = self.viewport;
        let pixel = |p: Pos2| {
            [
                (p.x - rect.min.x) / rect.width() * options.width as f32,
                (p.y - rect.min.y) / rect.height() * options.height as f32,
            ]
        };
        // Process ordered pointer events, including press/move/release coalesced into one
        // frame. A Response alone can lose the starting position of a very fast drag.
        let events = ctx.input(|i| i.raw.events.clone());
        for event in events {
            let released = matches!(
                event,
                egui::Event::PointerButton {
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                }
            );
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } if rect.contains(pos) => {
                    match tomo_render::pick_body(
                        &editor.document.project().scene,
                        editor.document.snapshot(),
                        &options,
                        pixel(pos),
                    ) {
                        Ok(selected) => {
                            editor.selected = selected;
                            self.drag = None;
                            if let Some(body) = editor
                                .document
                                .project()
                                .scene
                                .bodies
                                .iter()
                                .find(|b| Some(&b.id) == editor.selected.as_ref())
                            {
                                if let Ok(Some(hit)) = tomo_render::plane_position(
                                    &options,
                                    pixel(pos),
                                    body.position[1],
                                ) {
                                    self.drag = Some(Drag {
                                        id: body.id.clone(),
                                        origin: body.position,
                                        hit,
                                        pointer: pos,
                                    });
                                }
                            }
                        }
                        Err(e) => editor.report(Err(e), ""),
                    }
                }
                egui::Event::PointerMoved(pos)
                | egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } => {
                    if let Some(drag) = &self.drag {
                        if pos.distance(drag.pointer) > 4.0 {
                            if let Ok(Some(hit)) =
                                tomo_render::plane_position(&options, pixel(pos), drag.origin[1])
                            {
                                let mut candidate = editor.document.project().clone();
                                if let Some(body) =
                                    candidate.scene.bodies.iter_mut().find(|b| b.id == drag.id)
                                {
                                    for i in [0, 2] {
                                        let v = drag.origin[i] + hit[i] - drag.hit[i];
                                        body.position[i] =
                                            if self.snap { (v * 4.).round() / 4. } else { v };
                                    }
                                }
                                let expected = candidate.revision;
                                let r = editor.document.apply(candidate, expected).map(|_| ());
                                editor.report(r, "Object moved. Undo restores the whole drag.");
                            }
                        }
                    }
                }
                _ => {}
            }
            if released {
                self.drag = None;
            }
        }
    }
    pub fn cancel_drag(&mut self) {
        self.drag = None;
    }
    fn dialogs(&mut self, ctx: &Context, editor: &mut Editor) {
        if self.close_requested {
            if !editor.dirty() {
                self.exit = true;
                return;
            }
            egui::Window::new("Unsaved project")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Save your changes before closing?");
                    ui.horizontal(|ui| {
                        if ui.button("Save and close").clicked() {
                            if editor.file.is_some() {
                                let r = editor.save();
                                if r.is_ok() {
                                    self.exit = true;
                                }
                                editor.report(r, "Saved.");
                            } else {
                                self.file(FileAction::SaveAs, editor);
                                if let Some(dialog) = &mut self.file_dialog {
                                    dialog.close_after = true;
                                }
                            }
                            self.close_requested = false;
                        }
                        if ui.button("Discard and close").clicked() {
                            self.exit = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.close_requested = false;
                        }
                    });
                });
        }
        let Some(mut dialog) = self.file_dialog.take() else {
            return;
        };
        let mut done = false;
        let title = match dialog.action {
            FileAction::New => "New game project",
            FileAction::Open => "Open project",
            FileAction::SaveAs => "Save project to a new folder",
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .default_width(500.)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(if dialog.action == FileAction::Open {
                    "Project folder or project.tomo.json"
                } else {
                    "New folder path (parent folder must exist)"
                });
                ui.add(egui::TextEdit::singleline(&mut dialog.path).desired_width(480.));
                if dialog.action == FileAction::New {
                    ui.label("Game name");
                    ui.text_edit_singleline(&mut dialog.name);
                    ui.small("Starts with the editable Box Run template.");
                }
                let needs_discard = editor.dirty() && dialog.action != FileAction::SaveAs;
                if needs_discard {
                    ui.checkbox(
                        &mut dialog.discard,
                        "Discard current unsaved changes after opening succeeds",
                    );
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !needs_discard || dialog.discard,
                            egui::Button::new("Continue"),
                        )
                        .clicked()
                    {
                        let result = (|| {
                            let file = match dialog.action {
                                FileAction::Open => ProjectFile::open(&dialog.path)?,
                                FileAction::New => ProjectFile::create(
                                    &dialog.path,
                                    &Project::template(&dialog.name)?,
                                )?,
                                FileAction::SaveAs => {
                                    ProjectFile::create(&dialog.path, editor.document.project())?
                                }
                            };
                            if dialog.action != FileAction::SaveAs {
                                let document = Document::new(file.project().clone())?;
                                editor.stop();
                                editor.document = document;
                                editor.selected = Some(file.project().game.player.clone());
                                self.yaw = 0.;
                                self.pitch = 0.98;
                                self.distance = 24.;
                                self.target = [0.; 3];
                            }
                            editor.file = Some(file);
                            Ok(())
                        })();
                        if result.is_ok() {
                            done = true;
                            if dialog.close_after {
                                self.exit = true;
                            }
                        }
                        editor.report(result, "Project ready.");
                    }
                    if ui.button("Cancel").clicked() {
                        done = true;
                    }
                });
                if editor.error {
                    ui.colored_label(Color32::LIGHT_RED, &editor.message);
                }
            });
        if !done {
            self.file_dialog = Some(dialog);
        }
    }
}
fn vector(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut [f32; 3],
    speed: f64,
    range: std::ops::RangeInclusive<f32>,
) -> bool {
    let mut changed = false;
    ui.label(label);
    ui.horizontal(|ui| {
        for (axis, value) in ["X", "Y", "Z"].into_iter().zip(v) {
            ui.label(axis);
            changed |= ui
                .add(
                    DragValue::new(value)
                        .speed(speed)
                        .range(range.clone())
                        .max_decimals(2),
                )
                .changed();
        }
    });
    changed
}
fn to_euler([x, y, z, w]: [f32; 4]) -> [f32; 3] {
    [
        (2. * (w * x + y * z)).atan2(1. - 2. * (x * x + y * y)),
        (2. * (w * y - z * x)).clamp(-1., 1.).asin(),
        (2. * (w * z + x * y)).atan2(1. - 2. * (y * y + z * z)),
    ]
    .map(f32::to_degrees)
}
fn from_euler(angles: [f32; 3]) -> [f32; 4] {
    let [x, y, z] = angles.map(|a| a.to_radians() / 2.);
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
    ]
}
