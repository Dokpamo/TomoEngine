// SPDX-License-Identifier: MIT
use tomo_game::{
    Action, Condition, ContactKind, Game, GameRules, Input, Key, Logic, Replay, Rule, StartMode,
    Status, Trigger,
};
use tomo_scene::{Problem, Scene};

fn rule(id: &str, trigger: Trigger, actions: Vec<Action>) -> Rule {
    Rule {
        id: id.into(),
        name: id.into(),
        enabled: true,
        once: false,
        trigger,
        conditions: vec![],
        actions,
    }
}
fn settings(rules: Vec<Rule>) -> GameRules {
    GameRules {
        logic: Logic {
            schema_version: 1,
            start: StartMode::Immediate,
            rules,
        },
        ..Default::default()
    }
}
fn scene() -> Scene {
    Scene::from_json(tomo_game::LEVEL).unwrap()
}
fn game(rules: Vec<Rule>) -> Game {
    Game::from_scene(scene(), settings(rules)).unwrap()
}
fn step(game: &mut Game, count: u32, input: Input) {
    for _ in 0..count {
        game.step(input).unwrap();
    }
}

#[test]
fn authored_rules_replace_template_behavior_and_preserve_order_and_conditions() {
    let mut winning = rule("winning", Trigger::EveryTick {}, vec![Action::Win {}]);
    winning.conditions = vec![
        Condition::ScoreAtLeast { value: 4 },
        Condition::KeyHeld { key: Key::Action },
    ];
    let mut disabled = rule("disabled", Trigger::EveryTick {}, vec![Action::Lose {}]);
    disabled.enabled = false;
    let mut game = game(vec![
        rule(
            "start",
            Trigger::Start {},
            vec![Action::AddScore { amount: 2 }],
        ),
        rule(
            "timer",
            Trigger::Timer {
                ticks: 2,
                repeat: true,
            },
            vec![Action::AddScore { amount: 1 }],
        ),
        winning,
        disabled,
    ]);
    step(&mut game, 4, Input::default());
    assert_eq!(game.score(), 4);
    assert_eq!(game.status(), Status::Playing);
    assert!((game.player().position[0] + 5.).abs() < 0.001);
    step(
        &mut game,
        1,
        Input {
            action: true,
            ..Default::default()
        },
    );
    assert_eq!(game.status(), Status::Won);
    assert_eq!(game.elapsed_ticks(), 5);
    assert_eq!(game.rule_fires("start"), 1);
    assert_eq!(game.rule_fires("timer"), 2);
    assert_eq!(game.rule_fires("disabled"), 0);
    let stopped = game.snapshot().clone();
    step(&mut game, 10, Input::default());
    assert_eq!(game.snapshot(), &stopped);
    game.restart().unwrap();
    assert_eq!(game.score(), 0);
    assert_eq!(game.rule_fires("start"), 0);
    assert!(game.recent_events().is_empty());
    assert!(game.best_seconds().is_some());
}

#[test]
fn key_edges_move_real_bodies_and_ordered_velocity_actions_see_each_other() {
    let mut game = game(vec![
        rule("move", Trigger::EveryTick {}, vec![Action::MovePlayer {}]),
        rule(
            "jump",
            Trigger::KeyPressed { key: Key::Action },
            vec![
                Action::SetVelocity {
                    body: "player".into(),
                    linear: [0., 6., 0.],
                },
                Action::AddScore { amount: 1 },
            ],
        ),
    ]);
    let held = Input {
        action: true,
        right: true,
        ..Default::default()
    };
    step(&mut game, 12, held);
    assert!(game.player().position[1] > 1.);
    assert!(game.player().position[0] > -4.5);
    assert_eq!(game.score(), 1);
    step(&mut game, 1, Input::default());
    step(&mut game, 1, held);
    assert_eq!(game.score(), 2);
    assert!(game.player().linear_velocity[1] > 5.);
    // A later movement controller preserves an earlier rule's vertical velocity.
    let mut rules = settings(vec![
        rule(
            "launch",
            Trigger::Start {},
            vec![Action::SetVelocity {
                body: "player".into(),
                linear: [0., 6., 0.],
            }],
        ),
        rule("move", Trigger::EveryTick {}, vec![Action::MovePlayer {}]),
    ]);
    rules.logic.start = StartMode::FirstInput;
    let mut second = Game::from_scene(scene(), rules).unwrap();
    step(&mut second, 3, Input::default());
    assert_eq!(second.elapsed_ticks(), 0);
    second.step(held).unwrap();
    assert!(second.player().linear_velocity[1] > 5.);
}

