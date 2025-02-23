use chrono::{DateTime, Utc};
use poker::{Card, Eval};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::domain::room::{Hand, Player, Position, Room, Stage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedGameState {
    pub id: Uuid,
    pub players: Vec<PlayerState>,
    pub community_cards: Vec<CardString>,
    pub pots: Vec<u32>,
    pub stage: Stage,
    pub current_player: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CardString(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timestamped<T> {
    pub timestamp: DateTime<Utc>,
    pub data: T,
}

impl<T> Timestamped<T> {
    pub fn new(data: T) -> Self {
        Timestamped {
            timestamp: Utc::now(),
            data,
        }
    }
}

impl From<[Card; 2]> for PlayerHand {
    fn from(cards: [Card; 2]) -> Self {
        PlayerHand([
            CardString(cards[0].rank_suit_string()),
            CardString(cards[1].rank_suit_string()),
        ])
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub id: Uuid,
    pub name: String,
    pub chips: u32,
    pub bet: u32,
    pub has_folded: bool,
    pub position: Position,
    pub hand: HandState,
    pub eval: Option<String>,
    pub is_connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HandState {
    Empty,
    Hidden,
    Revealed(PlayerHand),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerHand(pub [CardString; 2]);

impl SharedGameState {
    pub fn from_room(room: Room, reveal_cards: bool) -> Self {
        SharedGameState {
            id: room.id,
            players: room
                .players
                .into_iter()
                .map(|p| PlayerState::from_player(p, reveal_cards))
                .collect(),
            community_cards: room.community_cards.into_iter().map(|c| c.into()).collect(),
            pots: room.pots.iter().map(|p| p.amount).collect(),
            stage: room.stage,
            current_player: room.player_in_turn,
        }
    }

    pub fn with_eval(mut self, eval: HashMap<Uuid, Eval>) -> Self {
        for player in self.players.iter_mut() {
            player.eval = eval.get(&player.id).map(|e| e.to_string());
        }
        self
    }
}

impl PlayerState {
    pub fn from_player(player: Player, reveal: bool) -> Self {
        PlayerState {
            id: player.id,
            name: player.name,
            chips: player.chips,
            bet: player.bet,
            has_folded: player.has_folded,
            position: player.position,
            hand: match player.hand {
                None => HandState::Empty,
                Some(Hand(cards)) if reveal => HandState::Revealed(cards.into()),
                Some(_) => HandState::Hidden,
            },
            eval: None,
            is_connected: player.is_connected,
        }
    }
}

impl From<Card> for CardString {
    fn from(card: Card) -> Self {
        CardString(card.rank_suit_string())
    }
}
