use std::collections::{HashMap, HashSet};

use eyre::{bail, ensure, ContextCompat, Report, Result};
use poker::{box_cards, Card};
use uuid::Uuid;

use crate::domain::deck::Deck;
use crate::domain::user::User;
use crate::service::game::ServiceRequiredAction;

#[derive(Debug, Clone)]
pub struct Room {
    pub id: Uuid,
    pub players: Vec<Player>,
    pub deck: Deck,
    pub community_cards: Vec<Card>,
    pub stage: Stage,
    pub pot: u32,
    pub player_joining_next_round: Vec<Player>,
    pub player_leaving_next_round: HashMap<Uuid, Player>,
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
        }
    }

    fn bet_amount(&mut self, amount: u32) -> Result<()> {
        ensure!(amount <= self.chips, "Not enough chips");
        self.bet += amount;
        self.chips -= amount;
        Ok(())
    }

    pub fn from_user(user: &User, buy_in: u32) -> Self {
        Player {
            id: user.id,
            name: user.name.clone(),
            hand: None,
            chips: buy_in,
            bet: 0,
            has_folded: false,
            position: Position::Normal,
            has_taken_turn: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    NotEnoughPlayers,
    PreFlop,
    Flop,
    Turn,
    River,
    Showdown,
}

#[derive(Debug, Eq, Hash, PartialEq, Clone, Ord, PartialOrd)]
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
            pot: 0,
            player_joining_next_round: Vec::new(),
            player_leaving_next_round: HashMap::new(),
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

    pub fn join_player(&mut self, player: Player) -> Result<()> {
        match self.stage {
            Stage::NotEnoughPlayers => {
                self.players.push(player);
                self.proceed()?;
            }
            _ => self.player_joining_next_round.push(player),
        };
        Ok(())
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

        // Reset the pot
        self.pot = 0;
        // Reset the community cards
        self.community_cards.clear();
        // Reset the deck
        self.deck = Deck::new();
    }

    fn seat_players(&mut self) {
        // Remove players who left the game
        self.players
            .retain(|p| !self.player_leaving_next_round.contains_key(&p.id));
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
        self.players
            .get((index_of_last_player_to_take_turn + 1) % self.players.len())
            .map(|p| p.id)
            .wrap_err("Player not found")
    }

    pub fn deal_community_card(&mut self) -> Result<()> {
        // burn one card
        self.deck.draw()?;

        let card = self.deck.draw()?;
        self.community_cards.push(card);
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

    pub fn split_pot(&mut self, winners: HashSet<Uuid>) {
        let total_pot = self.pot;
        let total_winners = winners.len();
        let earnings = total_pot / total_winners as u32;
        println!("Total pot: {}", total_pot);
        self.players.iter_mut().for_each(|p| {
            if winners.contains(&p.id) {
                p.chips += earnings;
            }
        });
    }

    /// Check if all non-folded players have the same bet
    fn can_proceed_to_next_stage(&self) -> bool {
        match self.stage {
            Stage::NotEnoughPlayers => self.players.len() >= 2,
            Stage::Showdown => true,
            _ => {
                let number_of_non_folder_players =
                    self.players.iter().filter(|p| !p.has_folded).count();
                if number_of_non_folder_players == 1 {
                    return true;
                }
                let all_players_taken_turn = self
                    .players
                    .iter()
                    .all(|p| p.has_taken_turn || p.has_folded);
                let all_non_folded_bets = self
                    .players
                    .iter()
                    .filter(|p| !p.has_folded)
                    .map(|p| p.bet)
                    .collect::<Vec<_>>();
                all_players_taken_turn && all_non_folded_bets.windows(2).all(|w| w[0] == w[1])
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
                ensure!(amount >= max_bet, "Invalid raise amount");
                player.bet_amount(amount)?;
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

            let next_index = (current_player_index + 1) % self.players.len();
            let end_index = next_index + self.players.len();
            let next_player_index = (next_index..end_index)
                .map(|index| index % self.players.len())
                .find(|index| self.players.get(*index).is_some_and(|p| !p.has_folded))
                .wrap_err("No players to act")?;
            let next_player_id = self
                .players
                .get(next_player_index)
                .map(|p| p.id)
                .wrap_err("Player not found")?;

            self.player_in_turn = Some(next_player_id);
            Ok(ServiceRequiredAction::NoAction)
        }
    }

    fn setup_stage(&mut self) -> Result<ServiceRequiredAction> {
        match self.stage {
            Stage::NotEnoughPlayers => {}
            Stage::PreFlop => {
                self.start_game()?;
            }
            Stage::Flop => {
                self.deal_community_card()?;
                self.deal_community_card()?;
                self.deal_community_card()?;
                self.player_in_turn = Some(self.player_to_act_first()?);
            }
            Stage::Turn => {
                self.deal_community_card()?;
                self.player_in_turn = Some(self.player_to_act_first()?);
            }
            Stage::River => {
                self.deal_community_card()?;
                self.player_in_turn = Some(self.player_to_act_first()?);
            }
            Stage::Showdown => {
                while self.community_cards.len() < 5 {
                    self.deal_community_card()?;
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
                self.pot += self.players.iter().map(|p| p.bet).sum::<u32>();
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
        if self.players.iter().filter(|p| !p.has_folded).count() == 1
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
impl Room {
    pub fn players_of_ids(&self, ids: HashSet<Uuid>) -> Vec<&Player> {
        self.players
            .iter()
            .filter(|p| ids.contains(&p.id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::rooms::RoomRepository;
    use crate::repository::users::UserRepository;
    use crate::service::game::GameService;
    use poker::{card, cards, Evaluator};

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
        for _ in 0..5 {
            room.deal_community_card()?;
        }
        assert_eq!(room.community_cards.len(), 5);
        Ok(())
    }

    #[test]
    fn test_winners() -> Result<()> {
        let game_service = GameService {
            evaluator: Evaluator::new(),
            room_repository: RoomRepository::new(),
            user_repository: UserRepository::new(),
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
                },
            ],
            deck: Deck::new(),
            community_cards: cards!("6s 7s 8s 9s Ts").try_collect()?,
            stage: Stage::Showdown,
            pot: 0,
            player_joining_next_round: Default::default(),
            player_leaving_next_round: Default::default(),
            player_in_turn: None,
        };

        let winners = game_service.find_winners(&room)?;
        let winners = room.players_of_ids(winners);
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
}
