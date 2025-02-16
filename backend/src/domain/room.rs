use std::collections::HashSet;

use eyre::{bail, ensure, ContextCompat, Report, Result};
use poker::{box_cards, Card};
use serde::{Deserialize, Serialize};
use socketioxide::socket::Sid;
use sqlx::{FromRow, Type};
use uuid::Uuid;

use crate::domain::deck::Deck;
use crate::domain::user::User;
use crate::error::Error;
use crate::service::game::ServiceRequiredAction;

#[derive(Debug, Clone, FromRow)]
pub struct Room {
    pub id: Uuid,
    pub players: Vec<Player>,
    pub deck: Deck,
    pub community_cards: Vec<Card>,
    pub stage: Stage,
    pub pots: Vec<Pot>,
    pub player_joining_next_round: Vec<Player>,
    pub player_leaving_next_round: HashSet<Uuid>,
    pub player_in_turn: Option<Uuid>,
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Player {
    pub id: Uuid,
    pub name: String,
    pub hand: Option<Hand>,
    pub chips: u32,
    pub bet: u32,
    pub has_folded: bool,
    pub position: Position,
    pub has_taken_turn: bool,
    pub sid: Sid,
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Pot {
    pub amount: u32,
    pub players: HashSet<Uuid>,
}

impl Player {
    pub fn new(name: String, buy_in: u32) -> Self {
        Player {
            id: Uuid::new_v4(),
            name,
            hand: None,
            chips: buy_in,
            bet: 0,
            has_folded: false,
            position: Position::Normal,
            has_taken_turn: false,
            sid: Sid::default(),
        }
    }

    fn bet_amount(&mut self, amount: u32) -> Result<()> {
        ensure!(
            amount <= self.chips,
            "{} does not have enough chips",
            self.name
        );
        self.bet += amount;
        self.chips -= amount;
        Ok(())
    }

    pub fn from_user(user: &User, buy_in: u32, sid: Sid) -> Self {
        Player {
            id: user.id,
            name: user.name.clone(),
            hand: None,
            chips: buy_in,
            bet: 0,
            has_folded: false,
            position: Position::Normal,
            has_taken_turn: false,
            sid,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Type, Serialize, Deserialize)]
#[sqlx(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Stage {
    NotEnoughPlayers,
    PreFlop,
    Flop,
    Turn,
    River,
    Showdown,
}

#[derive(Debug, Eq, Hash, PartialEq, Clone, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Position {
    Normal,
    BigBlind,
    SmallBlind,
    DealerAndSmallBlind,
    Dealer,
}

pub enum Action {
    Fold,
    Check,
    Call,
    Raise(u32),
    AllIn,
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Hand(pub [Card; 2]);

impl Room {
    pub fn new() -> Self {
        Room {
            id: Uuid::new_v4(),
            players: Vec::new(),
            deck: Deck::new(),
            community_cards: Vec::with_capacity(5),
            stage: Stage::NotEnoughPlayers,
            pots: vec![],
            player_joining_next_round: Vec::new(),
            player_leaving_next_round: Default::default(),
            player_in_turn: None,
        }
    }

    fn readjust_positions(&mut self, dealer_position: usize) -> Result<()> {
        let total_players = self.players.len();
        (dealer_position..)
            .take(total_players)
            .enumerate()
            .try_for_each(|(i, pos)| {
                let p = self
                    .players
                    .get_mut(pos % total_players)
                    .wrap_err("Player not found")?;
                if total_players > 2 {
                    p.position = match i {
                        0 => Position::Dealer,
                        1 => Position::SmallBlind,
                        2 => Position::BigBlind,
                        _ => Position::Normal,
                    }
                } else {
                    p.position = match i {
                        0 => Position::DealerAndSmallBlind,
                        1 => Position::BigBlind,
                        _ => unreachable!("Only maximum two players in the room"),
                    }
                }
                Ok(())
            })
    }

