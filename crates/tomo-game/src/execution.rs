// SPDX-License-Identifier: MIT
use crate::{Action, Condition, ContactKind, Game, Input, RuleEvent, StartMode, Status, Trigger};
use std::collections::BTreeSet;
use tomo_scene::Problem;

type Contacts = BTreeSet<(String, String, ContactKind)>;
impl Game {
    fn terminal(&self) -> bool {
        matches!(self.status, Status::Won | Status::Lost)
    }

    pub(crate) fn advance(&mut self, input: Input) -> Result<(), Problem> {
        if self.terminal() {
            return Ok(());
        }
        if self.status == Status::Ready
            && self.rules.logic.start == StartMode::FirstInput
            && input.direction() == [0., 0.]
            && !input.action
        {
            return Ok(());
        }
        self.status = Status::Playing;
        let next = self
            .elapsed_ticks
            .checked_add(1)
            .ok_or_else(|| Problem::new("LIMIT", "/ticks", "game clock overflow"))?;
        self.run_rules(false, input, next, &Contacts::new())?;
        // A pre-physics terminal action still completes this tick. No later rules run.
        self.snapshot = self.runtime.step()?;
        self.elapsed_ticks = next;
        if !self.terminal() {
            let contacts = self
                .runtime
                .contact_events()?
                .into_iter()
                .map(|e| {
                    let (a, b) = if e.body_a <= e.body_b {
                        (e.body_a, e.body_b)
                    } else {
                        (e.body_b, e.body_a)
                    };
                    (a, b, e.kind)
                })
                .collect();
            self.run_rules(true, input, next, &contacts)?;
        }
        self.previous_input = input;
        if self.status == Status::Won {
            self.best_ticks = Some(self.best_ticks.map_or(next, |best| best.min(next)));
        }
        Ok(())
    }

    fn run_rules(
        &mut self,
        post: bool,
        input: Input,
        tick: u32,
        contacts: &Contacts,
    ) -> Result<(), Problem> {
        for index in 0..self.rules.logic.rules.len() {
            let rule = &self.rules.logic.rules[index];
            if !rule.enabled || (rule.once && self.rule_fires[index] > 0) {
                continue;
            }
            let post_trigger = matches!(
                rule.trigger,
                Trigger::GoalReached {} | Trigger::Contact { .. }
            );
            if post != post_trigger {
                continue;
            }
            let triggered = match &rule.trigger {
                Trigger::Start {} => tick == 1,
                Trigger::EveryTick {} => true,
                Trigger::KeyPressed { key } => key.held(input) && !key.held(self.previous_input),
                Trigger::Timer { ticks, repeat } => {
                    tick == *ticks || (*repeat && tick % ticks == 0)
                }
                Trigger::GoalReached {} => self.rules.reached(self.scene(), self.player().position),
                Trigger::Contact {
                    body_a,
                    body_b,
                    kind,
                } => {
                    let (a, b) = if body_a <= body_b {
                        (body_a, body_b)
                    } else {
                        (body_b, body_a)
                    };
                    contacts.contains(&(a.clone(), b.clone(), *kind))
                }
            };
            if !triggered
                || !rule.conditions.iter().all(|c| match c {
                    Condition::KeyHeld { key } => key.held(input),
                    Condition::ScoreAtLeast { value } => self.score >= *value,
                    Condition::HeightBelow { body, y } => self.state(body).position[1] < *y,
                })
            {
                continue;
            }
            // The document is immutable while playing. Cloning this bounded action list
            // releases its borrow while ordered actions change the live game state.
            let actions = rule.actions.clone();
            let event = RuleEvent {
                tick,
                rule_id: rule.id.clone(),
            };
            for action in actions {
                self.apply_action(action, input)?;
                if self.terminal() {
                    break;
                }
            }
            self.rule_fires[index] += 1; // at most one fire per completed, non-wrapping tick
            if self.recent_events.len() == 32 {
                self.recent_events.remove(0);
            }
            self.recent_events.push(event);
            if self.terminal() {
                break;
            }
        }
        Ok(())
    }

    fn state(&self, id: &str) -> &tomo_runtime::BodyState {
        self.snapshot
            .bodies
            .iter()
            .find(|b| b.id == id)
            .expect("validated rule body")
    }
    fn velocity(&mut self, id: &str, linear: [f32; 3], angular: [f32; 3]) -> Result<(), Problem> {
        self.runtime.set_body_velocity(id, linear, angular)?;
        // Later actions must see earlier velocity changes in this same phase.
        self.snapshot = self.runtime.snapshot()?;
        Ok(())
    }
    fn apply_action(&mut self, action: Action, input: Input) -> Result<(), Problem> {
        match action {
            Action::MovePlayer {} => {
                let [x, z] = input.direction();
                let player = self.player();
                let [qx, qy, qz, qw] = player.rotation;
                let gain = -12. * if qw < 0. { -1. } else { 1. };
                let linear = [
                    x * self.rules.speed,
                    player.linear_velocity[1],
                    z * self.rules.speed,
                ];
                self.velocity(
                    &self.rules.player.clone(),
                    linear,
                    [qx * gain, qy * gain, qz * gain],
                )?;
            }
            Action::SetVelocity { body, linear } => {
                let angular = self.state(&body).angular_velocity;
                self.velocity(&body, linear, angular)?;
            }
            Action::AddScore { amount } => {
                self.score = self
                    .score
                    .checked_add(amount)
                    .filter(|s| (-1_000_000_000..=1_000_000_000).contains(s))
                    .ok_or_else(|| {
                        Problem::new(
                            "RULE_EXECUTION",
                            "/game/score",
                            "score exceeds +/-1000000000; restart the game",
                        )
                    })?;
            }
            Action::Win {} => self.status = Status::Won,
            Action::Lose {} => self.status = Status::Lost,
        }
        Ok(())
    }
}
