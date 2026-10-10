// SPDX-License-Identifier: MIT
//! Rule authoring drafts commit through the same revision-checked document transaction.
use crate::Editor;
use egui::{Color32, DragValue, RichText, Ui};
use tomo_game::{
    Action, Condition, ContactKind, Key, MAX_ACTIONS, MAX_CONDITIONS, MAX_RULES, Rule, StartMode,
    Trigger,
};
use tomo_project::Project;
use tomo_scene::{Motion, Scene};

#[derive(Default)]
pub(crate) struct LogicUi {
    pub open: bool,
    draft: Option<Project>,
    selected: usize,
    error: String,
}
impl LogicUi {
    pub fn open(&mut self, editor: &Editor) {
        self.open = true;
        self.draft = Some(editor.game.as_ref().map_or_else(
            || editor.document.project().clone(),
            |game| game.project().clone(),
        ));
        self.selected = 0;
        self.error.clear();
    }
    pub fn show(&mut self, ctx: &egui::Context, editor: &mut Editor) {
        if !self.open {
            return;
        }
        let Some(draft) = self.draft.as_mut() else {
            return;
        };
        let playing = editor.game.is_some();
        let mut apply = false;
        let mut reload = false;
        let mut close = false;
        let mut discard = false;
        egui::Window::new("Game Logic")
            .id(egui::Id::new("game_logic_editor"))
            .collapsible(false).default_pos(egui::pos2(215., 90.))
            .default_width(760.).resizable(true)
            .show(ctx, |ui| {
                ui.label(RichText::new("WHEN an event happens  +  IF all conditions match  ->  DO actions").strong());
                ui.small("Rules are evaluated top to bottom within each phase. Apply rules, then Done and Play to test.");
                if playing {
                    ui.colored_label(Color32::LIGHT_BLUE, "Preview paused while this window is open. Stop to edit rules.");
                }
                ui.add_enabled_ui(!playing, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Start clock");
                        egui::ComboBox::from_id_salt("logic_start").selected_text(match draft.game.logic.start {
                            StartMode::FirstInput => "On first movement / Space", StartMode::Immediate => "Immediately on Play",
                        }).show_ui(ui, |ui| {
                            ui.selectable_value(&mut draft.game.logic.start, StartMode::FirstInput, "On first movement / Space");
                            ui.selectable_value(&mut draft.game.logic.start, StartMode::Immediate, "Immediately on Play");
                        });
                        ui.label(format!("{} / {MAX_RULES} rules", draft.game.logic.rules.len()));
                    });
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(draft.game.logic.rules.len() < MAX_RULES, |ui| {
                            for (label, preset) in [("+ Rule", 0), ("+ Space jump", 1), ("+ Contact score", 2), ("+ Time limit", 3)] {
                                if ui.button(label).clicked() {
                                    let id = (1..).map(|i| format!("rule_{i}"))
                                        .find(|id| !draft.game.logic.rules.iter().any(|r| r.id == *id)).expect("bounded rules");
                                    let (name, trigger, action, once) = match preset {
                                        1 => ("Space jump", Trigger::KeyPressed { key: Key::Action }, Action::SetVelocity { body: draft.game.player.clone(), linear: [0., 6., 0.] }, false),
                                        2 => ("Contact score", Trigger::Contact { body_a: draft.game.player.clone(), body_b: draft.game.goal.clone(), kind: ContactKind::Started }, Action::AddScore { amount: 1 }, true),
                                        3 => ("Time limit", Trigger::Timer { ticks: 600, repeat: false }, Action::Lose {}, true),
                                        _ => ("New rule", Trigger::Start {}, Action::AddScore { amount: 1 }, true),
                                    };
                                    draft.game.logic.rules.push(Rule { id, name: name.into(), enabled: true, once, trigger, conditions: vec![], actions: vec![action] });
                                    self.selected = draft.game.logic.rules.len() - 1;
                                }
                            }
                        });
                    });
                });
                ui.separator();
                let height = (ctx.screen_rect().height() - 310.).clamp(180., 470.);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(200., height), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(200.);
                        egui::ScrollArea::vertical().id_salt("logic_rule_list").max_height(height).auto_shrink([false, false]).show(ui, |ui| {
                            for (i, rule) in draft.game.logic.rules.iter().enumerate() {
                                let title = format!("{}  {}{}", i + 1, rule.name, if rule.enabled { "" } else { " (off)" });
                                if ui.selectable_label(i == self.selected, title).clicked() { self.selected = i; }
                                if let Some(game) = &editor.game { ui.small(format!("Fired {} times", game.rule_fires(&rule.id))); }
                            }
                            if draft.game.logic.rules.is_empty() { ui.label("No rules. Add one above."); }
                        });
                    });
                    ui.add_space(10.);
                    ui.vertical(|ui| {
                        ui.set_width(490.);
                        let selected = self.selected;
                        let count = draft.game.logic.rules.len();
                        let mut remove = false;
                        let mut movement = 0;
                        ui.add_enabled_ui(!playing, |ui| {
                            if let Some(rule) = draft.game.logic.rules.get_mut(selected) {
                                ui.horizontal(|ui| {
                                    ui.small(format!("ID: {}", rule.id));
                                    if ui.add_enabled(selected > 0, egui::Button::new("Move up")).clicked() { movement = -1; }
                                    if ui.add_enabled(selected + 1 < count, egui::Button::new("Move down")).clicked() { movement = 1; }
                                    remove = ui.button("Remove rule").clicked();
                                });
                                egui::ScrollArea::vertical().id_salt(("logic_rule_fields", &rule.id)).max_height(height - 35.).show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("Name"); ui.text_edit_singleline(&mut rule.name);
                                    });
                                    ui.horizontal(|ui| { ui.checkbox(&mut rule.enabled, "Enabled"); ui.checkbox(&mut rule.once, "Once per run"); });
                                    ui.separator(); ui.strong("WHEN");
                                    trigger(ui, &mut rule.trigger, &draft.scene, &draft.game.player, &draft.game.goal);
                                    ui.separator(); ui.strong("IF (all must match)");
                                    let mut remove_condition = None;
                                    for (i, condition) in rule.conditions.iter_mut().enumerate() {
                                        ui.push_id(("condition", i), |ui| {
                                            ui.horizontal(|ui| { ui.label(format!("Condition {}", i + 1)); if ui.small_button("Remove").clicked() { remove_condition = Some(i); } });
                                            edit_condition(ui, condition, &draft.scene, &draft.game.player);
                                        });
                                    }
                                    if let Some(i) = remove_condition { rule.conditions.remove(i); }
                                    if ui.add_enabled(rule.conditions.len() < MAX_CONDITIONS, egui::Button::new("+ Condition")).clicked() {
                                        rule.conditions.push(Condition::ScoreAtLeast { value: 1 });
                                    }
                                    ui.separator(); ui.strong("DO (in order)");
                                    let mut remove_action = None;
                                    let mut move_action = None;
                                    for (i, action) in rule.actions.iter_mut().enumerate() {
                                        ui.push_id(("action", i), |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(format!("Action {}", i + 1));
                                                if ui.add_enabled(i > 0, egui::Button::new("Earlier")).clicked() { move_action = Some(i); }
                                                if ui.small_button("Remove").clicked() { remove_action = Some(i); }
                                            });
                                            edit_action(ui, action, &draft.scene, &draft.game.player);
                                        });
                                    }
                                    if let Some(i) = remove_action { rule.actions.remove(i); }
                                    else if let Some(i) = move_action { rule.actions.swap(i, i - 1); }
                                    if ui.add_enabled(rule.actions.len() < MAX_ACTIONS, egui::Button::new("+ Action")).clicked() {
                                        let i = if matches!(rule.actions.last(), Some(Action::Win {} | Action::Lose {})) { rule.actions.len() - 1 } else { rule.actions.len() };
                                        rule.actions.insert(i, Action::AddScore { amount: 1 });
                                    }
                                });
                            }
                        });
                        if remove {
                            draft.game.logic.rules.remove(selected);
                            self.selected = selected.saturating_sub(1);
                        } else if movement != 0 {
                            let to = (selected as isize + movement) as usize;
                            draft.game.logic.rules.swap(selected, to); self.selected = to;
                        }
                    });
                });
                ui.separator();
                if let Some(game) = &editor.game {
                    ui.label(format!("Preview: {:?}  |  score {}  |  tick {}", game.status(), game.score(), game.elapsed_ticks()));
                    if let Some(event) = game.recent_events().last() { ui.small(format!("Last rule: {} at tick {}", event.rule_id, event.tick)); }
                }
                let dirty = draft != editor.document.project();
                if dirty { ui.colored_label(Color32::YELLOW, "Draft changes — Apply rules to include them in Save and Play."); }
                if !self.error.is_empty() { ui.colored_label(Color32::LIGHT_RED, &self.error); }
                ui.horizontal(|ui| {
                    apply = ui.add_enabled(!playing && dirty, egui::Button::new("Apply rules")).clicked();
                    reload = ui.add_enabled(!playing && dirty, egui::Button::new("Reload current")).on_hover_text("Discard this draft and read the current document").clicked();
                    close = ui.add_enabled(!dirty, egui::Button::new("Done")).clicked();
                    if dirty { discard = ui.button("Discard & close").clicked(); }
                });
            });
        if apply {
            editor.document.end_gesture();
            let candidate = draft.clone();
            match editor.document.apply(candidate, draft.revision) {
                Ok(_) => {
                    *draft = editor.document.project().clone();
                    self.error.clear();
                    editor.report(
                        Ok(()),
                        "Game rules applied. Play to test, Save to keep them.",
                    );
                }
                Err(e) => {
                    self.error = format!("{}: {} ({})", e.code, e.message, e.path);
                    editor.report(Err(e), "");
                }
            }
        }
        if reload {
            *draft = editor.document.project().clone();
            self.selected = 0;
            self.error.clear();
        }
        if close || discard {
            self.open = false;
            self.draft = None;
            self.error.clear();
        }
    }
}

