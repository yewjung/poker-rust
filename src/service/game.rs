use std::collections::HashSet;

use eyre::{ensure, Result};
use poker::Evaluator;
use uuid::Uuid;

use crate::domain::room::{Action, Player, Room};
use crate::domain::user::User;
use crate::repository::rooms::RoomRepository;
use crate::repository::users::UserRepository;

pub struct GameService {
    pub evaluator: Evaluator,
    pub room_repository: RoomRepository,
    pub user_repository: UserRepository,
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
    pub fn take_action(&mut self, room_id: Uuid, player_id: Uuid, action: Action) -> Result<Room> {
        let mut room = self.room_repository.get(room_id)?;
        let action_required = room.take_action(player_id, action)?;
        match action_required {
            ServiceRequiredAction::NoAction => {}
            ServiceRequiredAction::FindWinners => {
                let winners = self.find_winners(&room)?;
                room.split_pot(winners);
                room.proceed()?;
            }
        }
        self.room_repository.update(room_id, room)
    }

    pub fn join_player(&mut self, room_id: Uuid, user_id: Uuid, buy_in: u32) -> Result<Room> {
        let mut room = self.room_repository.get(room_id)?;

        let mut user = self.user_repository.get(user_id)?;
        ensure!(buy_in <= user.balance, "Insufficient balance");

        room.join_player(Player::from_user(&user, buy_in))?;
        user.balance -= buy_in;
        self.user_repository.update(user_id, user)?;
        self.room_repository.update(room_id, room)
    }

    pub fn create_user(&mut self, name: String, balance: u32) -> Result<User> {
        self.user_repository.create_user(name, balance)
    }
}

#[derive(Debug)]
pub enum ServiceRequiredAction {
    NoAction,
    FindWinners,
}

#[cfg(test)]
mod tests {
    use eyre::ContextCompat;
    use super::*;
    use crate::domain::room::{Position, Stage};

