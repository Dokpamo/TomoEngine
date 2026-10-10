// SPDX-License-Identifier: MIT
use serde_json::json;
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};
use tomo_play::{FrameClock, Game, Input, view};
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

struct App {
    game: Game,
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
}

impl App {
    fn reset_input(&mut self) {
        self.keys.clear();
        self.clock.reset();
        self.last_frame = Instant::now();
    }
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: Problem) {
        self.error = Some(error);
        event_loop.exit();
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
            let input = if self.smoke_frames.is_some() {
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
            for _ in 0..steps {
                self.game.step(input)?;
            }
        }
        let paused = !self.active();
        if let (Some(window), Some(renderer)) = (&self.window, &mut self.renderer) {
            let size = window.inner_size();
            if size.width == 0 || size.height == 0 {
                self.reset_input();
                return Ok(());
            }
            let options = view::options(size.width, size.height, self.colliders);
            let style = view::style(&self.game, &options, paused);
            window.pre_present_notify();
            if renderer.present(self.game.scene(), self.game.snapshot(), &options, &style)? {
                self.presented = self.presented.saturating_add(1);
            }
            if let Some(limit) = self.smoke_frames {
                if self.presented == 30 {
                    let _ = window.request_inner_size(LogicalSize::new(1000., 700.));
                }
                if self.presented >= limit {
                    if self.game.player().position[0] < 1.0 || !self.smoke_resized {
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
            .with_title(format!("Tomo Engine - {}", self.game.scene().name))
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
        let size = window.inner_size();
        match WindowRenderer::new(window.clone(), size.width, size.height) {
            Ok(renderer) => self.renderer = Some(renderer),
            Err(error) => {
                self.fail(event_loop, error);
                return;
            }
        }
        self.focused = window.has_focus();
        window.request_redraw();
        self.window = Some(window);
        self.reset_input();
    }
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
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
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                self.reset_input();
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                self.reset_input();
            }
            WindowEvent::Resized(size) => {
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
                                self.reset_input();
                                self.paused = false;
                                if let Err(error) = self.game.restart() {
                                    self.fail(event_loop, error);
                                }
                            }
                            KeyCode::KeyP => {
                                self.paused = !self.paused;
                                self.reset_input();
                            }
                            KeyCode::KeyC => self.colliders = !self.colliders,
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

pub fn run(game: Game, smoke_frames: Option<u32>) -> Result<u8, Problem> {
    let event_loop =
        EventLoop::new().map_err(|e| Problem::new("WINDOW", "/event_loop", e.to_string()))?;
    let mut app = App {
        game,
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
