use std::collections::HashSet;
use std::sync::Arc;

use eyre::{ensure, ContextCompat, Result};
use lazy_static::lazy_static;
use poker::{box_cards, Card, Evaluator};
use uuid::Uuid;

use crate::domain::deck::Deck;

lazy_static! {
    static ref DEALER_POSITIONS: HashSet<Position> =
        { HashSet::from([Position::Dealer, Position::DealerAndSmallBlind,]) };
}

pub struct Room {
    pub id: String,
    pub players: Vec<Player>,
    pub deck: Deck,
    pub community_cards: Vec<Card>,
    pub evaluator: Arc<Evaluator>,
    pub stage: Stage,
}

#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct Player {
    id: Uuid,
    #[allow(dead_code)]
    name: String,
    hand: Hand,
    chips: u32,
    bet: u32,
    has_folded: bool,
    position: Position,
}

#[derive(Debug)]
enum Stage {
    PreFlop,
    Flop,
    Turn,
    River,
    Showdown,
}

#[derive(Eq, Hash, PartialEq, Clone)]
enum Position {
    Normal,
    SmallBlind,
    BigBlind,
    Dealer,
    DealerAndSmallBlind,
}

#[allow(dead_code)]
enum Action {
    Fold,
    Check,
    Call(u32),
    Raise(u32),
}

#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
struct Hand([Card; 2]);

impl Room {
    pub fn new(id: String, evaluator: Arc<Evaluator>) -> Self {
        Room {
            id,
            players: Vec::new(),
            deck: Deck::new(),
            community_cards: Vec::with_capacity(5),
            evaluator,
            stage: Stage::PreFlop,
        }
    }

    fn readjust_positions(&mut self, dealer: &Player) -> Result<()> {
        let total_players = self.players.len();
        let dealer_position = self
            .players
            .iter()
            .position(|p| p.id == dealer.id)
            .wrap_err("Dealer not found")?;
        (dealer_position..).take(total_players).for_each(|i| {
            let p = self.players.get_mut(i % total_players).unwrap();
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
        });
        Ok(())
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
        let players = self.players.clone();
        let dealer = players
            .iter()
            .find(|p| DEALER_POSITIONS.contains(&p.position))
            .wrap_err("Dealer not found")?;
        self.readjust_positions(dealer)
    }

    pub fn deal_community_card(&mut self) -> Result<()> {
        // burn one card
        self.deck.draw()?;

        let card = self.deck.draw()?;
        self.community_cards.push(card);
        Ok(())
    }

    pub fn winners(&self) -> Result<Vec<&Player>> {
        ensure!(
            self.community_cards.len() == 5,
            "All community cards must be dealt"
        );
        let mut winners = Vec::with_capacity(self.players.len());
        let mut best_hand = None;
        for player in &self.players {
            let hand = box_cards!(player.hand.0, self.community_cards);
            let hand = self.evaluator.evaluate(hand)?;
            match best_hand {
                None => {
                    best_hand = Some(hand);
                    winners.push(player);
                }
                Some(best) if hand.is_better_than(best) => {
                    best_hand = Some(hand);
                    winners.clear();
                    winners.push(player);
                }
                Some(best) if hand.is_equal_to(best) => {
                    winners.push(player);
                }
                _ => {}
            }
        }
        Ok(winners)
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

#[cfg(test)]
mod tests {
    use super::*;
    use poker::{card, cards};

    #[test]
    fn test_add_player() -> Result<()> {
        let evaluator = Arc::new(Evaluator::new());
        let mut room = Room::new("test".to_string(), evaluator);
        room.add_player("Alice".to_string())?;
        room.add_player("Bob".to_string())?;
        assert_eq!(2, room.players.len());
        Ok(())
    }

    #[test]
    fn test_deal_community_card() -> Result<()> {
        let evaluator = Arc::new(Evaluator::new());
        let mut room = Room::new("test".to_string(), evaluator);
        for _ in 0..5 {
            room.deal_community_card()?;
        }
        assert_eq!(room.community_cards.len(), 5);
        Ok(())
    }

    #[test]
    fn test_winners() -> Result<()> {
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
            evaluator: Arc::new(Evaluator::new()),
            stage: Stage::Showdown,
        };

        let winners = room.winners()?;
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
        let evaluator = Arc::new(Evaluator::new());
        let mut room = Room::new("test".to_string(), evaluator);
        room.add_player("Alice".to_string(), 400).unwrap();
        room.add_player("Bob".to_string(), 400).unwrap();
        assert!(room.can_proceed_to_next_stage());
    }
}