fn key(ui: &mut Ui, key: &mut Key) {
    egui::ComboBox::from_id_salt("key")
        .selected_text(key_name(*key))
        .show_ui(ui, |ui| {
            for k in [Key::Up, Key::Down, Key::Left, Key::Right, Key::Action] {
                ui.selectable_value(key, k, key_name(k));
            }
        });
}
fn key_name(key: Key) -> &'static str {
    match key {
        Key::Up => "W / Up",
        Key::Down => "S / Down",
        Key::Left => "A / Left",
        Key::Right => "D / Right",
        Key::Action => "Space",
    }
}
fn body(ui: &mut Ui, label: &str, id: &mut String, scene: &Scene, movable: bool) {
    egui::ComboBox::from_label(label)
        .selected_text(&*id)
        .show_ui(ui, |ui| {
            for b in &scene.bodies {
                if !movable || b.motion != Motion::Static {
                    ui.selectable_value(id, b.id.clone(), &b.id);
                }
            }
        });
}
fn trigger(ui: &mut Ui, trigger: &mut Trigger, scene: &Scene, player: &str, goal: &str) {
    let current = match trigger {
        Trigger::Start {} => 0,
        Trigger::EveryTick {} => 1,
        Trigger::KeyPressed { .. } => 2,
        Trigger::Timer { .. } => 3,
        Trigger::Contact { .. } => 4,
        Trigger::GoalReached {} => 5,
    };
    let mut selected = current;
    let labels = [
        "Game starts",
        "Every tick",
        "Key pressed",
        "Timer",
        "Contact",
        "Goal reached",
    ];
    egui::ComboBox::from_id_salt("trigger_type")
        .selected_text(labels[selected])
        .show_ui(ui, |ui| {
            for (i, label) in labels.iter().enumerate() {
                ui.selectable_value(&mut selected, i, *label);
            }
        });
    if current != selected {
        *trigger = match selected {
            0 => Trigger::Start {},
            1 => Trigger::EveryTick {},
            2 => Trigger::KeyPressed { key: Key::Action },
            3 => Trigger::Timer {
                ticks: 600,
                repeat: false,
            },
            4 => Trigger::Contact {
                body_a: player.into(),
                body_b: goal.into(),
                kind: ContactKind::Started,
            },
            _ => Trigger::GoalReached {},
        };
    }
    match trigger {
        Trigger::KeyPressed { key: k } => {
            key(ui, k);
            ui.small("One event on the released -> held edge; holding does not repeat.");
        }
        Trigger::Timer { ticks, repeat } => {
            ui.horizontal(|ui| {
                ui.label("Ticks");
                ui.add(DragValue::new(ticks).range(1..=36_000));
                ui.label(format!(
                    "{:.2} s",
                    f64::from(*ticks) * f64::from(scene.fixed_dt)
                ));
                ui.checkbox(repeat, "Repeat");
            });
        }
        Trigger::Contact {
            body_a,
            body_b,
            kind,
        } => {
            body(ui, "Body A", body_a, scene, false);
            body(ui, "Body B", body_b, scene, false);
            egui::ComboBox::from_label("Contact phase")
                .selected_text(format!("{kind:?}"))
                .show_ui(ui, |ui| {
                    for k in [
                        ContactKind::Started,
                        ContactKind::Persisted,
                        ContactKind::Stopped,
                    ] {
                        ui.selectable_value(kind, k, format!("{k:?}"));
                    }
                });
            ui.small("Actual physics contacts, including sensors. At least one body must be dynamic. Evaluated after physics, at most once per tick.");
        }
        Trigger::GoalReached {} => {
            ui.small("After physics: the configured player is fully inside the goal's horizontal bounds.");
        }
        _ => {
            ui.small("Evaluated before physics.");
        }
    }
}
fn edit_condition(ui: &mut Ui, condition: &mut Condition, scene: &Scene, player: &str) {
    let current = match condition {
        Condition::KeyHeld { .. } => 0,
        Condition::ScoreAtLeast { .. } => 1,
        Condition::HeightBelow { .. } => 2,
    };
    let mut selected = current;
    let labels = ["Key held", "Score at least", "Height below"];
    egui::ComboBox::from_id_salt("condition_type")
        .selected_text(labels[selected])
        .show_ui(ui, |ui| {
            for (i, label) in labels.iter().enumerate() {
                ui.selectable_value(&mut selected, i, *label);
            }
        });
    if current != selected {
        *condition = match selected {
            0 => Condition::KeyHeld { key: Key::Action },
            1 => Condition::ScoreAtLeast { value: 1 },
            _ => Condition::HeightBelow {
                body: player.into(),
                y: 0.,
            },
        };
    }
    match condition {
        Condition::KeyHeld { key: k } => key(ui, k),
        Condition::ScoreAtLeast { value } => {
            ui.add(DragValue::new(value).range(-1_000_000..=1_000_000));
        }
        Condition::HeightBelow { body: id, y } => {
            body(ui, "Body", id, scene, false);
            ui.horizontal(|ui| {
                ui.label("Y below (m)");
                ui.add(DragValue::new(y).speed(0.1).range(-10000. ..=10000.));
            });
        }
    }
}
fn edit_action(ui: &mut Ui, action: &mut Action, scene: &Scene, player: &str) {
    let current = match action {
        Action::MovePlayer {} => 0,
        Action::SetVelocity { .. } => 1,
        Action::AddScore { .. } => 2,
        Action::Win {} => 3,
        Action::Lose {} => 4,
    };
    let mut selected = current;
    let labels = [
        "Move player with input",
        "Set velocity",
        "Add score",
        "Win game",
        "Lose game",
    ];
    egui::ComboBox::from_id_salt("action_type")
        .selected_text(labels[selected])
        .show_ui(ui, |ui| {
            for (i, label) in labels.iter().enumerate() {
                ui.selectable_value(&mut selected, i, *label);
            }
        });
    if current != selected {
        *action = match selected {
            0 => Action::MovePlayer {},
            1 => Action::SetVelocity {
                body: player.into(),
                linear: [0., 6., 0.],
            },
            2 => Action::AddScore { amount: 1 },
            3 => Action::Win {},
            _ => Action::Lose {},
        };
    }
    match action {
        Action::MovePlayer {} => {
            ui.small("WASD / arrows control the player at Game settings speed. Keeps vertical velocity and returns the player upright.");
        }
        Action::SetVelocity { body: id, linear } => {
            body(ui, "Body", id, scene, true);
            ui.horizontal(|ui| {
                for (label, v) in ["X", "Y", "Z"].into_iter().zip(linear) {
                    ui.label(label);
                    ui.add(DragValue::new(v).speed(0.1).range(-100. ..=100.));
                }
                ui.label("m/s");
            });
            ui.small("Replaces world-space linear velocity. Angular velocity is preserved. Space jump is repeatable in mid-air; no grounded check yet.");
        }
        Action::AddScore { amount } => {
            ui.horizontal(|ui| {
                ui.label("Amount");
                ui.add(DragValue::new(amount).range(-1_000_000..=1_000_000));
            });
        }
        Action::Win {} | Action::Lose {} => {
            ui.small("Last action only. Ends this round and skips later rules; the current physics tick completes.");
        }
    }
}
