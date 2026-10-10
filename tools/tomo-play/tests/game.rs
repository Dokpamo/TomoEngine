// SPDX-License-Identifier: MIT
use tomo_play::{FrameClock, Game, Input, Replay, Status};

const WIN: &str = include_str!("../../../examples/replays/box_run_win.replay.json");

#[test]
fn fixed_input_replay_reaches_goal_and_restart_preserves_only_best() {
    let replay = Replay::from_json(WIN).unwrap();
    let mut game = Game::new().unwrap();
    let mut other = Game::new().unwrap();
    let initial = game.snapshot().clone();
    for _ in 0..60 {
        game.step(Input::default()).unwrap();
    }
    assert_eq!(*game.snapshot(), initial, "clock starts on first movement");
    assert!(replay.run(&mut game).unwrap());
    assert!(replay.run(&mut other).unwrap());
    assert_eq!(game.status(), Status::Won);
    assert_eq!(game.snapshot(), other.snapshot());
    assert!((game.player().position[0] - 5.).abs() < 0.5);
    assert!((game.player().position[2] + 4.).abs() < 0.5);
    let finish = game.snapshot().clone();
    let best = game.best_seconds();
    for _ in 0..30 {
        game.step(Input {
            left: true,
            ..Input::default()
        })
        .unwrap();
    }
    assert_eq!(*game.snapshot(), finish, "completed round is frozen");
    game.restart().unwrap();
    assert_eq!(*game.snapshot(), initial);
    assert_eq!(game.status(), Status::Ready);
    assert_eq!(game.elapsed_ticks(), 0);
    assert_eq!(game.best_seconds(), best);
}

#[test]
fn solid_wall_blocks_player_and_releasing_key_stops_horizontal_motion() {
    let mut game = Game::new().unwrap();
    for _ in 0..240 {
        game.step(Input {
            up: true,
            ..Input::default()
        })
        .unwrap();
    }
    assert!(game.player().position[2] > 1.4, "{:?}", game.player());
    assert_eq!(game.status(), Status::Playing);
    assert!(
        game.player().position[1] < 0.9,
        "must not climb over the wall"
    );
    game.restart().unwrap();
    for _ in 0..30 {
        game.step(Input {
            right: true,
            ..Input::default()
        })
        .unwrap();
    }
    let before = game.player().position[0];
    for _ in 0..120 {
        game.step(Input::default()).unwrap();
    }
    assert!((game.player().position[0] - before).abs() < 0.08);
}

#[test]
fn diagonals_are_normalized_and_opposing_inputs_cancel() {
    let input = Input {
        up: true,
        right: true,
        ..Input::default()
    };
    let [x, z] = input.direction();
    assert!((x * x + z * z - 1.).abs() < 1e-6);
    assert_eq!(
        Input {
            up: true,
            down: true,
            left: true,
            right: true
        }
        .direction(),
        [0., 0.]
    );
    let mut diagonal = Game::new().unwrap();
    let mut straight = Game::new().unwrap();
    for _ in 0..20 {
        diagonal.step(input).unwrap();
        straight
            .step(Input {
                right: true,
                ..Input::default()
            })
            .unwrap();
    }
    let p = diagonal.player().position;
    let length = ((p[0] + 5.).powi(2) + (p[2] - 4.).powi(2)).sqrt();
    assert!((length - (straight.player().position[0] + 5.)).abs() < 0.05);
}

#[test]
fn game_outlives_cli_tick_budget() {
    let mut game = Game::new().unwrap();
    game.step(Input {
        right: true,
        ..Input::default()
    })
    .unwrap();
    for _ in 0..3601 {
        game.step(Input::default()).unwrap();
    }
    assert_eq!(game.snapshot().tick, 3602);
    assert!(game.player().position[1] > 0.3);
}

#[test]
fn frame_clock_is_partition_independent_bounded_and_resettable() {
    let dt = f64::from(Game::new().unwrap().scene().fixed_dt);
    let mut high = FrameClock::default();
    let mut low = FrameClock::default();
    let a: u32 = (0..120).map(|_| high.advance(dt / 2., dt)).sum();
    let b: u32 = (0..30).map(|_| low.advance(dt * 2., dt)).sum();
    assert_eq!(a, 60);
    assert_eq!(a, b);
    assert_eq!(low.advance(30., dt), FrameClock::MAX_STEPS);
    assert_eq!(low.advance(0., dt), 0);
    low.advance(dt / 2., dt);
    low.reset();
    assert_eq!(low.advance(dt / 2., dt), 0);
}

#[test]
fn invalid_replays_are_rejected_before_world_changes() {
    for json in [
        r#"{"schema_version":2,"segments":[],"expect_win":true}"#,
        r#"{"schema_version":1,"segments":[{"ticks":1,"input":{"teleport":true}}],"expect_win":true}"#,
        r#"{"schema_version":1,"segments":[{"ticks":36001,"input":{}}],"expect_win":true}"#,
        r#"{"schema_version":1,"segments":[{"ticks":0,"input":{}}],"expect_win":false}"#,
    ] {
        assert!(Replay::from_json(json).is_err());
    }
    let mut replay = Replay::from_json(WIN).unwrap();
    replay.segments[0].ticks = u32::MAX;
    let mut game = Game::new().unwrap();
    let initial = game.snapshot().clone();
    assert!(replay.run(&mut game).is_err());
    assert_eq!(*game.snapshot(), initial);
}
