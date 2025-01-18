use std::collections::HashSet;

use eyre::{ensure, Result};
use poker::Evaluator;
use uuid::Uuid;

use crate::domain::room::Room;

pub struct GameService {
    pub evaluator: Evaluator,
}

impl GameService {
    pub fn find_winners(&self, room: &Room) -> Result<HashSet<Uuid>> {
        ensure!(room.is_showdown(), "Game is not in the showdown stage yet");
        let mut winners = HashSet::with_capacity(room.players.len());
        let mut best_hand = None;
        for (player_id, hand) in room.players_cards() {
            let hand = self.evaluator.evaluate(hand)?;
            match best_hand {
                None => {
                    best_hand = Some(hand);
                    winners.insert(player_id);
                }
                Some(best) if hand.is_better_than(best) => {
                    best_hand = Some(hand);
                    winners.clear();
                    winners.insert(player_id);
                }
                Some(best) if hand.is_equal_to(best) => {
                    winners.insert(player_id);
                }
                _ => {}
            }
        }
        Ok(winners)
    }
}
