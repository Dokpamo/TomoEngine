// SPDX-License-Identifier: MIT
use tomo_play::{Game, Replay, view};
use tomo_render::{Capture, capture_styled};

fn orange_center(image: &Capture) -> [f32; 2] {
    let mut x = 0.;
    let mut y = 0.;
    let mut count = 0.;
    for (i, pixel) in image.rgba.chunks_exact(4).enumerate() {
        if pixel[0] > 180 && (80..200).contains(&pixel[1]) && pixel[2] < 130 {
            x += (i % image.metadata.options.width as usize) as f32;
            y += (i / image.metadata.options.width as usize) as f32;
            count += 1.;
        }
    }
    assert!(count > 200., "player must produce visible pixels");
    [x / count, y / count]
}

#[test]
fn invalid_styles_fail_before_requesting_adapter() {
    let game = Game::new().unwrap();
    let options = view::options(1120, 760, false);
    let mut style = view::style(&game, &options, false);
    style.body_colors.insert("missing".into(), [1.; 3]);
    assert_eq!(
        capture_styled(game.scene(), game.snapshot(), &options, &style)
            .err()
            .unwrap()
            .code,
        "RENDER_INPUT"
    );
    let mut style = view::style(&game, &options, false);
    style.text[0].scale = f32::NAN;
    assert_eq!(
        capture_styled(game.scene(), game.snapshot(), &options, &style)
            .err()
            .unwrap()
            .code,
        "RENDER_INPUT"
    );
}

#[test]
#[ignore = "requires a real GPU adapter; run explicitly in GPU CI"]
fn rendered_player_moves_from_start_to_goal_and_hud_draws_on_top() {
    let mut game = Game::new().unwrap();
    let options = view::options(1120, 760, false);
    let start = capture_styled(
        game.scene(),
        game.snapshot(),
        &options,
        &view::style(&game, &options, false),
    )
    .unwrap();
    Replay::from_json(include_str!(
        "../../../examples/replays/box_run_win.replay.json"
    ))
    .unwrap()
    .run(&mut game)
    .unwrap();
    let finish = capture_styled(
        game.scene(),
        game.snapshot(),
        &options,
        &view::style(&game, &options, false),
    )
    .unwrap();
    let a = orange_center(&start);
    let b = orange_center(&finish);
    assert!(a[0] < 400. && a[1] > 480., "{a:?}");
    assert!(b[0] > 700. && b[1] < 280., "{b:?}");
    let pixel = |image: &Capture, x: usize, y: usize| -> [u8; 4] {
        image.rgba[(y * 1120 + x) * 4..(y * 1120 + x + 1) * 4]
            .try_into()
            .unwrap()
    };
    let banner = pixel(&finish, 400, 302);
    assert!(banner[1] > 200 && banner[2] > 150, "win banner is green");
    assert_ne!(
        pixel(&start, 400, 302),
        banner,
        "win overlay must actually be rendered"
    );
}
