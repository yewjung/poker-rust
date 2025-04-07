use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use dashmap::mapref::one::RefMut;
use eyre::{bail, ensure, ContextCompat, Result};
use itertools::Itertools;
use log::{error, info};
use poker::{Eval, Evaluator};
use serde::Serialize;
use socketioxide::socket::Sid;
use socketioxide::SocketIo;
use tap::TapFallible;
use tokio::time::sleep;
use uuid::Uuid;

use types::domain::{Action, RoomInfo, ServiceEvent, ServiceRequiredAction};
use types::error::Error;
use types::room::{Hand, Player, Room, Stage};
use types::state::{PlayerHand, SharedGameState, Timestamped};

use crate::repository::rooms::{RoomInfoRepository, RoomRepository};
use crate::repository::users::UserRepository;

#[cfg(test)]
use types::domain::User;

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
        ensure!(
            room.stage.is_showdown(),
            "Game is not in the showdown stage yet"
        );
        // in special case where stage is Showdown(false), evaluation is not needed, winner should be the last man standing
        if matches!(room.stage, Stage::Showdown(false)) {
            let sole_player = room
                .players
                .iter()
                .find_or_first(|p| !p.has_folded)
                .wrap_err("No player left in the game")?;
            let total_pot = room.pots.iter().map(|pot| pot.amount).sum();
            return Ok(GameResult {
                hands_eval: Default::default(),
                winners: vec![(total_pot, HashSet::from([sole_player.id]))],
            });
        }
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

    fn join_player_to_ws_room(&self, room_id: Uuid, sid: Sid) {
        if let Some(operator) = self.io.of("/game") {
            if let Some(socket) = operator.get_socket(sid) {
                socket.leave_all();
                socket.join(room_id.to_string());
            }
        }
    }

    pub fn disconnect_socket(&self, sid: Sid) -> Result<()> {
        if let Some(operator) = self.io.of("/game") {
            if let Some(socket) = operator.get_socket(sid) {
                socket.disconnect()?
            }
        }
        Ok(())
    }

    fn remove_player_from_ws_room(&self, room_id: Uuid, sid: Sid) {
        if let Some(operator) = self.io.of("/game") {
            if let Some(socket) = operator.get_socket(sid) {
                socket.leave(room_id.to_string());
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

        let action_required = room.join_player(Player::from_user(&user, buy_in as u32, sid))?;
        let player_count = room.player_count();
        self.join_player_to_ws_room(room_id, sid);
        self.service_action_required(action_required, room).await?;
        user.balance -= buy_in;
        self.user_repository
            .update_balance_and_room(user_id, user.balance, room_id)
            .await?;
        Ok(player_count)
    }

    #[cfg(test)]
    pub async fn create_user(&self, name: String, balance: i64) -> Result<User> {
        self.user_repository.create_user(name, balance).await
    }

    pub async fn leave_player(&self, user_id: Uuid, sid: Sid) -> Result<()> {
        let room_id = self
            .user_repository
            .get(user_id)
            .await?
            .and_then(|user| user.current_room);
        let room_id = match room_id {
            Some(room_id) => room_id,
            None => {
                info!("User {} not in any room", user_id);
                return Ok(());
            }
        };
        let (_, tx) = self
            .room_info_repository
            .get_room_for_update(room_id)
            .await?;

        let player_count = self
            .leave_player_and_update_player(user_id, room_id, sid)
            .await;
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

    async fn leave_player_and_update_player(
        &self,
        user_id: Uuid,
        room_id: Uuid,
        sid: Sid,
    ) -> Result<usize> {
        let mut room = self
            .room_repository
            .get_mut_lock(room_id)
            .wrap_err(Error::InvalidRoomId)?;
        let player_chips = room.leave_player(user_id);
        self.user_repository
            .remove_player_and_reimburse_chips(user_id, player_chips as i64)
            .await?;
        let player_count = room.player_count();
        self.remove_player_from_ws_room(room_id, sid);
        self.service_action_required(ServiceRequiredAction::NoAction, room)
            .await?;
        Ok(player_count)
    }

    // this function takes the ServiceRequiredAction enum and perform the corresponding action
    async fn service_action_required(
        &self,
        action: ServiceRequiredAction,
        mut room: RefMut<'_, Uuid, Room>,
    ) -> Result<()> {
        let room_id = &room.id.to_string();

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
                self.emit_to_room(
                    room_id.clone(),
                    ServiceEvent::Room,
                    &Timestamped::new(game_state),
                )
                .await;
                // sleep for 5 seconds to show the result
                sleep(Duration::from_secs(5)).await;

                let mut pot_splits = room.split_pot(winners)?;
                // reversing the winnings because the last item is the last pot
                pot_splits.reverse();
                // emit winnings
                for winnings in pot_splits {
                    self.emit_to_room(
                        room_id.clone(),
                        ServiceEvent::Outcome,
                        &Timestamped::new(winnings),
                    )
                    .await;
                    sleep(Duration::from_secs(3)).await;
                }
                self.emit_to_room(
                    room_id.clone(),
                    ServiceEvent::Outcome,
                    &Timestamped::new(Vec::new()),
                )
                .await;

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

#[cfg(test)]
mod tests {
    use super::*;
    use eyre::bail;
    use lazy_static::lazy_static;
    use poker::{card, cards};
    use socketioxide::extract::SocketRef;
    use std::str::FromStr;
    use types::deck::Deck;
    use types::room::{Position, Pot, ProceedType, Stage};

    use types::domain::User;

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

    #[test]
    fn test_add_player() -> Result<()> {
        let mut room = Room::new();
        room.join_player(Player::new("Alice".to_string(), 400))?;
        room.join_player(Player::new("Bob".to_string(), 400))?;
        assert_eq!(2, room.players.len());
        Ok(())
    }

    #[test]
    fn test_deal_community_card() -> Result<()> {
        let mut room = Room::new();
        room.deal_community_card(Stage::Flop)?;
        room.deal_community_card(Stage::Turn)?;
        room.deal_community_card(Stage::River)?;
        assert_eq!(room.community_cards.len(), 5);
        Ok(())
    }

    #[test]
    fn test_winners() -> Result<()> {
        let (_, io) = SocketIo::new_layer();
        let game_service = GameService {
            evaluator: Evaluator::new(),
            room_repository: RoomRepository::new(),
            room_info_repository: RoomInfoRepository::faux(),
            user_repository: Arc::new(UserRepository::faux()),
            io,
        };
        let room = Room {
            id: Uuid::new_v4(),
            players: vec![
                Player {
                    id: Uuid::from_u128(1),
                    name: "Alice".to_string(),
                    hand: Some(Hand([card!("3s")?, card!("2s")?])),
                    chips: 0,
                    bet: 0,
                    has_folded: false,
                    position: Position::DealerAndSmallBlind,
                    has_taken_turn: true,
                    sid: Sid::from_str("AA9AAA0AAzAAAAHs")?,
                    is_connected: true,
                    last_action: None,
                },
                Player {
                    id: Uuid::from_u128(2),
                    name: "Bob".to_string(),
                    hand: Some(Hand([card!("4s")?, card!("5s")?])),
                    chips: 0,
                    bet: 0,
                    has_folded: false,
                    position: Position::BigBlind,
                    has_taken_turn: true,
                    sid: Sid::from_str("AA9AAA0AAzAAAAHB")?,
                    is_connected: true,
                    last_action: None,
                },
            ],
            deck: Deck::new(),
            community_cards: cards!("6s 7s 8s 9s Ts").try_collect()?,
            stage: Stage::Showdown(true),
            pots: vec![Pot {
                amount: 0,
                players: HashSet::from([Uuid::from_u128(1), Uuid::from_u128(2)]),
            }],
            player_joining_next_round: Default::default(),
            player_in_turn: None,
        };

        let game_result = game_service.find_winners(&room)?;
        let (_, winner_ids) = game_result.winners.first().wrap_err("No winners")?;
        let mut winners: Vec<_> = room
            .players
            .iter()
            .filter(|p| winner_ids.contains(&p.id))
            .collect();
        winners.sort_by(|a, b| a.id.cmp(&b.id));
        // Both players have straight flushes
        assert_eq!(
            winners,
            vec![
                &Player {
                    id: Uuid::from_u128(1),
                    name: "Alice".to_string(),
                    hand: Some(Hand([card!("3s")?, card!("2s")?])),
                    chips: 0,
                    bet: 0,
                    has_folded: false,
                    position: Position::DealerAndSmallBlind,
                    has_taken_turn: true,
                    sid: Sid::from_str("AA9AAA0AAzAAAAHs")?,
                    is_connected: true,
                    last_action: None,
                },
                &Player {
                    id: Uuid::from_u128(2),
                    name: "Bob".to_string(),
                    hand: Some(Hand([card!("4s")?, card!("5s")?])),
                    chips: 0,
                    bet: 0,
                    has_folded: false,
                    position: Position::BigBlind,
                    has_taken_turn: true,
                    sid: Sid::from_str("AA9AAA0AAzAAAAHB")?,
                    is_connected: true,
                    last_action: None,
                },
            ]
        );
        Ok(())
    }

    #[test]
    fn stage_should_change_to_pre_flop_when_there_is_minimum_players() -> Result<()> {
        let mut room = Room::new();
        room.join_player(Player::new("Alice".to_string(), 400))?;
        room.join_player(Player::new("Bob".to_string(), 400))?;
        assert_eq!(room.stage, Stage::PreFlop);
        Ok(())
    }

    #[test]
    fn test_readjust_positions_with_4_players() -> Result<()> {
        let mut room = Room::new();
        room.join_player(Player::new("Alice".to_string(), 400))?;
        room.join_player(Player::new("Bob".to_string(), 400))?;
        room.join_player(Player::new("Charlie".to_string(), 400))?;
        room.join_player(Player::new("David".to_string(), 400))?;

        let positions = room
            .players
            .iter()
            .map(|p| p.position.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            positions,
            vec![Position::DealerAndSmallBlind, Position::BigBlind]
        );
        Ok(())
    }

    #[test]
    fn test_readjust_positions_with_2_players() -> Result<()> {
        let mut room = Room::new();
        room.join_player(Player::new("Alice".to_string(), 400))?;
        room.join_player(Player::new("Bob".to_string(), 400))?;
        let positions = room
            .players
            .iter()
            .map(|p| p.position.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            positions,
            vec![Position::DealerAndSmallBlind, Position::BigBlind]
        );
        Ok(())
    }

    struct PlayerState {
        bet: u32,
        has_taken_turn: bool,
        has_folded: bool,
    }

    #[rstest::rstest]
    #[case(
        PlayerState { bet: 0, has_taken_turn: true, has_folded: true },
        PlayerState { bet: 0, has_taken_turn: false, has_folded: false },
        "Alice",
        ProceedType::ShowdownWithoutDealing
        // true
    )]
    #[case(
        PlayerState { bet: 0, has_taken_turn: false, has_folded: false },
        PlayerState { bet: 0, has_taken_turn: false, has_folded: false },
        "Bob",
        ProceedType::NoAction
        // false
    )]
    #[case(
        PlayerState { bet: 0, has_taken_turn: false, has_folded: false },
        PlayerState { bet: 0, has_taken_turn: false, has_folded: false },
        "Alice",
        ProceedType::NoAction
        // false
    )]
    #[case(
        PlayerState { bet: 10, has_taken_turn: true, has_folded: false },
        PlayerState { bet: 10, has_taken_turn: true, has_folded: false },
        "Bob",
        ProceedType::Normal // all bets are equal, can proceed to next stage
    )]
    #[case(
        PlayerState { bet: 10, has_taken_turn: true, has_folded: false },
        PlayerState { bet: 0, has_taken_turn: false, has_folded: false },
        "Alice",
        ProceedType::NoAction // it is Bob's turn to match alice's bet
    )]
    #[case(
        PlayerState { bet: 10, has_taken_turn: true, has_folded: false },
        PlayerState { bet: 5, has_taken_turn: true, has_folded: false },
        "Alice",
        ProceedType::NoAction // because it is Bob's turn to match alice's bet
    )]
    #[case(
        PlayerState { bet: 15, has_taken_turn: true, has_folded: false },
        PlayerState { bet: 10, has_taken_turn: true, has_folded: true },
        "Bob",
        ProceedType::ShowdownWithoutDealing // true because bob has folded, leaving Alice the only player left
    )]
    #[case(
        PlayerState { bet: 500, has_taken_turn: true, has_folded: false },
        PlayerState { bet: 1000, has_taken_turn: true, has_folded: false },
        "Bob",
        ProceedType::ShowdownWithDealing // true because both players all-in, can proceed to next stage
    )]
    #[case(
        // alice all in
        PlayerState { bet: 500, has_taken_turn: true, has_folded: false },
        // bob all in
        PlayerState { bet: 1000, has_taken_turn: true, has_folded: false },
        "Alice",
        ProceedType::ShowdownWithDealing // true because both players all-in, can proceed to next stage
    )]
    #[case(
        // alice all in
        PlayerState { bet: 500, has_taken_turn: true, has_folded: false },
        PlayerState { bet: 22, has_taken_turn: true, has_folded: false },
        "Alice",
        ProceedType::NoAction // false because it is Bob's turn to match Alice's bet
    )]
    fn test_can_proceed_to_next_stage(
        #[case] PlayerState {
            bet: alice_bet,
            has_taken_turn: alice_has_taken_turn,
            has_folded: alice_has_folded,
        }: PlayerState,
        #[case] PlayerState {
            bet: bob_bet,
            has_taken_turn: bob_has_taken_turn,
            has_folded: bob_has_folded,
        }: PlayerState,
        #[case] player_in_turn: &str,
        #[case] proceed_type: ProceedType,
    ) -> Result<()> {
        let room = Room {
            id: Uuid::new_v4(),
            players: vec![
                Player {
                    id: Uuid::from_u128(1),
                    name: "Alice".to_string(),
                    hand: Some(Hand([card!("3s")?, card!("2s")?])),
                    chips: 500 - alice_bet,
                    bet: alice_bet,
                    has_folded: alice_has_folded,
                    position: Position::DealerAndSmallBlind,
                    has_taken_turn: alice_has_taken_turn,
                    sid: Sid::default(),
                    is_connected: true,
                    last_action: None,
                },
                Player {
                    id: Uuid::from_u128(2),
                    name: "Bob".to_string(),
                    hand: Some(Hand([card!("4s")?, card!("5s")?])),
                    chips: 1000 - bob_bet,
                    bet: bob_bet,
                    has_folded: bob_has_folded,
                    position: Position::BigBlind,
                    has_taken_turn: bob_has_taken_turn,
                    sid: Sid::default(),
                    is_connected: true,
                    last_action: None,
                },
            ],
            deck: Deck::new(),
            community_cards: Vec::new(),
            stage: Stage::PreFlop,
            pots: vec![Pot {
                amount: 0,
                players: HashSet::from([Uuid::from_u128(1), Uuid::from_u128(2)]),
            }],
            player_joining_next_round: Vec::new(),
            player_in_turn: if player_in_turn == "Alice" {
                Some(Uuid::from_u128(1))
            } else {
                Some(Uuid::from_u128(2))
            },
        };
        assert_eq!(room.can_proceed_to_next_stage(), proceed_type);
        Ok(())
    }

    #[test]
    fn test_side_pots() -> Result<()> {
        let mut room = Room::new();
        let alice = Player::new("Alice".to_string(), 500);
        let bob = Player::new("Bob".to_string(), 1000);
        let charlie = Player::new("Charlie".to_string(), 1500);
        let david = Player::new("David".to_string(), 2000);
        let alice_id = alice.id;
        let bob_id = bob.id;
        let charlie_id = charlie.id;
        let david_id = david.id;

        room.players.push(alice);
        room.players.push(bob);
        room.players.push(charlie);
        room.players.push(david);

        room.proceed()?;
        // PreFlop
        assert_eq!(room.stage, Stage::PreFlop);
        assert_eq!(room.player_in_turn, Some(david_id));

        room.take_action(david_id, Action::Raise(500))?;
        room.take_action(alice_id, Action::Call)?;
        room.take_action(bob_id, Action::Call)?;
        room.take_action(charlie_id, Action::Call)?;

        assert_eq!(
            room.pots,
            vec![Pot {
                amount: 2000,
                players: HashSet::from([alice_id, bob_id, charlie_id, david_id]),
            }]
        );

        // Flop
        assert_eq!(room.stage, Stage::Flop);
        assert_eq!(room.player_in_turn, Some(bob_id));

        room.take_action(bob_id, Action::Raise(500))?;
        room.take_action(charlie_id, Action::Call)?;
        room.take_action(david_id, Action::Call)?;

        // Turn
        assert_eq!(
            room.pots,
            vec![
                Pot {
                    amount: 2000,
                    players: HashSet::from([alice_id, bob_id, charlie_id, david_id]),
                },
                Pot {
                    amount: 1500,
                    players: HashSet::from([bob_id, charlie_id, david_id]),
                },
            ]
        );
        assert_eq!(room.stage, Stage::Turn);
        assert_eq!(room.player_in_turn, Some(charlie_id));

        room.take_action(charlie_id, Action::Raise(500))?;
        room.take_action(david_id, Action::Call)?;

        // Game skips to Showdown resets to PreFlop
        assert_eq!(room.stage, Stage::Showdown(true));
        assert_eq!(
            room.pots,
            vec![
                Pot {
                    amount: 2000,
                    players: HashSet::from([alice_id, bob_id, charlie_id, david_id]),
                },
                Pot {
                    amount: 1500,
                    players: HashSet::from([bob_id, charlie_id, david_id]),
                },
                Pot {
                    amount: 1000,
                    players: HashSet::from([charlie_id, david_id]),
                },
            ]
        );
        Ok(())
    }

    #[test]
    fn closest_to_dealer_should_return_correct_player() -> Result<()> {
        let mut room = Room::new();
        let player1 = Player::new("Alice".to_string(), 400);
        let player2 = Player::new("Bob".to_string(), 400);
        let player3 = Player::new("Charlie".to_string(), 400);
        let player4 = Player::new("David".to_string(), 400);

        room.join_player(player1.clone())?;
        room.join_player(player2.clone())?;
        room.join_player(player3.clone())?;
        room.join_player(player4.clone())?;
        room.proceed()?;

        let player_ids = HashSet::from([player1.id, player2.id, player3.id, player4.id]);

        let closest_player = room.closest_to_dealer(&player_ids)?;
        assert_eq!(closest_player, player2.id);
        Ok(())
    }

    #[test]
    fn closest_to_dealer_should_return_error_if_no_dealer() -> Result<()> {
        let mut room = Room::new();
        let player1 = Player::new("Alice".to_string(), 400);
        let player2 = Player::new("Bob".to_string(), 400);

        room.join_player(player1.clone())?;
        room.join_player(player2.clone())?;
        room.proceed()?;

        let player_ids = HashSet::from([player1.id, player2.id]);

        let closest_player = room.closest_to_dealer(&player_ids)?;
        assert_eq!(closest_player, player2.id);
        Ok(())
    }
}
