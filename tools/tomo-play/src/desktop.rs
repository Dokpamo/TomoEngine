// SPDX-License-Identifier: MIT
use crate::{FrameClock, Input, view};
use serde_json::json;
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};
use tomo_project::Session;
use tomo_render::WindowRenderer;
use tomo_scene::Problem;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

// winit 0.30 reports iOS safe-area dimensions as inner_size, but UIKit touch
// coordinates and the Metal layer span the full window. Use one coordinate space.
fn surface_size(window: &Window) -> winit::dpi::PhysicalSize<u32> {
    if cfg!(target_os = "ios") {
        window.outer_size()
    } else {
        window.inner_size()
    }
}

struct App {
    audio: Option<tomo_render::audio::AudioPlayer>,
    audio_tried: bool,
    notice: Option<String>,
    game: Session,
    behavior: Option<Box<dyn tomo_game::sdk::Behavior>>,
    renderer: Option<WindowRenderer>,
    window: Option<Arc<Window>>,
    keys: BTreeSet<KeyCode>,
    clock: FrameClock,
    last_frame: Instant,
    next_redraw: Instant,
    started: Instant,
    focused: bool,
    paused: bool,
    occluded: bool,
    minimized: bool,
    colliders: bool,
    error: Option<Problem>,
    smoke_frames: Option<u32>,
    presented: u32,
    smoke_resized: bool,
    save_path: std::path::PathBuf,
    pointer: [f32; 2],
    touches: std::collections::BTreeMap<u64, [f32; 2]>,
}