    pub fn join_player(&mut self, player: Player) -> Result<ServiceRequiredAction> {
        if !self.is_joinable() {
            bail!(Error::RoomIsFull);
        }
        match self.stage {
            Stage::NotEnoughPlayers => {
                self.players.push(player);
                self.proceed()
            }
            _ => {
                self.player_joining_next_round.push(player);
                Ok(ServiceRequiredAction::NoAction)
            }
        }
    }

    pub fn leave_player(&mut self, player_id: Uuid) {
        if self.players.iter().any(|p| p.id == player_id) {
            self.player_leaving_next_round.insert(player_id);
        }
        self.player_joining_next_round.retain(|p| p.id != player_id);
    }

    fn is_joinable(&self) -> bool {
        let count = self
            .players
            .iter()
            .chain(self.player_joining_next_round.iter())
            .filter(|p| p.chips > 0)
            .count();
        count - self.player_leaving_next_round.len() < 10
    }

    pub fn start_game(&mut self) -> Result<()> {
        self.reset_table();
        // Reset the bets
        self.players.iter_mut().try_for_each(|p| {
            p.bet = 0;
            p.has_folded = false;
            p.has_taken_turn = false;
            p.hand = Some(Hand([self.deck.draw()?, self.deck.draw()?]));
            Ok::<(), Report>(())
        })?;

        // find the next dealer
        let dealer_seat = self
            .players
            .iter()
            .enumerate()
            .rev()
            .max_by(|a, b| a.1.position.cmp(&b.1.position))
            .map(|(i, _)| i)
            .wrap_err("No players")?;

        let dealer = self.players.get(dealer_seat).wrap_err("Dealer not found")?;
        let next_dealer_seat = match dealer.position {
            Position::Dealer | Position::DealerAndSmallBlind => {
                (dealer_seat + 1) % self.players.len()
            }
            _ => dealer_seat,
        };
        self.readjust_positions(next_dealer_seat)?;

        self.apply_binds()?;

        self.player_in_turn = Some(self.player_to_act_first()?);
        Ok(())
    }

    fn apply_binds(&mut self) -> Result<()> {
        self.players.iter_mut().try_for_each(|p| match p.position {
            Position::BigBlind => p.bet_amount(2),
            Position::SmallBlind | Position::DealerAndSmallBlind => p.bet_amount(1),
            _ => Ok(()),
        })
    }

    fn reset_table(&mut self) {
        self.seat_players();

        self.pots = vec![];
        // Reset the community cards
        self.community_cards.clear();
        // Reset the deck
        self.deck = Deck::new();
    }

    fn seat_players(&mut self) {
        // Remove players who left the game
        self.players
            .retain(|p| !self.player_leaving_next_round.contains(&p.id));
        // Remove players who have no chips
        self.players.retain(|p| p.chips > 0);
        self.player_leaving_next_round.clear();
        // Add players who joined the game
        self.players.append(&mut self.player_joining_next_round);
    }

    fn player_to_act_first(&self) -> Result<Uuid> {
        let index_of_last_player_to_take_turn = match self.stage {
            Stage::NotEnoughPlayers => unreachable!("Impossible to reach this state"),
            Stage::PreFlop => self
                .players
                .iter()
                .enumerate()
                .find(|(_, p)| p.position == Position::BigBlind)
                .map(|(index, _)| index)
                .wrap_err("Big blind not found")?,
            _ => self
                .players
                .iter()
                .enumerate()
                .find(|(_, p)| {
                    p.position == Position::Dealer || p.position == Position::DealerAndSmallBlind
                })
                .map(|(index, _)| index)
                .wrap_err("Dealer not found")?,
        };
        self.next_player_after(index_of_last_player_to_take_turn)
    }

    fn next_player_after(&self, curr_player_index: usize) -> Result<Uuid> {
        let next_index = (curr_player_index + 1) % self.players.len();
        let end_index = next_index + self.players.len();
        let next_player_index = (next_index..end_index)
            .map(|index| index % self.players.len())
            .find(|index| {
                self.players
                    .get(*index)
                    .is_some_and(|p| !p.has_folded && p.chips > 0)
            })
            .wrap_err("No players to act")?;

        self.players
            .get(next_player_index)
            .map(|p| p.id)
            .wrap_err("Player not found")
    }

