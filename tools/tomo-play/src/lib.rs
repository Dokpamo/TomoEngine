// SPDX-License-Identifier: MIT
//! Desktop player for Tomo projects and the built-in Box Run example.
#![forbid(unsafe_code)]
pub use tomo_game::{
    FrameClock, Game, GameRules, Input, LEVEL, Replay, ReplaySegment, SPEED, Status,
};
pub mod desktop;
mod touch;
pub mod view;