#[test]
fn real_sensor_contacts_have_stable_ids_and_started_persisted_stopped_events() {
    let mut scene = scene();
    scene
        .bodies
        .iter_mut()
        .find(|b| b.id == "goal")
        .unwrap()
        .position = [-5., 0.035, 4.];
    let mut runtime = tomo_runtime::Runtime::from_scene(scene.clone()).unwrap();
    assert!(runtime.contact_events().unwrap().is_empty());
    runtime.step().unwrap();
    assert!(
        runtime
            .contact_events()
            .unwrap()
            .iter()
            .any(|c| c.body_a == "goal"
                && c.body_b == "player"
                && c.is_sensor
                && c.kind == ContactKind::Started)
    );
    let mut rules = vec![rule(
        "move",
        Trigger::EveryTick {},
        vec![Action::MovePlayer {}],
    )];
    for (id, kind) in [
        ("enter", ContactKind::Started),
        ("stay", ContactKind::Persisted),
        ("leave", ContactKind::Stopped),
    ] {
        rules.push(rule(
            id,
            Trigger::Contact {
                body_a: "player".into(),
                body_b: "goal".into(),
                kind,
            },
            vec![Action::AddScore { amount: 1 }],
        ));
    }
    let mut game = Game::from_scene(scene, settings(rules)).unwrap();
    step(&mut game, 8, Input::default());
    assert_eq!(game.rule_fires("enter"), 1);
    assert!(game.rule_fires("stay") >= 6);
    step(
        &mut game,
        40,
        Input {
            right: true,
            ..Default::default()
        },
    );
    assert_eq!(game.rule_fires("enter"), 1);
    assert_eq!(game.rule_fires("leave"), 1);
    assert!(game.recent_events().len() <= 32);
}

#[test]
fn once_only_consumes_a_matching_rule_and_terminal_actions_skip_later_rules() {
    let mut once = rule(
        "once",
        Trigger::EveryTick {},
        vec![Action::AddScore { amount: 3 }],
    );
    once.once = true;
    once.conditions
        .push(Condition::KeyHeld { key: Key::Action });
    let mut game = game(vec![
        once,
        rule(
            "timeout",
            Trigger::Timer {
                ticks: 5,
                repeat: false,
            },
            vec![Action::Lose {}],
        ),
        rule(
            "unreachable",
            Trigger::Timer {
                ticks: 5,
                repeat: false,
            },
            vec![Action::Win {}],
        ),
    ]);
    step(&mut game, 2, Input::default());
    assert_eq!(game.score(), 0);
    step(
        &mut game,
        9,
        Input {
            action: true,
            ..Default::default()
        },
    );
    assert_eq!(game.score(), 3);
    assert_eq!(game.status(), Status::Lost);
    assert_eq!(game.elapsed_ticks(), 5);
    assert_eq!(game.rule_fires("unreachable"), 0);
    assert_eq!(game.best_seconds(), None);
}

#[test]
fn falling_condition_and_score_failure_are_fail_stop_until_restart() {
    let mut falling = rule("fall", Trigger::EveryTick {}, vec![Action::Lose {}]);
    falling.conditions.push(Condition::HeightBelow {
        body: "player".into(),
        y: 0.5,
    });
    let mut first = game(vec![falling]);
    first.step(Input::default()).unwrap();
    assert_eq!(first.status(), Status::Lost);
    let mut overflow = game(vec![rule(
        "score",
        Trigger::EveryTick {},
        vec![Action::AddScore { amount: 1_000_000 }],
    )]);
    step(&mut overflow, 1000, Input::default());
    assert_eq!(
        overflow.step(Input::default()).unwrap_err().code,
        "RULE_EXECUTION"
    );
    let failed = overflow.snapshot().clone();
    assert_eq!(
        overflow.step(Input::default()).unwrap_err().code,
        "FAILED_GAME"
    );
    assert_eq!(overflow.snapshot(), &failed);
    overflow.restart().unwrap();
    overflow.step(Input::default()).unwrap();
    assert_eq!(overflow.score(), 1_000_000);
}