    fn deal_community_card(&mut self, stage: Stage) -> Result<()> {
        match stage {
            Stage::Flop => {
                self.deck.draw()?;
                self.community_cards.push(self.deck.draw()?);
                self.community_cards.push(self.deck.draw()?);
                self.community_cards.push(self.deck.draw()?);
            }
            Stage::Turn | Stage::River => {
                self.deck.draw()?;
                self.community_cards.push(self.deck.draw()?);
            }
            _ => bail!("Invalid stage to deal community card"),
        }
        Ok(())
    }

    pub fn players_cards(&self) -> Vec<(Uuid, Box<[Card]>)> {
        self.players
            .iter()
            .filter(|p| !p.has_folded)
            .map(|p| {
                let cards = p
                    .hand
                    .as_ref()
                    .map(|h| box_cards!(h.0, self.community_cards))
                    .unwrap_or(box_cards!(self.community_cards, []));
                (p.id, cards)
            })
            .collect()
    }

    pub fn is_showdown(&self) -> bool {
        self.stage == Stage::Showdown && self.community_cards.len() == 5
    }

    pub fn split_pot(&mut self, winners: Vec<(u32, HashSet<Uuid>)>) -> Result<()> {
        for (amount, winner_ids) in winners {
            let earnings = amount / winner_ids.len() as u32;

            self.players.iter_mut().for_each(|p| {
                if winner_ids.contains(&p.id) {
                    p.chips += earnings;
                }
            });
            let remainder = amount % winner_ids.len() as u32;
            let remainder_winner = self.closest_to_dealer(&winner_ids)?;
            let remainder_winner = self
                .players
                .iter_mut()
                .find(|p| p.id == remainder_winner)
                .wrap_err("Remainder winner not found")?;
            remainder_winner.chips += remainder;
        }
        Ok(())
    }

    fn closest_to_dealer(&self, player_ids: &HashSet<Uuid>) -> Result<Uuid> {
        let dealer_index = self
            .players
            .iter()
            .position(|p| {
                p.position == Position::Dealer || p.position == Position::DealerAndSmallBlind
            })
            .wrap_err("Dealer not found")?;
        let next_to_dealer = (dealer_index + 1) % self.players.len();

        let end_index = next_to_dealer + self.players.len();
        (next_to_dealer..end_index)
            .map(|index| index % self.players.len())
            .find_map(|index| {
                let p_id = self.players[index].id;
                if player_ids.contains(&p_id) {
                    Some(p_id)
                } else {
                    None
                }
            })
            .wrap_err("No players found")
    }

    /// Check if all non-folded players have the same bet
    fn can_proceed_to_next_stage(&self) -> bool {
        match self.stage {
            Stage::NotEnoughPlayers => self.players.len() >= 2,
            Stage::Showdown => true,
            _ => {
                if self
                    .players
                    .iter()
                    .filter(|p| !p.has_folded && p.chips > 0)
                    .any(|p| !p.has_taken_turn)
                {
                    return false;
                }
                let max_bet = self.players.iter().map(|p| p.bet).max().unwrap_or_default();
                !self
                    .players
                    .iter()
                    .filter(|p| self.player_in_turn.is_some_and(|q| q != p.id))
                    .any(|p| p.chips > 0 && p.bet < max_bet && !p.has_folded)
            }
        }
    }

    pub fn take_action(
        &mut self,
        player_id: Uuid,
        action: Action,
    ) -> Result<ServiceRequiredAction> {
        ensure!(self.player_in_turn == Some(player_id), "Not player's turn");
        let max_bet = self
            .players
            .iter()
            .filter(|p| !p.has_folded)
            .map(|p| p.bet)
            .max()
            .unwrap_or_default();
        let player = self
            .players
            .iter_mut()
            .find(|p| p.id == player_id)
            .wrap_err("Player not found")?;
        match action {
            Action::Fold => player.has_folded = true,
            Action::Check => {
                if player.bet < max_bet {
                    bail!("Player must call or raise");
                }
            }
            Action::Call => {
                let call_amount = max_bet - player.bet;
                player.bet_amount(call_amount)?;
            }
            Action::Raise(amount) => {
                ensure!(amount + player.bet >= max_bet, "Invalid raise amount");
                player.bet_amount(amount)?;
            }
            Action::AllIn => {
                let all_in_amount = player.chips;
                player.bet_amount(all_in_amount)?;
            }
        };
        player.has_taken_turn = true;
        self.proceed()
    }

