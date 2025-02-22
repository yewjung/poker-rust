use std::cmp::PartialEq;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::mapref::one::RefMut;
use eyre::{bail, ensure, ContextCompat, Result};
use log::error;
use poker::{Eval, Evaluator};
use serde::Serialize;
use socketioxide::socket::Sid;
use socketioxide::SocketIo;
use tap::TapFallible;
use uuid::Uuid;

use types::domain::{Action, RoomInfo, ServiceEvent, User};

use crate::domain::room::{Hand, Player, Room};
use crate::domain::state::{PlayerHand, SharedGameState, Timestamped};
use crate::error::Error;
use crate::repository::rooms::{RoomInfoRepository, RoomRepository};
use crate::repository::users::UserRepository;

#[derive(Clone)]
pub struct GameService {
    pub evaluator: Evaluator,
    pub room_repository: RoomRepository,
    pub room_info_repository: RoomInfoRepository,
    pub user_repository: Arc<UserRepository>,
    pub io: SocketIo,
}

pub struct GameResult {
    pub hands_eval: HashMap<Uuid, Eval>,
    pub winners: Vec<(u32, HashSet<Uuid>)>,
}

impl GameService {
    pub async fn init_rooms(&mut self) -> Result<()> {
        let rooms = self.room_info_repository.get_all().await?;
        for room in rooms {
            self.room_repository.upsert(Room::new_with_id(room.room_id));
        }
        Ok(())
    }