#[test]
fn strict_program_validation_rejects_invalid_data_even_in_disabled_rules() {
    let scene = scene();
    let base = settings(vec![rule(
        "test",
        Trigger::Start {},
        vec![Action::AddScore { amount: 1 }],
    )]);
    let bad = |mutate: fn(&mut GameRules)| -> Problem {
        let mut rules = base.clone();
        mutate(&mut rules);
        rules.validate(&scene).unwrap_err()
    };
    assert_eq!(bad(|r| r.logic.schema_version = 9).code, "VERSION");
    bad(|r| r.logic.rules[0].id = "../bad".into());
    bad(|r| r.logic.rules[0].name = "\n".into());
    bad(|r| r.logic.rules.push(r.logic.rules[0].clone()));
    bad(|r| r.logic.rules[0].actions.clear());
    bad(|r| r.logic.rules[0].actions = vec![Action::Win {}, Action::AddScore { amount: 1 }]);
    bad(|r| {
        r.logic.rules[0].trigger = Trigger::Timer {
            ticks: 0,
            repeat: true,
        }
    });
    bad(|r| {
        r.logic.rules[0].actions = vec![Action::SetVelocity {
            body: "floor".into(),
            linear: [1.; 3],
        }]
    });
    bad(|r| {
        r.logic.rules[0].actions = vec![Action::SetVelocity {
            body: "player".into(),
            linear: [f32::NAN; 3],
        }]
    });
    bad(|r| {
        r.logic.rules[0].enabled = false;
        r.logic.rules[0].conditions = vec![Condition::HeightBelow {
            body: "missing".into(),
            y: 0.,
        }];
    });
    bad(|r| r.logic.rules[0].conditions = vec![Condition::KeyHeld { key: Key::Up }; 9]);
    bad(|r| r.logic.rules[0].actions = vec![Action::Win {}; 9]);
    bad(|r| r.logic.rules = vec![r.logic.rules[0].clone(); 65]);
    for invalid in [
        r#"{"type":"start","execute":"code"}"#,
        r#"{"type":"timer","ticks":1,"repeat":true,"unknown":true}"#,
        r#"{"type":"script","source":"code"}"#,
    ] {
        assert!(
            serde_json::from_str::<Trigger>(invalid).is_err(),
            "{invalid}"
        );
    }
    assert!(serde_json::from_str::<Action>(r#"{"type":"win","execute":"code"}"#).is_err());
}

#[test]
fn replay_versions_and_rule_work_are_checked_before_mutation() {
    let mut game = game(vec![rule(
        "space",
        Trigger::KeyPressed { key: Key::Action },
        vec![Action::Win {}],
    )]);
    let replay = Replay::from_json(r#"{"schema_version":2,"segments":[{"ticks":1,"input":{"action":true}}],"expect_win":true}"#).unwrap();
    assert!(replay.run(&mut game).unwrap());
    for action in ["true", "false"] {
        let json = format!(
            r#"{{"schema_version":1,"segments":[{{"ticks":1,"input":{{"action":{action}}}}}],"expect_win":false}}"#
        );
        assert!(Replay::from_json(&json).is_err());
    }
    let rules = (0..64)
        .map(|i| {
            rule(
                &format!("r{i}"),
                Trigger::EveryTick {},
                vec![Action::AddScore { amount: 0 }; 8],
            )
        })
        .collect();
    let mut large = Game::from_scene(scene(), settings(rules)).unwrap();
    let initial = large.snapshot().clone();
    let replay = Replay::from_json(
        r#"{"schema_version":2,"segments":[{"ticks":36000,"input":{}}],"expect_win":false}"#,
    )
    .unwrap();
    assert_eq!(replay.run(&mut large).unwrap_err().code, "LIMIT");
    assert_eq!(large.snapshot(), &initial);
    assert_eq!(large.score(), 0);
}