    pub fn proceed(&mut self) -> Result<ServiceRequiredAction> {
        if self.can_proceed_to_next_stage() {
            self.proceed_to_next_stage()?;
            self.setup_stage()
        } else if self.stage == Stage::NotEnoughPlayers {
            Ok(ServiceRequiredAction::NoAction)
        } else {
            // move turn to the next player
            let current_player_index = self
                .players
                .iter()
                .enumerate()
                .find(|(_, p)| Some(p.id) == self.player_in_turn)
                .map(|(index, _)| index)
                .wrap_err("Player not found")?;

            let next_player_id = self.next_player_after(current_player_index)?;

            self.player_in_turn = Some(next_player_id);
            Ok(ServiceRequiredAction::NoAction)
        }
    }

    fn setup_stage(&mut self) -> Result<ServiceRequiredAction> {
        match self.stage {
            Stage::NotEnoughPlayers => {}
            Stage::PreFlop => {
                self.start_game()?;
                return Ok(ServiceRequiredAction::PlayerReceiveCards);
            }
            Stage::Flop => {
                self.deal_community_card(Stage::Flop)?;
                self.player_in_turn = Some(self.player_to_act_first()?);
            }
            Stage::Turn => {
                self.deal_community_card(Stage::Turn)?;
                self.player_in_turn = Some(self.player_to_act_first()?);
            }
            Stage::River => {
                self.deal_community_card(Stage::River)?;
                self.player_in_turn = Some(self.player_to_act_first()?);
            }
            Stage::Showdown => {
                match self.community_cards.len() {
                    0 => {
                        self.deal_community_card(Stage::Flop)?;
                        self.deal_community_card(Stage::Turn)?;
                        self.deal_community_card(Stage::River)?;
                    }
                    3 => {
                        self.deal_community_card(Stage::Turn)?;
                        self.deal_community_card(Stage::River)?;
                    }
                    4 => {
                        self.deal_community_card(Stage::River)?;
                    }
                    5 => {}
                    _ => bail!("Invalid number of community cards"),
                }
                self.player_in_turn = None;
                return Ok(ServiceRequiredAction::FindWinners);
            }
        }
        Ok(ServiceRequiredAction::NoAction)
    }

    fn end_stage(&mut self) -> Result<()> {
        match self.stage {
            Stage::NotEnoughPlayers | Stage::Showdown => {}
            Stage::PreFlop | Stage::Flop | Stage::Turn | Stage::River => {
                // create side pot if needed
                let mut bets = self
                    .players
                    .iter()
                    .filter(|p| p.bet > 0)
                    .map(|p| (p.id, p.bet))
                    .collect::<Vec<_>>();
                bets.sort_by(|a, b| b.1.cmp(&a.1));
                while let Some(smallest_bet) = bets.last().map(|p| p.1) {
                    let mut pot = Pot {
                        amount: 0,
                        players: HashSet::new(),
                    };
                    for b in bets.iter_mut().rev() {
                        b.1 -= smallest_bet;
                        pot.amount += smallest_bet;
                        pot.players.insert(b.0);
                    }
                    self.pots.push(pot);
                    bets.retain(|(_, bet)| *bet > 0);
                }

                // merge consecutive pots with same players
                let mut new_pots = vec![self.pots.first().cloned().wrap_err("No pots")?];
                for i in 1..self.pots.len() {
                    match new_pots.last_mut() {
                        Some(pot) if pot.players == self.pots[i].players => {
                            pot.amount += self.pots[i].amount;
                        }
                        _ => new_pots.push(self.pots[i].clone()),
                    }
                }
                self.pots = new_pots;

                self.players.iter_mut().for_each(|p| {
                    p.has_taken_turn = false;
                    p.bet = 0;
                });
            }
        }
        Ok(())
    }