impl App {
    fn reset_input(&mut self) {
        self.touches.clear();
        if let Some(a) = &self.audio {
            a.clear();
        }
        self.keys.clear();
        self.clock.reset();
        self.last_frame = Instant::now();
    }
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: Problem) {
        self.error = Some(error);
        event_loop.exit();
    }
    fn action(&mut self, action: tomo_scene::UiAction) {
        if self.behavior.is_some()
            && matches!(
                action,
                tomo_scene::UiAction::Save | tomo_scene::UiAction::Load
            )
        {
            if let Some(w) = &self.window {
                w.set_title("SDK game: implement native behavior persistence in your host");
            }
            return;
        }
        let result = match action {
            tomo_scene::UiAction::Restart => self.game.restart(),
            tomo_scene::UiAction::Pause => {
                self.paused = true;
                Ok(())
            }
            tomo_scene::UiAction::Resume => {
                self.paused = false;
                self.game.begin();
                Ok(())
            }
            tomo_scene::UiAction::Save => (|| {
                if let Some(parent) = self.save_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        Problem::new("SAVE_PATH", parent.display().to_string(), e.to_string())
                    })?;
                }
                self.game.save_progress(&self.save_path)
            })(),
            tomo_scene::UiAction::Load => self.game.load_progress(&self.save_path),
            tomo_scene::UiAction::NextScene => self.game.next_scene(),
        };
        if result.is_ok()
            && matches!(
                action,
                tomo_scene::UiAction::Restart | tomo_scene::UiAction::NextScene
            )
        {
            if let Some(b) = &mut self.behavior {
                b.reset();
            }
        }
        self.notice = Some(match &result {
            Ok(()) => format!("{:?}", action).to_ascii_uppercase(),
            Err(e) => format!("ERROR {}", e.code.replace('_', " ")),
        });
        self.reset_input();
        if let Some(window) = &self.window {
            window.set_title(&match result {
                Ok(()) => format!("Tomo - {} / {action:?}", self.game.scene().name),
                Err(e) => format!("Tomo - {}", e),
            });
        }
    }
    fn click(&mut self, p: [f32; 2]) -> bool {
        if let Some(action) = tomo_render::hud_action(
            self.game.content(),
            view::hud_state(&self.game, self.paused),
            p,
        ) {
            self.action(action);
            true
        } else {
            false
        }
    }
    fn active(&self) -> bool {
        !self.minimized
            && (self.smoke_frames.is_some() || (self.focused && !self.paused && !self.occluded))
    }
    fn draw(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Problem> {
        let now = Instant::now();
        if self.smoke_frames.is_some() && now.duration_since(self.started) > Duration::from_secs(30)
        {
            return Err(Problem::new(
                "WINDOW",
                "/smoke-test",
                "window test timed out",
            ));
        }
        let elapsed = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        if self.active() {
            let steps = if self.smoke_frames.is_some() {
                1
            } else {
                self.clock
                    .advance(elapsed, f64::from(self.game.scene().fixed_dt))
            };
            let key = |a, b| self.keys.contains(&a) || self.keys.contains(&b);
            let mut input = if self.smoke_frames.is_some() {
                Input {
                    right: true,
                    ..Input::default()
                }
            } else {
                Input {
                    up: key(KeyCode::KeyW, KeyCode::ArrowUp),
                    down: key(KeyCode::KeyS, KeyCode::ArrowDown),
                    left: key(KeyCode::KeyA, KeyCode::ArrowLeft),
                    right: key(KeyCode::KeyD, KeyCode::ArrowRight),
                    action: self.keys.contains(&KeyCode::Space),
                }
            };
            for p in self.touches.values() {
                crate::touch::apply(*p, &mut input);
            }
            for _ in 0..steps {
                if let Some(b) = &mut self.behavior {
                    self.game.step_behavior(b.as_mut(), input)?;
                } else {
                    self.game.step(input)?;
                }
                let sounds = self.game.take_sounds();
                if !sounds.is_empty() {
                    if !self.audio_tried {
                        self.audio_tried = true;
                        match tomo_render::audio::AudioPlayer::new() {
                            Ok(a) => self.audio = Some(a),
                            Err(e) => eprintln!("{}", json!({"warning":e})),
                        }
                    }
                    if let Some(audio) = &self.audio {
                        for cue in sounds {
                            if let Err(e) = audio.play(&cue) {
                                eprintln!("{}", json!({"warning":e}));
                            }
                        }
                    }
                }
            }
        }
        let paused = !self.active();
        if let (Some(window), Some(renderer)) = (&self.window, &mut self.renderer) {
            let size = surface_size(window);
            if size.width == 0 || size.height == 0 {
                self.reset_input();
                return Ok(());
            }
            renderer.resize(size.width, size.height)?;
            let (options, mut style) =
                view::frame(&self.game, size.width, size.height, self.colliders, paused)?;
            if cfg!(any(target_os = "ios", target_os = "android")) {
                crate::touch::overlay(&options, &mut style);
            }
            if let Some(notice) = &self.notice {
                style.text.push(tomo_render::OverlayText {
                    position: [options.width as f32 * 0.35, options.height as f32 * 0.82],
                    scale: (options.height as f32 / 150.).max(0.1),
                    text: notice.clone(),
                    color: [1., 0.85, 0.2],
                });
            }
            window.pre_present_notify();
            if renderer.present(self.game.scene(), self.game.snapshot(), &options, &style)? {
                self.presented = self.presented.saturating_add(1);
            }
            if let Some(limit) = self.smoke_frames {
                if self.presented == 30 {
                    let _ = window.request_inner_size(LogicalSize::new(1000., 700.));
                }
                if self.presented >= limit {
                    if (!self.game.rules().player.is_empty()
                        && self.game.player().position[0] < 1.0)
                        || !self.smoke_resized
                    {
                        return Err(Problem::new(
                            "SIMULATION",
                            "/smoke-test",
                            "player did not advance or window did not resize",
                        ));
                    }
                    event_loop.exit();
                }
            }
        }
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            self.reset_input();
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(format!("Tomo Engine - {}", self.game.scene().name));
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let attributes = attributes
            .with_inner_size(LogicalSize::new(1120., 760.))
            .with_min_inner_size(LogicalSize::new(800., 600.));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.fail(
                    event_loop,
                    Problem::new("WINDOW", "/window", error.to_string()),
                );
                return;
            }
        };
        let size = surface_size(&window);
        match WindowRenderer::new(window.clone(), size.width, size.height) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(error) => {
                self.fail(event_loop, error);
                return;
            }
        }
        self.focused = window.has_focus();
        self.minimized = size.width == 0 || size.height == 0;
        self.occluded = false;
        window.request_redraw();
        self.window = Some(window);
        self.reset_input();
    }
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.audio = None;
        self.audio_tried = false;
        self.renderer = None;
        self.window = None;
        self.reset_input();
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().is_none_or(|w| w.id() != id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(w) = &self.window {
                    let s = surface_size(w);
                    self.pointer = [
                        position.x as f32 / s.width.max(1) as f32,
                        position.y as f32 / s.height.max(1) as f32,
                    ];
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                self.click(self.pointer);
            }
            WindowEvent::Touch(touch) => {
                if let Some(w) = &self.window {
                    let size = surface_size(w);
                    let p = [
                        touch.location.x as f32 / size.width.max(1) as f32,
                        touch.location.y as f32 / size.height.max(1) as f32,
                    ];
                    if cfg!(debug_assertions) && std::env::var_os("TOMO_INPUT_TRACE").is_some() {
                        eprintln!(
                            "{}",
                            json!({"event":"touch","phase":format!("{:?}",touch.phase),"normalized":p,"window":[size.width,size.height],"action":tomo_render::hud_action(self.game.content(),view::hud_state(&self.game,self.paused),p)})
                        );
                    }
                    match touch.phase {
                        winit::event::TouchPhase::Started => {
                            if !self.click(p) {
                                self.touches.insert(touch.id, p);
                            }
                        }
                        winit::event::TouchPhase::Moved => {
                            if let Some(old) = self.touches.get_mut(&touch.id) {
                                *old = p;
                            }
                        }
                        _ => {
                            self.touches.remove(&touch.id);
                        }
                    }
                }
            }
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                self.reset_input();
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                self.reset_input();
            }
            WindowEvent::Resized(size) => {
                let size = if cfg!(target_os = "ios") {
                    self.window.as_ref().map_or(size, |w| surface_size(w))
                } else {
                    size
                };
                self.minimized = size.width == 0 || size.height == 0;
                if self.smoke_frames.is_some() && self.presented >= 30 && !self.minimized {
                    self.smoke_resized = true;
                }
                self.reset_input();
                if let Some(renderer) = &mut self.renderer {
                    if let Err(error) = renderer.resize(size.width, size.height) {
                        self.fail(event_loop, error);
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if event.state == ElementState::Released {
                        self.keys.remove(&code);
                    } else if !event.repeat {
                        match code {
                            KeyCode::Escape => event_loop.exit(),
                            KeyCode::KeyR => {
                                self.paused = false;
                                self.action(tomo_scene::UiAction::Restart);
                            }
                            KeyCode::KeyP => {
                                self.paused = !self.paused;
                                self.reset_input();
                            }
                            KeyCode::KeyC => self.colliders = !self.colliders,
                            KeyCode::F5 => self.action(tomo_scene::UiAction::Save),
                            KeyCode::F9 => self.action(tomo_scene::UiAction::Load),
                            KeyCode::KeyN => self.action(tomo_scene::UiAction::NextScene),
                            _ => {
                                if self.active() {
                                    self.keys.insert(code);
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.draw(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let interval = if self.active() {
            Duration::from_secs_f64(1.0 / 60.0)
        } else {
            Duration::from_millis(100)
        };
        let now = Instant::now();
        if self.smoke_frames.is_some() && now.duration_since(self.started) > Duration::from_secs(30)
        {
            self.fail(
                event_loop,
                Problem::new("WINDOW", "/smoke-test", "window test timed out"),
            );
            return;
        }
        if now >= self.next_redraw {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            self.next_redraw = now + interval;
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_redraw));
    }
}

pub fn run(
    game: Session,
    smoke_frames: Option<u32>,
    save_path: std::path::PathBuf,
) -> Result<u8, Problem> {
    run_inner(game, smoke_frames, save_path, None)
}
pub fn run_scripted(
    project: tomo_project::Project,
    behavior: Box<dyn tomo_game::sdk::Behavior>,
) -> Result<u8, Problem> {
    run_inner(
        Session::new(project)?,
        None,
        std::path::PathBuf::from("progress.tomo.json"),
        Some(behavior),
    )
}
fn run_inner(
    game: Session,
    smoke_frames: Option<u32>,
    save_path: std::path::PathBuf,
    behavior: Option<Box<dyn tomo_game::sdk::Behavior>>,
) -> Result<u8, Problem> {
    let event_loop =
        EventLoop::new().map_err(|e| Problem::new("WINDOW", "/event_loop", e.to_string()))?;
    run_loop(event_loop, game, smoke_frames, save_path, behavior)
}
fn run_loop(
    event_loop: EventLoop<()>,
    game: Session,
    smoke_frames: Option<u32>,
    save_path: std::path::PathBuf,
    behavior: Option<Box<dyn tomo_game::sdk::Behavior>>,
) -> Result<u8, Problem> {
    let mut app = App {
        audio: None,
        audio_tried: false,
        notice: None,
        game,
        behavior,
        renderer: None,
        window: None,
        keys: BTreeSet::new(),
        clock: FrameClock::default(),
        last_frame: Instant::now(),
        next_redraw: Instant::now(),
        started: Instant::now(),
        focused: true,
        paused: false,
        occluded: false,
        minimized: false,
        colliders: false,
        error: None,
        smoke_frames,
        presented: 0,
        smoke_resized: false,
        save_path,
        pointer: [0.; 2],
        touches: Default::default(),
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| Problem::new("WINDOW", "/event_loop", e.to_string()))?;
    if let Some(error) = app.error {
        return Err(error);
    }
    if smoke_frames.is_some_and(|frames| app.presented < frames) {
        return Err(Problem::new(
            "WINDOW",
            "/smoke-test",
            "window closed before test completed",
        ));
    }
    println!(
        "{}",
        json!({"schema_version":1,"status":app.game.status(),"seconds":app.game.seconds(),
        "snapshot":app.game.snapshot(),"presented_frames":app.presented,
        "adapter":app.renderer.as_ref().map(|r| &r.adapter_name),"backend":app.renderer.as_ref().map(|r| &r.backend)})
    );
    Ok(0)
}

/// NativeActivity host: project assets and private saves use Android-owned paths.
#[cfg(target_os = "android")]
pub fn run_android(app: winit::platform::android::activity::AndroidApp) {
    use std::io::Read;
    use winit::platform::android::EventLoopBuilderExtAndroid;
    let result = (|| {
        let manager = app.asset_manager();
        let name = std::ffi::CString::new("game/project.tomo.json").expect("static asset name");
        let mut asset = manager.open(&name).ok_or_else(|| {
            Problem::new("PROJECT_FILE", "/assets/game", "missing packaged project")
        })?;
        if asset.length() > tomo_scene::MAX_DOCUMENT_BYTES {
            return Err(Problem::new(
                "LIMIT",
                "/assets/game",
                "project exceeds 1 MiB",
            ));
        }
        let mut json = String::new();
        asset
            .read_to_string(&mut json)
            .map_err(|e| Problem::new("PROJECT_FILE", "/assets/game", e.to_string()))?;
        let session = Session::new(tomo_project::Project::from_json(&json)?)?;
        let saves = app
            .internal_data_path()
            .ok_or_else(|| Problem::new("SAVE_PATH", "/android", "private data path unavailable"))?
            .join("progress.tomo.json");
        let event_loop = EventLoop::builder()
            .with_android_app(app)
            .build()
            .map_err(|e| Problem::new("WINDOW", "/android", e.to_string()))?;
        run_loop(event_loop, session, None, saves, None)
    })();
    if let Err(error) = result {
        eprintln!("{}", json!({"schema_version":1,"ok":false,"error":error}));
    }
}
