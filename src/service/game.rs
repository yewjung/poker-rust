use std::collections::HashSet;

use eyre::{ensure, ContextCompat, Result};
use poker::Evaluator;
use uuid::Uuid;

use crate::domain::room::{Action, Room};
use crate::repository::rooms::RoomRepository;

pub struct GameService {
    pub evaluator: Evaluator,
    pub room_repository: RoomRepository,
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

    pub fn create_room(&mut self) -> Result<Room> {
        let room = Room::new();
        self.room_repository.insert(room)
    }

    // this function takes action from a player
    pub fn take_action(&mut self, room_id: Uuid, player_id: Uuid, action: Action) -> Result<()> {
        let mut room = self
            .room_repository
            .get(room_id)
            .wrap_err("Room not found")?
            .clone();
        let action_required = room.take_action(player_id, action)?;
        match action_required {
            ServiceRequiredAction::NoAction => {}
            ServiceRequiredAction::FindWinners => {
                let winners = self.find_winners(&room)?;
                room.split_pot(winners);
                room.proceed()?;
            }
        }
        self.room_repository.update(room_id, room)?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum ServiceRequiredAction {
    NoAction,
    FindWinners,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::room::{Player, Stage};

    #[test]
    fn test_whole_game_flow() -> Result<()> {
        // setup
        let mut service = GameService {
            evaluator: Evaluator::new(),
            room_repository: RoomRepository::new(),
        };

        // create room
        let mut room = service.create_room()?;

        assert_eq!(room.stage, Stage::NotEnoughPlayers);

        // join players
        let alice = Player::new("Alice".to_string(), 500);
        let alice_id = alice.id;
        room.join_player(alice)?;
        assert_eq!(room.stage, Stage::NotEnoughPlayers);

        let bob = Player::new("Bob".to_string(), 500);
        let bob_id = bob.id;
        room.join_player(bob)?;

        assert_eq!(room.stage, Stage::PreFlop);
        assert_eq!(room.player_in_turn, Some(alice_id));
        assert_eq!(room.community_cards.len(), 0);

        // alice takes action

        Ok(())
    }
}
