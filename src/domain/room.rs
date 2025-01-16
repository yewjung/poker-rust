use std::sync::Arc;

use eyre::{ensure, Result};
use poker::{box_cards, Card, Evaluator};

use crate::domain::deck::Deck;

pub struct Room {
    pub id: String,
    pub players: Vec<Player>,
    pub deck: Deck,
    pub community_cards: Vec<Card>,
    pub evaluator: Arc<Evaluator>,
}

#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct Player {
    #[allow(dead_code)]
    name: String,
    hand: Hand,
}

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
        }
    }

    pub fn add_player(&mut self, name: String) -> Result<()> {
        self.players.push(Player {
            name,
            hand: Hand([self.deck.draw()?, self.deck.draw()?]),
        });
        Ok(())
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
                    name: "Alice".to_string(),
                    hand: Hand([card!("3s")?, card!("2s")?]),
                },
                Player {
                    name: "Bob".to_string(),
                    hand: Hand([card!("4s")?, card!("5s")?]),
                },
            ],
            deck: Deck::new(),
            community_cards: cards!("6s 7s 8s 9s Ts").try_collect()?,
            evaluator: Arc::new(Evaluator::new()),
        };

        let winners = room.winners()?;
        // Both players have straight flushes
        assert_eq!(
            winners,
            vec![
                &Player {
                    name: "Alice".to_string(),
                    hand: Hand([card!("3s")?, card!("2s")?]),
                },
                &Player {
                    name: "Bob".to_string(),
                    hand: Hand([card!("4s")?, card!("5s")?]),
                },
            ]
        );
        Ok(())
    }
}