    pub async fn get_rooms(&self) -> Result<Vec<RoomInfo>> {
        self.room_info_repository.get_all().await
    }
    pub fn find_winners(&self, room: &Room) -> Result<GameResult> {
        ensure!(room.is_showdown(), "Game is not in the showdown stage yet");

        let hands_eval = room
            .players_cards()
            .into_iter()
            .map(|(k, v)| Ok((k, self.evaluator.evaluate(v)?)))
            .collect::<Result<HashMap<Uuid, Eval>>>()?;

        let mut winners: Vec<(u32, HashSet<Uuid>)> = Vec::with_capacity(room.pots.len());
        for pot in room.pots.iter().rev() {
            let player_hands: Vec<_> = pot
                .players
                .iter()
                .map(|player_id| {
                    (
                        *player_id,
                        *hands_eval.get(player_id).unwrap_or(&Eval::WORST),
                    )
                })
                .collect();
            let best_hands = Self::all_best_hands(&player_hands);
            winners.push((pot.amount, best_hands));
        }
        Ok(GameResult {
            hands_eval,
            winners,
        })
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

    #[cfg(test)]
    pub fn create_room(&mut self) -> Result<Room> {
        let room = Room::new();
        self.room_repository.upsert(room.clone());
        Ok(room)
    }

    // this function takes action from a player
    pub async fn take_action(
        &self,
        room_id: Uuid,
        player_id: Uuid,
        action: Action,
    ) -> Result<Room> {
        if let Some(mut room) = self.room_repository.get_mut_lock(room_id) {
            let action_required = room.take_action(player_id, action)?;
            self.service_action_required(action_required, room).await?;
        } else {
            bail!(Error::InvalidRoomId);
        }
        self.room_repository
            .get(room_id)
            .wrap_err(Error::InvalidRoomId)
    }

    async fn emit_to_room<T: ?Sized + Serialize>(
        &self,
        room: String,
        event: ServiceEvent,
        data: &T,
    ) {
        if let Some(operator) = self.io.of("/game") {
            let result = operator.to(room).emit(event, data).await;
            if let Err(e) = result {
                error!("Error occurred when emitting to room: {:?}", e);
            }
        }
    }

    fn emit_to_socket<T: ?Sized + Serialize>(&self, sid: Sid, event: ServiceEvent, data: &T) {
        if let Some(operator) = self.io.of("/game") {
            if let Some(socket) = operator.get_socket(sid) {
                let _ = socket.emit(event, data);
            }
        }
    }

    pub async fn join_player(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        buy_in: i64,
        sid: Sid,
    ) -> Result<Room> {
        let (_, tx) = self
            .room_info_repository
            .get_room_for_update(room_id)
            .await
            .tap_err(|e| {
                error!(
                    "error occurred when getting from room_info for room : {:?}. Error: {:?}",
                    room_id, e
                )
            })?;
        let update_result = self
            .update_game_state_and_user(room_id, user_id, buy_in, sid)
            .await;
        let player_count = match update_result {
            Ok(player_count) => player_count,
            Err(e) => {
                tx.rollback().await?;
                error!("error occurred when updating game state and user: {:?}", e);
                return Err(e);
            }
        };
        self.room_info_repository
            .update(room_id, player_count as i32, tx)
            .await?;
        self.room_repository
            .get(room_id)
            .wrap_err(Error::InvalidRoomId)
    }

    async fn update_game_state_and_user(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        buy_in: i64,
        sid: Sid,
    ) -> Result<usize> {
        let mut room = self
            .room_repository
            .get_mut_lock(room_id)
            .wrap_err(Error::InvalidRoomId)?;
        let mut user = self
            .user_repository
            .get(user_id)
            .await?
            .wrap_err("User not found")?;
        ensure!(buy_in <= user.balance, Error::InsufficientBalance);

        let action_required = (*room).join_player(Player::from_user(&user, buy_in as u32, sid))?;
        let player_count = room.player_count();
        self.service_action_required(action_required, room).await?;
        user.balance -= buy_in;
        self.user_repository
            .update_balance_and_room(user_id, user.balance, room_id)
            .await?;
        Ok(player_count)
    }

    pub async fn create_user(&self, name: String, balance: i64) -> Result<User> {
        self.user_repository.create_user(name, balance).await
    }

    pub async fn leave_player(&self, user_id: Uuid) -> Result<()> {
        let room_id = self
            .user_repository
            .get(user_id)
            .await?
            .and_then(|user| user.current_room)
            .wrap_err("User not in any room")?;

        let (_, tx) = self
            .room_info_repository
            .get_room_for_update(room_id)
            .await?;

        let player_count = self.leave_player_and_update_player(user_id, room_id).await;
        let player_count = match player_count {
            Ok(player_count) => player_count,
            Err(e) => {
                tx.rollback().await?;
                return Err(e);
            }
        };

        self.room_info_repository
            .update(room_id, player_count as i32, tx)
            .await?;
        Ok(())
    }

    async fn leave_player_and_update_player(&self, user_id: Uuid, room_id: Uuid) -> Result<usize> {
        let mut room = self
            .room_repository
            .get_mut_lock(room_id)
            .wrap_err(Error::InvalidRoomId)?;
        room.leave_player(user_id);
        self.user_repository
            .update_player_room(user_id, None)
            .await?;
        Ok(room.player_count())
    }

    // this function takes the ServiceRequiredAction enum and perform the corresponding action
    async fn service_action_required(
        &self,
        action: ServiceRequiredAction,
        mut room: RefMut<'_, Uuid, Room>,
    ) -> Result<()> {
        let room_id = room.id.to_string();

        match action {
            ServiceRequiredAction::NoAction => {
                // emit game state
                let game_state = SharedGameState::from_room(room.clone(), false);
                self.emit_to_room(room_id, ServiceEvent::Room, &Timestamped::new(game_state))
                    .await;
                Ok(())
            }
            ServiceRequiredAction::FindWinners => {
                let GameResult {
                    hands_eval,
                    winners,
                } = self.find_winners(&room)?;
                // emit game state
                let game_state =
                    SharedGameState::from_room(room.clone(), true).with_eval(hands_eval);
                self.emit_to_room(room_id, ServiceEvent::Room, &Timestamped::new(game_state))
                    .await;

                room.split_pot(winners)?;
                Box::pin(self.service_action_required(room.proceed()?, room)).await
            }
            ServiceRequiredAction::PlayerReceiveCards => {
                // emit game state
                let game_state = SharedGameState::from_room(room.clone(), false);
                self.emit_to_room(room_id, ServiceEvent::Room, &Timestamped::new(game_state))
                    .await;

                for player in room.players.iter() {
                    if let Some(Hand(cards)) = player.hand {
                        let hand: PlayerHand = cards.into();
                        self.emit_to_socket(
                            player.sid,
                            ServiceEvent::Hand,
                            &Timestamped::new(hand),
                        );
                    }
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ServiceRequiredAction {
    NoAction,
    FindWinners,
    PlayerReceiveCards,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::room::{Position, Stage};
    use eyre::bail;
    use lazy_static::lazy_static;
    use poker::cards;
    use socketioxide::extract::SocketRef;

    lazy_static! {
        static ref users: HashMap<Uuid, User> = HashMap::from([
            (
                Uuid::from_u128(1),
                User {
                    id: Uuid::from_u128(1),
                    name: "Alice".to_string(),
                    balance: 1000,
                    current_room: None,
                }
            ),
            (
                Uuid::from_u128(2),
                User {
                    id: Uuid::from_u128(2),
                    name: "Bob".to_string(),
                    balance: 1000,
                    current_room: None,
                }
            ),
            (
                Uuid::from_u128(3),
                User {
                    id: Uuid::from_u128(3),
                    name: "Charlie".to_string(),
                    balance: 2000,
                    current_room: None,
                }
            ),
            (
                Uuid::from_u128(4),
                User {
                    id: Uuid::from_u128(4),
                    name: "Dennis".to_string(),
                    balance: 2000,
                    current_room: None,
                }
            )
        ]);
    }

    fn mock_user_repository() -> UserRepository {
        let mut user_repository = UserRepository::faux();
        faux::when!(user_repository.create_user).then(|(name, balance)| {
            let mut user = match name.as_str() {
                "Alice" => users.get(&Uuid::from_u128(1)).unwrap().clone(),
                "Bob" => users.get(&Uuid::from_u128(2)).unwrap().clone(),
                "Charlie" => users.get(&Uuid::from_u128(3)).unwrap().clone(),
                "Dennis" => users.get(&Uuid::from_u128(4)).unwrap().clone(),
                _ => bail!("User not found"),
            };
            user.balance = balance;
            Ok(user)
        });

        faux::when!(user_repository.get).then(|id| Ok(users.get(&id).cloned()));

        faux::when!(user_repository.update_balance_and_room).then(|(_, _, _)| Ok(()));
        user_repository
    }

    #[tokio::test]
    #[ignore]
    async fn test_whole_game_flow() -> Result<()> {
        // setup
        let (_, io) = SocketIo::new_layer();
        io.ns("/game", |_: SocketRef| async {});
        let mut service = GameService {
            evaluator: Evaluator::new(),
            room_repository: RoomRepository::new(),
            room_info_repository: RoomInfoRepository::faux(),
            user_repository: Arc::new(mock_user_repository()),
            io,
        };
        let alice = service.create_user("Alice".to_string(), 1000).await?;
        let bob = service.create_user("Bob".to_string(), 1000).await?;

        // create room
        let room = service.create_room()?;
        assert_eq!(room.stage, Stage::NotEnoughPlayers);

        // join players
        let room = service
            .join_player(room.id, alice.id, 500, Sid::default())
            .await?;
        assert_eq!(room.stage, Stage::NotEnoughPlayers);
        let room = service
            .join_player(room.id, bob.id, 500, Sid::default())
            .await?;

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
        let bob_action_result = service.take_action(room.id, bob.id, Action::Check).await;
        assert_eq!(
            bob_action_result.unwrap_err().to_string(),
            "Not player's turn".to_string()
        );

        // alice takes invalid actions
        let error_message = service
            .take_action(room.id, alice.id, Action::Check)
            .await
            .unwrap_err()
            .to_string();
        assert_eq!(error_message, "Player must call or raise".to_string());

        // alice takes valid action
        let room = service.take_action(room.id, alice.id, Action::Call).await?;
        assert_eq!(room.player_in_turn, Some(bob.id));
        // bob checks
        let room = service.take_action(room.id, bob.id, Action::Check).await?;

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
        let room = service.take_action(room.id, bob.id, Action::Check).await?;
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
        let room = service
            .take_action(room.id, alice.id, Action::Raise(10))
            .await?;
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
            .await
            .unwrap_err()
            .to_string();
        assert_eq!(error_message, "Invalid raise amount".to_string());

        // bob calls
        let room = service.take_action(room.id, bob.id, Action::Call).await?;

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

        let room = service.take_action(room.id, bob.id, Action::Check).await?;
        let room = service
            .take_action(room.id, alice.id, Action::Check)
            .await?;

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

        let room = service.take_action(room.id, bob.id, Action::Check).await?;
        let room = service
            .take_action(room.id, alice.id, Action::Check)
            .await?;

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
        assert_eq!(room.pots, vec![]);
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_fold_and_raise() -> Result<()> {
        // setup
        let (_, io) = SocketIo::new_layer();
        io.ns("/game", |_: SocketRef| async {});
        let mut service = GameService {
            evaluator: Evaluator::new(),
            room_repository: RoomRepository::new(),
            room_info_repository: RoomInfoRepository::faux(),
            user_repository: Arc::new(mock_user_repository()),
            io,
        };
        let alice = service.create_user("Alice".to_string(), 1000).await?;
        let bob = service.create_user("Bob".to_string(), 1000).await?;

        // create room
        let room = service.create_room()?;
        assert_eq!(room.stage, Stage::NotEnoughPlayers);

        // join players
        let room = service
            .join_player(room.id, alice.id, 500, Sid::default())
            .await?;
        assert_eq!(room.stage, Stage::NotEnoughPlayers);
        let room = service
            .join_player(room.id, bob.id, 500, Sid::default())
            .await?;
        assert_eq!(room.stage, Stage::PreFlop);

        // alice takes action
        let room = service
            .take_action(room.id, alice.id, Action::Raise(10))
            .await?;
        let room = service
            .take_action(room.id, bob.id, Action::Raise(20))
            .await?;
        let room = service.take_action(room.id, alice.id, Action::Fold).await?;
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

    #[tokio::test]
    #[ignore]
    async fn test_skip_player_with_zero_chips() -> Result<()> {
        // setup
        let (_, io) = SocketIo::new_layer();
        io.ns("/game", |_: SocketRef| async {});
        let mut service = GameService {
            evaluator: Evaluator::new(),
            room_repository: RoomRepository::new(),
            room_info_repository: RoomInfoRepository::faux(),
            user_repository: Arc::new(mock_user_repository()),
            io,
        };
        let alice = service.create_user("Alice".to_string(), 1000).await?;
        let bob = service.create_user("Bob".to_string(), 1000).await?;

        // create room
        let room = service.create_room()?;
        assert_eq!(room.stage, Stage::NotEnoughPlayers);

        // join players
        let room = service
            .join_player(room.id, alice.id, 500, Sid::default())
            .await?;
        assert_eq!(room.stage, Stage::NotEnoughPlayers);
        let room = service
            .join_player(room.id, bob.id, 1000, Sid::default())
            .await?;
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
        let room = service
            .take_action(room.id, alice.id, Action::AllIn)
            .await?;
        assert_eq!(room.player_in_turn, Some(bob.id));
        let alice = room
            .players
            .iter()
            .find(|p| p.id == alice.id)
            .expect("Alice not found");
        assert_eq!(alice.chips, 0);
        assert_eq!(room.stage, Stage::PreFlop);
        let room = service.take_action(room.id, bob.id, Action::Call).await?;
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

    #[tokio::test]
    #[ignore]
    async fn test_proceed_when_alice_reraise_bob() -> Result<()> {
        // setup
        let (_, io) = SocketIo::new_layer();
        io.ns("/game", |_: SocketRef| async {});
        let mut service = GameService {
            evaluator: Evaluator::new(),
            room_repository: RoomRepository::new(),
            room_info_repository: RoomInfoRepository::faux(),
            user_repository: Arc::new(mock_user_repository()),
            io,
        };
        let alice = service.create_user("Alice".to_string(), 1000).await?;
        let bob = service.create_user("Bob".to_string(), 1000).await?;

        // create room
        let room = service.create_room()?;
        assert_eq!(room.stage, Stage::NotEnoughPlayers);

        // join players
        let room = service
            .join_player(room.id, alice.id, 500, Sid::default())
            .await?;
        assert_eq!(room.stage, Stage::NotEnoughPlayers);
        let room = service
            .join_player(room.id, bob.id, 1000, Sid::default())
            .await?;
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
        let room = service
            .take_action(room.id, alice.id, Action::Raise(10))
            .await?;
        assert_eq!(room.player_in_turn, Some(bob.id));
        let room = service
            .take_action(room.id, bob.id, Action::Raise(20))
            .await?;
        assert_eq!(room.player_in_turn, Some(alice.id));
        let room = service
            .take_action(room.id, alice.id, Action::AllIn)
            .await?;
        assert_eq!(room.player_in_turn, Some(bob.id));
        let alice = room
            .players
            .iter()
            .find(|p| p.id == alice.id)
            .expect("Alice not found");
        assert_eq!(alice.chips, 0);
        assert_eq!(room.stage, Stage::PreFlop);
        let room = service.take_action(room.id, bob.id, Action::Call).await?;
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

    #[tokio::test]
    #[ignore]
    async fn test_bob_all_in_during_flop() -> Result<()> {
        // setup
        let (_, io) = SocketIo::new_layer();
        io.ns("/game", |_: SocketRef| async {});
        let mut service = GameService {
            evaluator: Evaluator::new(),
            room_repository: RoomRepository::new(),
            room_info_repository: RoomInfoRepository::faux(),
            user_repository: Arc::new(mock_user_repository()),
            io,
        };
        let alice = service.create_user("Alice".to_string(), 1000).await?;
        let bob = service.create_user("Bob".to_string(), 1000).await?;

        // create room
        let room = service.create_room()?;
        assert_eq!(room.stage, Stage::NotEnoughPlayers);

        // join players
        let room = service
            .join_player(room.id, alice.id, 500, Sid::default())
            .await?;
        assert_eq!(room.stage, Stage::NotEnoughPlayers);
        let room = service
            .join_player(room.id, bob.id, 1000, Sid::default())
            .await?;
        assert_eq!(room.stage, Stage::PreFlop);

        // alice takes action
        let room = service.take_action(room.id, alice.id, Action::Call).await?;
        assert_eq!(room.player_in_turn, Some(bob.id));
        let room = service.take_action(room.id, bob.id, Action::AllIn).await?;
        assert_eq!(room.player_in_turn, Some(alice.id));
        let error_message = service
            .take_action(room.id, alice.id, Action::Call)
            .await
            .unwrap_err()
            .to_string();
        assert_eq!(
            error_message,
            "Alice does not have enough chips".to_string()
        );
        let room = service
            .take_action(room.id, alice.id, Action::AllIn)
            .await?;
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

    #[tokio::test]
    #[ignore]
    async fn test_3_players() -> Result<()> {
        // setup
        let (_, io) = SocketIo::new_layer();
        io.ns("/game", |_: SocketRef| async {});
        let mut service = GameService {
            evaluator: Evaluator::new(),
            room_repository: RoomRepository::new(),
            room_info_repository: RoomInfoRepository::faux(),
            user_repository: Arc::new(mock_user_repository()),
            io,
        };
        let alice = service.create_user("Alice".to_string(), 1000).await?;
        let bob = service.create_user("Bob".to_string(), 1000).await?;
        let charlie = service.create_user("Charlie".to_string(), 2000).await?;
        let dennis = service.create_user("Dennis".to_string(), 2000).await?;

        // create room
        let room = service.create_room()?;

        // join players
        let room = service
            .join_player(room.id, alice.id, 500, Sid::default())
            .await?;
        let room = service
            .join_player(room.id, bob.id, 1000, Sid::default())
            .await?;
        let room = service
            .join_player(room.id, charlie.id, 1500, Sid::default())
            .await?;
        let room = service
            .join_player(room.id, dennis.id, 2000, Sid::default())
            .await?;
        assert_eq!(room.player_joining_next_round.len(), 2);

        // preflop
        service.take_action(room.id, alice.id, Action::Call).await?;
        service.take_action(room.id, bob.id, Action::Check).await?;

        // flop
        service.take_action(room.id, bob.id, Action::Check).await?;
        service
            .take_action(room.id, alice.id, Action::Check)
            .await?;

        // turn
        service.take_action(room.id, bob.id, Action::Check).await?;
        service
            .take_action(room.id, alice.id, Action::Check)
            .await?;

        // river
        service.take_action(room.id, bob.id, Action::Check).await?;
        let room = service
            .take_action(room.id, alice.id, Action::Check)
            .await?;

        // game restarts to preFlop
        assert_eq!(room.stage, Stage::PreFlop);
        assert_eq!(room.player_joining_next_round, vec![]);
        assert_eq!(room.players.len(), 4);

        // preflop
        assert_eq!(room.player_in_turn, Some(alice.id));

        service.take_action(room.id, alice.id, Action::Call).await?;
        service.take_action(room.id, bob.id, Action::Call).await?;
        service
            .take_action(room.id, charlie.id, Action::Call)
            .await?;
        let room = service
            .take_action(room.id, dennis.id, Action::Check)
            .await?;
        assert_eq!(room.stage, Stage::Flop);

        // flop
        // print all chips of players
        assert_eq!(room.player_in_turn, Some(charlie.id));
        service
            .take_action(room.id, charlie.id, Action::Raise(510))
            .await?;
        service
            .take_action(room.id, dennis.id, Action::Call)
            .await?;
        service
            .take_action(room.id, alice.id, Action::AllIn)
            .await?;
        let room = service.take_action(room.id, bob.id, Action::Call).await?;

        // turn
        assert_eq!(room.stage, Stage::Turn);
        assert_eq!(room.player_in_turn, Some(charlie.id));
        service
            .take_action(room.id, charlie.id, Action::Raise(510))
            .await?;
        service
            .take_action(room.id, dennis.id, Action::Call)
            .await?;
        let room = service.take_action(room.id, bob.id, Action::AllIn).await?;

        // river
        assert_eq!(room.stage, Stage::River);
        assert_eq!(room.player_in_turn, Some(charlie.id));
        let room = service
            .take_action(room.id, charlie.id, Action::AllIn)
            .await?;
        assert_eq!(room.stage, Stage::River);
        let room = service
            .take_action(room.id, dennis.id, Action::Call)
            .await?;

        // game resets to preFlop or NotEnoughPlayers
        if room.players.len() == 1 {
            assert_eq!(room.stage, Stage::NotEnoughPlayers);
        } else {
            assert_eq!(room.stage, Stage::PreFlop);
        }

        Ok(())
    }
}
