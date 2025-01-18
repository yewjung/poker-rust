use std::collections::HashSet;

use eyre::{ensure, ContextCompat, Result};
use poker::{box_cards, Card, Evaluator};
use uuid::Uuid;

use crate::domain::deck::Deck;

pub struct Room {
    pub id: String,
    pub players: Vec<Player>,
    pub deck: Deck,
    pub community_cards: Vec<Card>,
    pub stage: Stage,
    pub pot: u32,
}

#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct Player {
    id: Uuid,
    name: String,
    hand: Hand,
    chips: u32,
    bet: u32,
    has_folded: bool,
    position: Position,
}

#[derive(Debug, PartialEq)]
enum Stage {
    PreFlop,
    Flop,
    Turn,
    River,
    Showdown,
}

#[derive(Eq, Hash, PartialEq, Clone, Ord, PartialOrd)]
#[cfg_attr(test, derive(Debug))]
enum Position {
    Normal,
    BigBlind,
    SmallBlind,
    DealerAndSmallBlind,
    Dealer,
}

enum Action {
    Fold,
    Check,
    Call(u32),
    Raise(u32),
}

#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct Hand(pub [Card; 2]);

impl Room {
    pub fn new(id: String) -> Self {
        Room {
            id,
            players: Vec::new(),
            deck: Deck::new(),
            community_cards: Vec::with_capacity(5),
            stage: Stage::PreFlop,
            pot: 0,
        }
    }

    fn readjust_positions(&mut self, dealer_id: Uuid) -> Result<()> {
        let total_players = self.players.len();
        let dealer_position = self
            .players
            .iter()
            .position(|p| p.id == dealer_id)
            .wrap_err("Dealer not found")?;
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

    pub fn add_player(&mut self, name: String, buy_in: u32) -> Result<()> {
        ensure!(self.players.len() < 10, "Room is full");
        self.players.push(Player {
            id: Uuid::new_v4(),
            name,
            hand: Hand([self.deck.draw()?, self.deck.draw()?]),
            chips: buy_in,
            bet: 0,
            has_folded: false,
            position: Position::Normal,
        });
        let dealer = self
            .players
            .iter()
            .max_by(|a, b| a.position.cmp(&b.position))
            .map(|p| p.id)
            .wrap_err("No players")?;
        self.readjust_positions(dealer)
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
            .map(|p| (p.id, box_cards!(p.hand.0, self.community_cards)))
            .collect()
    }

    pub fn players_of_ids(&self, ids: HashSet<Uuid>) -> Vec<&Player> {
        self.players
            .iter()
            .filter(|p| ids.contains(&p.id))
            .collect()
    }

    pub fn is_showdown(&self) -> bool {
        self.stage == Stage::Showdown && self.community_cards.len() == 5
    }

    /// Check if all non-folded players have the same bet
    fn can_proceed_to_next_stage(&self) -> bool {
        let all_non_folded_bets = self
            .players
            .iter()
            .filter(|p| !p.has_folded)
            .map(|p| p.bet)
            .collect::<Vec<_>>();
        all_non_folded_bets.windows(2).all(|w| w[0] == w[1])
    }
}

impl Stage {
    fn next(&self) -> Self {
        match self {
            Stage::PreFlop => Stage::Flop,
            Stage::Flop => Stage::Turn,
            Stage::Turn => Stage::River,
            Stage::River => Stage::Showdown,
            Stage::Showdown => Stage::PreFlop,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::game::GameService;
    use poker::{card, cards};

    #[test]
    fn test_add_player() -> Result<()> {
        let mut room = Room::new("test".to_string());
        room.add_player("Alice".to_string(), 400)?;
        room.add_player("Bob".to_string(), 400)?;
        assert_eq!(2, room.players.len());
        Ok(())
    }

    #[test]
    fn test_deal_community_card() -> Result<()> {
        let mut room = Room::new("test".to_string());
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
        };
        let room = Room {
            id: "test".to_string(),
            players: vec![
                Player {
                    id: Uuid::from_u128(1),
                    name: "Alice".to_string(),
                    hand: Hand([card!("3s")?, card!("2s")?]),
                    chips: 0,
                    bet: 0,
                    has_folded: false,
                    position: Position::DealerAndSmallBlind,
                },
                Player {
                    id: Uuid::from_u128(2),
                    name: "Bob".to_string(),
                    hand: Hand([card!("4s")?, card!("5s")?]),
                    chips: 0,
                    bet: 0,
                    has_folded: false,
                    position: Position::BigBlind,
                },
            ],
            deck: Deck::new(),
            community_cards: cards!("6s 7s 8s 9s Ts").try_collect()?,
            stage: Stage::Showdown,
            pot: 0,
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
                    hand: Hand([card!("3s")?, card!("2s")?]),
                    chips: 0,
                    bet: 0,
                    has_folded: false,
                    position: Position::DealerAndSmallBlind,
                },
                &Player {
                    id: Uuid::from_u128(2),
                    name: "Bob".to_string(),
                    hand: Hand([card!("4s")?, card!("5s")?]),
                    chips: 0,
                    bet: 0,
                    has_folded: false,
                    position: Position::BigBlind,
                },
            ]
        );
        Ok(())
    }

    #[test]
    fn test_can_proceed_to_next_stage() {
        let mut room = Room::new("test".to_string());
        room.add_player("Alice".to_string(), 400).unwrap();
        room.add_player("Bob".to_string(), 400).unwrap();
        assert!(room.can_proceed_to_next_stage());
    }

    #[test]
    fn test_readjust_positions_with_4_players() -> Result<()> {
        let mut room = Room::new("test".to_string());
        room.add_player("Alice".to_string(), 400)?;
        room.add_player("Bob".to_string(), 400)?;
        room.add_player("Charlie".to_string(), 400)?;
        room.add_player("David".to_string(), 400)?;
        let positions = room
            .players
            .iter()
            .map(|p| p.position.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            positions,
            vec![
                Position::Dealer,
                Position::SmallBlind,
                Position::BigBlind,
                Position::Normal,
            ]
        );
        Ok(())
    }

    #[test]
    fn test_readjust_positions_with_2_players() -> Result<()> {
        let mut room = Room::new("test".to_string());
        room.add_player("Alice".to_string(), 400)?;
        room.add_player("Bob".to_string(), 400)?;
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