    #[test]
    fn test_whole_game_flow() -> Result<()> {
        // setup
        let mut service = GameService {
            evaluator: Evaluator::new(),
            room_repository: RoomRepository::new(),
            user_repository: UserRepository::new(),
        };
        let alice = service.create_user("Alice".to_string(), 1000)?;
        let bob = service.create_user("Bob".to_string(), 1000)?;

        // create room
        let room = service.create_room()?;
        assert_eq!(room.stage, Stage::NotEnoughPlayers);

        // join players
        let room = service.join_player(room.id, alice.id, 500)?;
        assert_eq!(room.stage, Stage::NotEnoughPlayers);
        let room = service.join_player(room.id, bob.id, 500)?;

        assert_eq!(room.stage, Stage::PreFlop);
        assert_eq!(room.player_in_turn, Some(alice.id));
        assert_eq!(room.community_cards.len(), 0);
        let dealer_count = room
            .players
            .iter()
            .filter(|p| p.position == Position::DealerAndSmallBlind)
            .count();
        let big_blind_count = room
            .players
            .iter()
            .filter(|p| p.position == Position::BigBlind)
            .count();
        assert_eq!(dealer_count, 1);
        assert_eq!(big_blind_count, 1);
        assert_eq!(room.player_in_turn, Some(alice.id));
        let big_blind = room
            .players
            .iter()
            .find(|p| p.position == Position::BigBlind)
            .expect("Big blind not found");
        assert_eq!(big_blind.id, bob.id);
        assert_eq!(big_blind.bet, 2);

        let dealer_and_small_blind = room
            .players
            .iter()
            .find(|p| p.position == Position::DealerAndSmallBlind)
            .expect("Dealer and small blind not found");
        assert_eq!(dealer_and_small_blind.id, alice.id);
        assert_eq!(dealer_and_small_blind.bet, 1);

        // bob takes action, but not bob's turn
        let bob_action_result = service.take_action(room.id, bob.id, Action::Check);
        assert_eq!(
            bob_action_result.unwrap_err().to_string(),
            "Not player's turn".to_string()
        );

        // alice takes invalid actions
        let error_message = service
            .take_action(room.id, alice.id, Action::Check)
            .unwrap_err()
            .to_string();
        assert_eq!(error_message, "Player must call or raise".to_string());
        let error_message = service
            .take_action(room.id, alice.id, Action::Call(2))
            .unwrap_err()
            .to_string();
        assert_eq!(error_message, "Invalid call amount".to_string());

        // alice takes valid action
        let room = service.take_action(room.id, alice.id, Action::Call(1))?;
        assert_eq!(room.player_in_turn, Some(bob.id));
        // bob checks
        let room = service.take_action(room.id, bob.id, Action::Check)?;

        // flop
        assert_eq!(room.stage, Stage::Flop);
        assert_eq!(room.community_cards.len(), 3);
        let alice = room
            .players
            .iter()
            .find(|p| p.id == alice.id)
            .expect("Alice not found");
        let bob = room
            .players
            .iter()
            .find(|p| p.id == bob.id)
            .expect("Bob not found");
        assert!(!alice.has_taken_turn);
        assert!(!bob.has_taken_turn);
        assert_eq!(room.player_in_turn, Some(bob.id));
        let room = service.take_action(room.id, bob.id, Action::Check)?;
        let alice = room
            .players
            .iter()
            .find(|p| p.id == alice.id)
            .expect("Alice not found");
        let bob = room
            .players
            .iter()
            .find(|p| p.id == bob.id)
            .expect("Bob not found");
        assert!(bob.has_taken_turn);
        assert!(!alice.has_taken_turn);
        assert_eq!(room.player_in_turn, Some(alice.id));
        let room = service.take_action(room.id, alice.id, Action::Check)?;

        // turn
        assert_eq!(room.stage, Stage::Turn);
        assert_eq!(room.community_cards.len(), 4);
        let alice = room
            .players
            .iter()
            .find(|p| p.id == alice.id)
            .expect("Alice not found");
        let bob = room
            .players
            .iter()
            .find(|p| p.id == bob.id)
            .expect("Bob not found");
        assert!(!alice.has_taken_turn);
        assert!(!bob.has_taken_turn);
        assert_eq!(room.player_in_turn, Some(bob.id));

        let room = service.take_action(room.id, bob.id, Action::Check)?;
        let room = service.take_action(room.id, alice.id, Action::Check)?;

        // river
        assert_eq!(room.stage, Stage::River);
        assert_eq!(room.community_cards.len(), 5);

        let alice = room
            .players
            .iter()
            .find(|p| p.id == alice.id)
            .expect("Alice not found");
        let bob = room
            .players
            .iter()
            .find(|p| p.id == bob.id)
            .expect("Bob not found");
        assert!(!alice.has_taken_turn);
        assert!(!bob.has_taken_turn);
        assert_eq!(room.player_in_turn, Some(bob.id));

        let room = service.take_action(room.id, bob.id, Action::Check)?;
        let room = service.take_action(room.id, alice.id, Action::Check)?;

        // game restarts to preFlop
        assert_eq!(room.stage, Stage::PreFlop);
        let new_dealer = room.players
            .iter()
            .find(|p| p.position == Position::DealerAndSmallBlind)
            .wrap_err("Dealer and small blind not found")?;
        let new_big_blind = room.players
            .iter()
            .find(|p| p.position == Position::BigBlind)
            .wrap_err("Big blind not found")?;
        assert_eq!(new_dealer.name, "Bob".to_string());
        assert_eq!(new_big_blind.name, "Alice".to_string());
        assert_eq!(new_dealer.bet, 1);
        assert_eq!(new_big_blind.bet, 2);
        assert_eq!(room.community_cards.len(), 0);
        assert!(!new_dealer.has_taken_turn);
        assert!(!new_big_blind.has_taken_turn);
        assert_eq!(room.player_in_turn, Some(new_dealer.id));
        assert_eq!(room.pot, 0);
        Ok(())
    }

    // TODO: test FOLD, RAISE, CALL, ALL_IN
}
