// SPDX-License-Identifier: MIT
#![forbid(unsafe_code)]
use serde_json::json;
use std::{
    process::ExitCode,
    sync::Arc,
    time::{Duration, Instant},
};
use tomo_editor::{
    Editor,
    ui::{EditorUi, theme},
};
use tomo_game::{FrameClock, Input};
use tomo_project::ProjectFile;
use tomo_render::{EditorFrame, WindowRenderer};
use tomo_scene::Problem;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

struct App {
    audio: Option<tomo_render::audio::AudioPlayer>,
    audio_tried: bool,
    editor: Editor,
    ui: EditorUi,
    ctx: egui::Context,
    state: Option<egui_winit::State>,
    window: Option<Arc<Window>>,
    renderer: Option<WindowRenderer>,
    clock: FrameClock,
    last_frame: Instant,
    next_frame: Instant,
    started: Instant,
    focused: bool,
    occluded: bool,
    minimized: bool,
    was_playing: bool,
    error: Option<Problem>,
    smoke: bool,
    presented: u32,
    resized: bool,
    smoke_moved: bool,
}
impl App {
    fn fail(&mut self, events: &ActiveEventLoop, e: Problem) {
        self.error = Some(e);
        events.exit();
    }
    fn reset_time(&mut self) {
        self.clock.reset();
        self.last_frame = Instant::now();
    }
    fn draw(&mut self, events: &ActiveEventLoop) -> Result<(), Problem> {
        let Some(window) = self.window.clone() else {
            return Ok(());
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            self.reset_time();
            return Ok(());
        }
        let input = self
            .state
            .as_mut()
            .expect("window input")
            .take_egui_input(&window);
        let ctx = self.ctx.clone();
        let output = ctx.run(input, |ctx| self.ui.show(ctx, &mut self.editor));
        self.state
            .as_mut()
            .expect("window input")
            .handle_platform_output(&window, output.platform_output);
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        let playing = self.editor.game.is_some()
            && !self.editor.paused
            && !self.editor.failed
            && !self.ui.blocked()
            && !self.minimized
            && (self.smoke || (self.focused && !self.occluded));
        if playing != self.was_playing {
            self.clock.reset();
            self.was_playing = playing;
        }
        if playing {
            let dt = f64::from(self.editor.document.project().scene.fixed_dt);
            let steps = if self.smoke {
                1
            } else {
                self.clock.advance(elapsed, dt)
            };
            let input = if self.smoke {
                Input {
                    right: true,
                    ..Default::default()
                }
            } else if ctx.wants_keyboard_input() {
                Input::default()
            } else {
                ctx.input(|i| Input {
                    up: i.key_down(egui::Key::W) || i.key_down(egui::Key::ArrowUp),
                    down: i.key_down(egui::Key::S) || i.key_down(egui::Key::ArrowDown),
                    left: i.key_down(egui::Key::A) || i.key_down(egui::Key::ArrowLeft),
                    right: i.key_down(egui::Key::D) || i.key_down(egui::Key::ArrowRight),
                    action: i.key_down(egui::Key::Space),
                })
            };
            for _ in 0..steps {
                if let Err(e) = self.editor.step(input) {
                    self.editor.report(Err(e), "");
                    break;
                }
                let sounds = self
                    .editor
                    .game
                    .as_mut()
                    .map(|g| g.take_sounds())
                    .unwrap_or_default();
                if !sounds.is_empty() {
                    if !self.audio_tried {
                        self.audio_tried = true;
                        match tomo_render::audio::AudioPlayer::new() {
                            Ok(a) => self.audio = Some(a),
                            Err(e) => self.editor.report(Err(e), ""),
                        }
                    }
                    if let Some(a) = &self.audio {
                        for cue in sounds {
                            if let Err(e) = a.play(&cue) {
                                self.editor.report(Err(e), "");
                            }
                        }
                    }
                }
            }
        } else {
            if let Some(a) = &self.audio {
                a.clear();
            }
            self.clock.reset();
        }
        let ppp = output.pixels_per_point;
        let rect = self.ui.viewport;
        let x = (rect.min.x * ppp)
            .round()
            .clamp(0., size.width.saturating_sub(1) as f32) as u32;
        let y = (rect.min.y * ppp)
            .round()
            .clamp(0., size.height.saturating_sub(1) as f32) as u32;
        let right = (rect.max.x * ppp)
            .round()
            .clamp((x + 1) as f32, size.width as f32) as u32;
        let bottom = (rect.max.y * ppp)
            .round()
            .clamp((y + 1) as f32, size.height as f32) as u32;
        let primitives = ctx.tessellate(output.shapes, ppp);
        let frame = EditorFrame {
            viewport: [x, y, right - x, bottom - y],
            primitives: &primitives,
            textures: &output.textures_delta,
            pixels_per_point: ppp,
        };
        let mut options = self.ui.options();
        let mut style = self.ui.style(&self.editor);
        if let Some(game) = &self.editor.game {
            options.camera =
                tomo_render::content_camera(game.content(), game.snapshot(), game.seconds())?;
            tomo_render::content_visuals(
                game.content(),
                game.snapshot(),
                game.seconds(),
                &mut style,
            )?;
            tomo_render::content_hud(
                game.content(),
                tomo_editor::hud_state(game, self.editor.paused),
                &options,
                &mut style,
            );
        } else {
            tomo_render::content_visuals(
                &self.editor.document.project().content,
                self.editor.document.snapshot(),
                0.,
                &mut style,
            )?;
            tomo_render::content_hud(
                &self.editor.document.project().content,
                tomo_render::HudState {
                    score: 0,
                    seconds: 0.,
                    paused: false,
                    status: tomo_scene::Visibility::Ready,
                },
                &options,
                &mut style,
            );
        }
        let (scene, snapshot) = match &self.editor.game {
            Some(game) => (game.scene(), game.snapshot()),
            None => (
                &self.editor.document.project().scene,
                self.editor.document.snapshot(),
            ),
        };
        window.pre_present_notify();
        if self
            .renderer
            .as_mut()
            .expect("window renderer")
            .present_editor(scene, snapshot, &options, &style, &frame)?
        {
            self.presented = self.presented.saturating_add(1);
            if self.smoke {
                self.smoke_step(&window, events)?;
            }
        }
        if self.ui.exit {
            events.exit();
        }
        Ok(())
    }
    fn smoke_step(&mut self, window: &Window, events: &ActiveEventLoop) -> Result<(), Problem> {
        match self.presented {
            10 => {
                self.editor.document.add(false)?;
                let mut candidate = self.editor.document.project().clone();
                candidate.game.logic.rules.push(tomo_game::Rule {
                    id: "smoke_score".into(),
                    name: "Smoke start score".into(),
                    enabled: true,
                    once: true,
                    trigger: tomo_game::Trigger::Start {},
                    conditions: vec![],
                    actions: vec![tomo_game::Action::AddScore { amount: 1 }],
                });
                let revision = candidate.revision;
                self.editor.document.apply(candidate, revision)?;
            }
            20 => {
                self.editor.start()?;
            }
            60 => {
                self.smoke_moved = self
                    .editor
                    .game
                    .as_ref()
                    .is_some_and(|g| g.player().position[0] > -3. && g.score() == 1);
                self.editor.stop();
                let _ = window.request_inner_size(LogicalSize::new(1150., 740.));
            }
            65 => self.ui.open_logic(&self.editor),
            120 => {
                let initial = self
                    .editor
                    .document
                    .snapshot()
                    .bodies
                    .iter()
                    .find(|b| b.id == "player")
                    .expect("template player");
                if !self.smoke_moved
                    || !self.resized
                    || initial.position[0] != -5.
                    || self.editor.document.project().scene.bodies.len() != 10
                {
                    return Err(Problem::new(
                        "EDITOR_TEST",
                        "/smoke",
                        "edit, preview isolation or resize failed",
                    ));
                }
                events.exit();
            }
            _ => {}
        }
        Ok(())
    }
}
impl ApplicationHandler for App {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.window.is_some() {
            self.reset_time();
            return;
        }
        let result = (|| {
            let window = Arc::new(
                events
                    .create_window(
                        Window::default_attributes()
                            .with_title("Tomo Editor")
                            .with_inner_size(LogicalSize::new(1280., 820.))
                            .with_min_inner_size(LogicalSize::new(1000., 640.)),
                    )
                    .map_err(|e| Problem::new("WINDOW", "/window", e.to_string()))?,
            );
            let size = window.inner_size();
            let renderer = WindowRenderer::new(window.clone(), size.width, size.height)?;
            // Recreate context with the device so its atlas is fully uploaded after resume.
            self.ctx = egui::Context::default();
            theme(&self.ctx);
            self.state = Some(egui_winit::State::new(
                self.ctx.clone(),
                egui::ViewportId::ROOT,
                window.as_ref(),
                Some(window.scale_factor() as f32),
                window.theme(),
                Some(4096),
            ));
            self.focused = window.has_focus();
            self.renderer = Some(renderer);
            self.window = Some(window);
            self.reset_time();
            Ok(())
        })();
        if let Err(e) = result {
            self.fail(events, e);
        }
    }
    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.renderer = None;
        self.state = None;
        self.window = None;
        self.reset_time();
    }
    fn window_event(&mut self, events: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = &self.window else {
            return;
        };
        if window.id() != id {
            return;
        }
        if let Some(state) = &mut self.state {
            let _ = state.on_window_event(window, &event);
        }
        match event {
            WindowEvent::CloseRequested => {
                if self.smoke {
                    events.exit();
                } else {
                    self.ui.close_requested = true;
                }
            }
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if !focused {
                    self.ui.cancel_drag();
                    self.editor.document.end_gesture();
                }
                self.reset_time();
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                self.reset_time();
            }
            WindowEvent::Resized(size) => {
                self.minimized = size.width == 0 || size.height == 0;
                self.reset_time();
                if self.presented >= 60 && !self.minimized {
                    self.resized = true;
                }
                if let Some(renderer) = &mut self.renderer {
                    if let Err(e) = renderer.resize(size.width, size.height) {
                        self.fail(events, e);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.draw(events) {
                    self.fail(events, e);
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        let now = Instant::now();
        if self.smoke && now.duration_since(self.started) > Duration::from_secs(30) {
            self.fail(
                events,
                Problem::new("EDITOR_TEST", "/smoke", "editor window test timed out"),
            );
            return;
        }
        if now >= self.next_frame {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            self.next_frame = now
                + if !self.smoke && (self.minimized || self.occluded || !self.focused) {
                    Duration::from_millis(100)
                } else {
                    Duration::from_secs_f64(1. / 60.)
                };
        }
        events.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }
}
fn run(args: &[String]) -> Result<(), Problem> {
    let (file, smoke) = match args {
        [] => (None, false),
        [flag, path] if flag == "--project" => (Some(ProjectFile::open(path)?), false),
        [flag] if flag == "--smoke-test" => (None, true),
        [flag] if flag == "--help" => {
            println!(
                "{}",
                json!({"schema_version":1,"commands":["tomo-editor","tomo-editor --project DIRECTORY","tomo-editor --smoke-test"],"template":"editable reach-the-goal rules","schemas":["project v2 (v1 import)","scene v1","logic v1"],"game_logic_editor":true})
            );
            return Ok(());
        }
        _ => {
            return Err(Problem::new(
                "ARGUMENT",
                "/",
                "use --help for editor commands",
            ));
        }
    };
    let events =
        EventLoop::new().map_err(|e| Problem::new("WINDOW", "/event_loop", e.to_string()))?;
    let mut app = App {
        audio: None,
        audio_tried: false,
        editor: Editor::new(file)?,
        ui: EditorUi::default(),
        ctx: egui::Context::default(),
        state: None,
        window: None,
        renderer: None,
        clock: FrameClock::default(),
        last_frame: Instant::now(),
        next_frame: Instant::now(),
        started: Instant::now(),
        focused: true,
        occluded: false,
        minimized: false,
        was_playing: false,
        error: None,
        smoke,
        presented: 0,
        resized: false,
        smoke_moved: false,
    };
    events
        .run_app(&mut app)
        .map_err(|e| Problem::new("WINDOW", "/event_loop", e.to_string()))?;
    if let Some(error) = app.error {
        return Err(error);
    }
    if smoke && app.presented < 120 {
        return Err(Problem::new(
            "EDITOR_TEST",
            "/smoke",
            "window closed before test completed",
        ));
    }
    println!(
        "{}",
        json!({"schema_version":1,"ok":true,"presented_frames":app.presented,"smoke_test":smoke,"project":app.editor.document.project().name,
        "adapter":app.renderer.as_ref().map(|r|&r.adapter_name),"backend":app.renderer.as_ref().map(|r|&r.backend)})
    );
    Ok(())
}
fn main() -> ExitCode {
    let args = std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| Problem::new("ARGUMENT", "/", "expected UTF-8 arguments"))
        })
        .collect::<Result<Vec<_>, _>>();
    match args.and_then(|args| run(&args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let code = if matches!(
                error.code.as_str(),
                "WINDOW" | "RENDER" | "GPU_UNAVAILABLE" | "EDITOR_TEST" | "SIMULATION"
            ) {
                3
            } else {
                2
            };
            println!("{}", json!({"schema_version":1,"ok":false,"error":error}));
            ExitCode::from(code)
        }
    }
}
