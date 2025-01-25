use std::collections::{HashMap, HashSet};

use eyre::{ensure, Result};
use poker::{Eval, Evaluator};
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
    pub fn find_winners(&self, room: &Room) -> Result<Vec<(u32, HashSet<Uuid>)>> {
        ensure!(room.is_showdown(), "Game is not in the showdown stage yet");

        let hands = room
            .players_cards()
            .into_iter()
            .map(|(k, v)| Ok((k, self.evaluator.evaluate(v)?)))
            .collect::<Result<HashMap<Uuid, Eval>>>()?;

        let mut winners: Vec<(u32, HashSet<Uuid>)> = Vec::with_capacity(room.pots.len());
        for pot in room.pots.iter().rev() {
            let player_hands: Vec<_> = pot
                .players
                .iter()
                .map(|player_id| (*player_id, *hands.get(player_id).unwrap_or(&Eval::WORST)))
                .collect();
            let best_hands = Self::all_best_hands(&player_hands);
            winners.push((pot.amount, best_hands));
        }
        Ok(winners)
    }

    fn all_best_hands(v: &[(Uuid, Eval)]) -> HashSet<Uuid> {
        let mut largest = HashSet::new();
        let mut best_hand = Eval::WORST;
        for (player_id, hand) in v {
            if hand.is_better_than(best_hand) {
                largest.clear();
                largest.insert(*player_id);
                best_hand = *hand;
            } else if hand.is_equal_to(best_hand) {
                largest.insert(*player_id);
            }
        }
        largest
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
    use super::*;
    use crate::domain::room::{Position, Pot, Stage};
    use eyre::ContextCompat;
    use poker::cards;

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

        // alice takes valid action
        let room = service.take_action(room.id, alice.id, Action::Call)?;
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
        let room = service.take_action(room.id, alice.id, Action::Raise(10))?;
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
        assert_eq!(room.stage, Stage::Flop);
        assert_eq!(room.player_in_turn, Some(bob.id));
        // bob takes invalid action by raising insufficient amount
        let error_message = service
            .take_action(room.id, bob.id, Action::Raise(1))
            .unwrap_err()
            .to_string();
        assert_eq!(error_message, "Invalid raise amount".to_string());

        // bob calls
        let room = service.take_action(room.id, bob.id, Action::Call)?;

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
        let new_dealer = room
            .players
            .iter()
            .find(|p| p.position == Position::DealerAndSmallBlind)
            .wrap_err("Dealer and small blind not found")?;
        let new_big_blind = room
            .players
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
        assert_eq!(
            room.pots,
            vec![Pot {
                amount: 0,
                players: HashSet::from([new_dealer.id, new_big_blind.id]),
            }]
        );
        Ok(())
    }

    #[test]
    fn test_fold_and_raise() -> Result<()> {
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

        // alice takes action
        let room = service.take_action(room.id, alice.id, Action::Raise(10))?;
        let room = service.take_action(room.id, bob.id, Action::Raise(20))?;
        let room = service.take_action(room.id, alice.id, Action::Fold)?;
        assert_eq!(room.stage, Stage::PreFlop);
        let bob = room
            .players
            .iter()
            .find(|p| p.id == bob.id)
            .wrap_err("Bob not found")?;
        let alice = room
            .players
            .iter()
            .find(|p| p.id == alice.id)
            .wrap_err("Alice not found")?;
        assert_eq!(bob.chips + bob.bet, 511);
        assert_eq!(alice.chips + alice.bet, 489);

        Ok(())
    }

    #[test]
    fn test_all_best_hands() -> Result<()> {
        let evaluator = Evaluator::new();
        let hands = vec![
            (
                Uuid::from_u128(1),
                evaluator.evaluate(cards!("Ks Js Ts Qs As").try_collect::<Vec<_>>()?)?,
            ),
            (
                Uuid::from_u128(2),
                evaluator.evaluate(cards!("Kh Jh Th Qh Ah").try_collect::<Vec<_>>()?)?,
            ),
            (
                Uuid::from_u128(3),
                evaluator.evaluate(cards!("Ks Kd Kc Qs Qd").try_collect::<Vec<_>>()?)?,
            ),
        ];
        let best_hands = GameService::all_best_hands(&hands);
        assert_eq!(best_hands.len(), 2);
        assert!(best_hands.contains(&Uuid::from_u128(1)));
        assert!(best_hands.contains(&Uuid::from_u128(2)));
        Ok(())
    }

    // TODO: implement split pots

    #[test]
    fn test_skip_player_with_zero_chips() -> Result<()> {
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
        let room = service.join_player(room.id, bob.id, 1000)?;
        assert_eq!(room.stage, Stage::PreFlop);

        // assert initial bets
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
        assert_eq!(alice.bet, 1);
        assert_eq!(bob.bet, 2);
        assert_eq!(alice.chips, 499);
        assert_eq!(bob.chips, 998);

        assert!(!alice.has_taken_turn);
        assert!(!bob.has_taken_turn);

        // alice takes action
        let room = service.take_action(room.id, alice.id, Action::AllIn)?;
        assert_eq!(room.player_in_turn, Some(bob.id));
        let alice = room
            .players
            .iter()
            .find(|p| p.id == alice.id)
            .expect("Alice not found");
        assert_eq!(alice.chips, 0);
        assert_eq!(room.stage, Stage::PreFlop);
        let room = service.take_action(room.id, bob.id, Action::Call)?;
        let bob = room
            .players
            .iter()
            .find(|p| p.id == bob.id)
            .expect("Bob not found");
        if bob.chips == 1500 {
            // bob won, alice loss
            assert_eq!(room.stage, Stage::NotEnoughPlayers);
        } else {
            // bob loss, alice won
            assert_eq!(room.stage, Stage::PreFlop);
            assert_eq!(room.player_in_turn, Some(bob.id));
        }
        Ok(())
    }

    #[test]
    fn test_proceed_when_alice_reraise_bob() -> Result<()> {
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
        let room = service.join_player(room.id, bob.id, 1000)?;
        assert_eq!(room.stage, Stage::PreFlop);

        // assert initial bets
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
        assert_eq!(alice.bet, 1);
        assert_eq!(bob.bet, 2);
        assert_eq!(alice.chips, 499);

        // alice takes action
        let room = service.take_action(room.id, alice.id, Action::Raise(10))?;
        assert_eq!(room.player_in_turn, Some(bob.id));
        let room = service.take_action(room.id, bob.id, Action::Raise(20))?;
        assert_eq!(room.player_in_turn, Some(alice.id));
        let room = service.take_action(room.id, alice.id, Action::AllIn)?;
        assert_eq!(room.player_in_turn, Some(bob.id));
        let alice = room
            .players
            .iter()
            .find(|p| p.id == alice.id)
            .expect("Alice not found");
        assert_eq!(alice.chips, 0);
        assert_eq!(room.stage, Stage::PreFlop);
        let room = service.take_action(room.id, bob.id, Action::Call)?;
        let bob = room
            .players
            .iter()
            .find(|p| p.id == bob.id)
            .expect("Bob not found");
        if bob.chips == 1500 {
            // bob won, alice loss
            assert_eq!(room.stage, Stage::NotEnoughPlayers);
        } else {
            // bob loss, alice won
            assert_eq!(room.stage, Stage::PreFlop);
            assert_eq!(room.player_in_turn, Some(bob.id));
        }
        Ok(())
    }

    #[test]
    fn test_bob_all_in_during_flop() -> Result<()> {
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
        let room = service.join_player(room.id, bob.id, 1000)?;
        assert_eq!(room.stage, Stage::PreFlop);

        // alice takes action
        let room = service.take_action(room.id, alice.id, Action::Call)?;
        assert_eq!(room.player_in_turn, Some(bob.id));
        let room = service.take_action(room.id, bob.id, Action::AllIn)?;
        assert_eq!(room.player_in_turn, Some(alice.id));
        let error_message = service
            .take_action(room.id, alice.id, Action::Call)
            .unwrap_err()
            .to_string();
        assert_eq!(
            error_message,
            "Alice does not have enough chips".to_string()
        );
        let room = service.take_action(room.id, alice.id, Action::AllIn)?;
        if room.players.iter().any(|p| p.chips == 1500) {
            // someone won
            assert_eq!(room.players.len(), 1);
            assert_eq!(room.stage, Stage::NotEnoughPlayers);
        } else {
            // draw
            assert_eq!(room.stage, Stage::PreFlop);
            assert_eq!(room.player_in_turn, Some(bob.id));
        }

        Ok(())
    }

    #[test]
    fn test_3_players() -> Result<()> {
        // setup
        let mut service = GameService {
            evaluator: Evaluator::new(),
            room_repository: RoomRepository::new(),
            user_repository: UserRepository::new(),
        };
        let alice = service.create_user("Alice".to_string(), 1000)?;
        let bob = service.create_user("Bob".to_string(), 1000)?;
        let charlie = service.create_user("Charlie".to_string(), 1000)?;

        // create room
        let room = service.create_room()?;

        // join players
        let room = service.join_player(room.id, alice.id, 500)?;
        let room = service.join_player(room.id, bob.id, 1000)?;
        let room = service.join_player(room.id, charlie.id, 1000)?;
        assert_eq!(room.player_joining_next_round.len(), 1);

        // preflop
        service.take_action(room.id, alice.id, Action::Call)?;
        service.take_action(room.id, bob.id, Action::Check)?;

        // flop
        service.take_action(room.id, bob.id, Action::Check)?;
        service.take_action(room.id, alice.id, Action::Check)?;

        // turn
        service.take_action(room.id, bob.id, Action::Check)?;
        service.take_action(room.id, alice.id, Action::Check)?;

        // river
        service.take_action(room.id, bob.id, Action::Check)?;
        let room = service.take_action(room.id, alice.id, Action::Check)?;

        // game restarts to preFlop
        assert_eq!(room.stage, Stage::PreFlop);
        assert_eq!(room.player_joining_next_round, vec![]);
        assert_eq!(room.players.len(), 3);

        Ok(())
    }

    // TODO: pot chopping, give remainder to the player who started first

    // TODO: Flop should only burn one cards
}
