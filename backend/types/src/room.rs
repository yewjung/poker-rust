use std::collections::HashSet;

use eyre::{bail, ensure, ContextCompat, Report, Result};
use poker::{box_cards, Card};
use serde::{Deserialize, Serialize};
use socketioxide::socket::Sid;
use sqlx::Type;
use uuid::Uuid;

use crate::domain::{Action, User};

use crate::deck::Deck;
use crate::domain::ServiceRequiredAction;
use crate::error::Error;

#[derive(Debug, Clone)]
pub struct Room {
    pub id: Uuid,
    pub players: Vec<Player>,
    pub deck: Deck,
    pub community_cards: Vec<Card>,
    pub stage: Stage,
    pub pots: Vec<Pot>,
    pub player_joining_next_round: Vec<Player>,
    pub player_in_turn: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq)]
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
    pub is_connected: bool,
}

#[derive(Debug, Clone, PartialEq)]
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
            is_connected: true,
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
            is_connected: true,
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

#[derive(Debug, Clone, PartialEq)]
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
            player_in_turn: None,
        }
    }

    pub fn new_with_id(id: Uuid) -> Self {
        Room {
            id,
            players: Vec::new(),
            deck: Deck::new(),
            community_cards: Vec::with_capacity(5),
            stage: Stage::NotEnoughPlayers,
            pots: vec![],
            player_joining_next_round: Vec::new(),
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
        self.players
            .iter_mut()
            .chain(self.player_joining_next_round.iter_mut())
            .filter(|p| p.id == player_id)
            .for_each(|p| {
                p.is_connected = false;
                p.has_folded = true;
            });
        if self.players.iter().all(|p| !p.is_connected) {
            self.reset_table();
            self.stage = Stage::NotEnoughPlayers;
        }
    }

    fn is_joinable(&self) -> bool {
        self.player_count() < 10
    }

    pub fn player_count(&self) -> usize {
        self.players
            .iter()
            .chain(self.player_joining_next_round.iter())
            .filter(|p| p.is_connected && p.chips > 0)
            .count()
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
        // reset player turn
        self.player_in_turn = None;
    }

    fn seat_players(&mut self) {
        // Remove players who left the game or have no chips
        self.players.retain(|p| p.is_connected && p.chips > 0);
        // Add players who joined the game
        self.player_joining_next_round
            .retain(|p| p.is_connected && p.chips > 0);
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

    pub fn deal_community_card(&mut self, stage: Stage) -> Result<()> {
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

    pub fn closest_to_dealer(&self, player_ids: &HashSet<Uuid>) -> Result<Uuid> {
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
    pub fn can_proceed_to_next_stage(&self) -> bool {
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