    fn proceed_to_next_stage(&mut self) -> Result<()> {
        self.end_stage()?;
        if self
            .players
            .iter()
            .filter(|p| !p.has_folded && p.chips > 0)
            .count()
            <= 1
            && self.stage != Stage::Showdown
        {
            self.stage = Stage::Showdown;
            return Ok(());
        }
        match self.stage {
            Stage::Showdown => {
                self.seat_players();
                if self.players.len() >= 2 {
                    self.stage = Stage::PreFlop
                } else {
                    self.stage = Stage::NotEnoughPlayers
                }
            }
            Stage::NotEnoughPlayers => {
                if self.players.len() >= 2 {
                    self.stage = Stage::PreFlop
                } else {
                    unreachable!("Impossible to reach this state")
                }
            }
            Stage::PreFlop => self.stage = Stage::Flop,
            Stage::Flop => self.stage = Stage::Turn,
            Stage::Turn => self.stage = Stage::River,
            Stage::River => self.stage = Stage::Showdown,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::rooms::RoomRepository;
    use crate::repository::users::UserRepository;
    use crate::service::game::GameService;
    use poker::{card, cards, Evaluator};
    use socketioxide::SocketIo;
    use std::str::FromStr;
    use std::sync::Arc;

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
                },
            ],
            deck: Deck::new(),
            community_cards: cards!("6s 7s 8s 9s Ts").try_collect()?,
            stage: Stage::Showdown,
            pots: vec![Pot {
                amount: 0,
                players: HashSet::from([Uuid::from_u128(1), Uuid::from_u128(2)]),
            }],
            player_joining_next_round: Default::default(),
            player_leaving_next_round: Default::default(),
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
                    sid: Sid::from_str("AA9AAA0AAzAAAAHs")?
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
                    sid: Sid::from_str("AA9AAA0AAzAAAAHB")?
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
        PlayerState { bet: 0, has_taken_turn: false, has_folded: false },
        PlayerState { bet: 0, has_taken_turn: false, has_folded: false },
        "Bob",
        false
    )]
    #[case(
        PlayerState { bet: 0, has_taken_turn: false, has_folded: false },
        PlayerState { bet: 0, has_taken_turn: false, has_folded: false },
        "Alice",
        false
    )]
    #[case(
        PlayerState { bet: 10, has_taken_turn: true, has_folded: false },
        PlayerState { bet: 10, has_taken_turn: true, has_folded: false },
        "Bob",
        true
    )]
    #[case(
        PlayerState { bet: 10, has_taken_turn: true, has_folded: false },
        PlayerState { bet: 0, has_taken_turn: false, has_folded: false },
        "Alice",
        false
    )]
    #[case(
        PlayerState { bet: 10, has_taken_turn: true, has_folded: false },
        PlayerState { bet: 5, has_taken_turn: true, has_folded: false },
        "Alice",
        false
    )]
    #[case(
        PlayerState { bet: 15, has_taken_turn: true, has_folded: false },
        PlayerState { bet: 10, has_taken_turn: true, has_folded: true },
        "Bob",
        true
    )]
    #[case(
        PlayerState { bet: 500, has_taken_turn: true, has_folded: false },
        PlayerState { bet: 1000, has_taken_turn: true, has_folded: false },
        "Bob",
        true
    )]
    #[case(
        PlayerState { bet: 500, has_taken_turn: true, has_folded: false },
        PlayerState { bet: 1000, has_taken_turn: true, has_folded: false },
        "Alice",
        true
    )]
    #[case(
        PlayerState { bet: 500, has_taken_turn: true, has_folded: false },
        PlayerState { bet: 22, has_taken_turn: true, has_folded: false },
        "Alice",
        false
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
        #[case] can_proceed: bool,
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
            player_leaving_next_round: Default::default(),
            player_in_turn: if player_in_turn == "Alice" {
                Some(Uuid::from_u128(1))
            } else {
                Some(Uuid::from_u128(2))
            },
        };
        assert_eq!(room.can_proceed_to_next_stage(), can_proceed);
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
        assert_eq!(room.stage, Stage::Showdown);
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
