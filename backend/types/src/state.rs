use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use poker::{Card, Eval, Rank, Suit};
use ratatui::prelude::{Color, Span, Style};
use ratatui::style::Stylize;
use ratatui::text::Line;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::domain::Action;
use crate::room::{Hand, Player, Position, Room, Stage};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SharedGameState {
    pub id: Uuid,
    pub players: Vec<PlayerState>,
    pub community_cards: Vec<SerdeCard>,
    pub pots: Vec<u32>,
    pub stage: Stage,
    pub current_player: Option<Uuid>,
}

impl SharedGameState {
    pub fn filled_state_for_test() -> Self {
        let player_id = Uuid::from_str("a3853c6f-58d6-4872-a8ac-17257e330603").unwrap();
        Self {
            id: Default::default(),
            players: vec![
                PlayerState {
                    id: player_id,
                    name: "Yew Jung".to_string(),
                    chips: 500,
                    bet: 10,
                    has_folded: false,
                    position: Position::DealerAndSmallBlind,
                    hand: HandState::Hidden,
                    eval: None,
                    is_connected: true,
                    last_action: Some(Action::Check),
                },
                PlayerState {
                    id: Uuid::new_v4(),
                    name: "John Doe".to_string(),
                    chips: 1000,
                    bet: 20,
                    has_folded: false,
                    position: Position::BigBlind,
                    hand: HandState::Hidden,
                    eval: None,
                    is_connected: true,
                    last_action: None,
                },
                PlayerState {
                    id: Uuid::new_v4(),
                    name: "Jane Doe".to_string(),
                    chips: 1000,
                    bet: 20,
                    has_folded: true,
                    position: Position::BigBlind,
                    hand: HandState::Hidden,
                    eval: None,
                    is_connected: false,
                    last_action: None,
                },
            ],
            community_cards: vec![
                SerdeCard(Card::new(Rank::Ace, Suit::Spades)),
                SerdeCard(Card::new(Rank::King, Suit::Clubs)),
                SerdeCard(Card::new(Rank::Queen, Suit::Hearts)),
                SerdeCard(Card::new(Rank::Queen, Suit::Diamonds)),
            ],
            pots: vec![1000],
            stage: Stage::Flop,
            current_player: Some(player_id),
        }
    }
}

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

    pub fn is_newer(&self, other: &Self) -> bool {
        self.timestamp > other.timestamp
    }
}

impl From<[Card; 2]> for PlayerHand {
    fn from([a, b]: [Card; 2]) -> Self {
        PlayerHand([Some(SerdeCard(a)), Some(SerdeCard(b))])
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
    pub last_action: Option<Action>,
}

impl PlayerState {
    pub fn top_title(&self) -> &str {
        if self.has_folded {
            "Folded"
        } else {
            self.last_action.as_ref().map(AsRef::as_ref).unwrap_or("")
        }
    }

    pub fn bottom_title(&self) -> &str {
        &self.name
    }

    pub fn chips_display(&self) -> Line {
        format!("Chips: {}", self.chips).into()
    }

    pub fn bet_display(&self) -> Line {
        format!("Bet: {}", self.bet).into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HandState {
    Empty,
    Hidden,
    Revealed(PlayerHand),
}

impl HandState {
    pub fn reveal(&mut self, hand: PlayerHand) {
        *self = HandState::Revealed(hand);
    }

    pub fn line(&self) -> Line {
        match self {
            HandState::Empty => "No cards".black().on_white().into(),
            HandState::Hidden => "[ ?? ] [ ?? ]".black().on_white().into(),
            HandState::Revealed(hand) => hand.line(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlayerHand(pub [Option<SerdeCard>; 2]);

impl PlayerHand {
    pub fn line(&self) -> Line {
        let Self([a, b]) = self;
        Line::from(vec![Self::card_span(a), Self::card_span(b)])
    }
    fn card_span(card: &Option<SerdeCard>) -> Span {
        match card {
            Some(card) => card.span(),
            None => "[ ?? ]".black().on_white(),
        }
    }
}

impl PlayerHand {
    pub fn display(&self) -> [String; 2] {
        let Self([a, b]) = self;
        [Self::display_card(a), Self::display_card(b)]
    }

    fn display_card(card: &Option<SerdeCard>) -> String {
        match card {
            Some(card) => card.to_string(),
            None => "[ ?? ]".to_string(),
        }
    }
}

impl SharedGameState {
    pub fn from_room(room: Room, reveal_cards: bool) -> Self {
        SharedGameState {
            id: room.id,
            players: room
                .players
                .into_iter()
                .map(|p| PlayerState::from_player(p, reveal_cards))
                .collect(),
            community_cards: room.community_cards.into_iter().map(SerdeCard).collect(),
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
            last_action: player.last_action,
        }
    }

    pub fn reveal(&mut self, hand: PlayerHand) {
        self.hand.reveal(hand);
    }
}

#[derive(Debug, Clone, derive_more::Deref)]
pub struct SerdeCard(pub Card);

impl SerdeCard {
    pub fn span(&self) -> Span {
        let SerdeCard(card) = &self;
        let fg_color = match card.suit() {
            Suit::Clubs => Color::Green,
            Suit::Hearts => Color::Red,
            Suit::Spades => Color::Black,
            Suit::Diamonds => Color::Blue,
        };
        Span::from(card.to_string()).style(Style::default().fg(fg_color).bg(Color::White))
    }
}

impl<'de> Deserialize<'de> for SerdeCard {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let card = Card::from_str(&s).map_err(serde::de::Error::custom)?;
        Ok(SerdeCard(card))
    }
}

impl Serialize for SerdeCard {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.rank_suit_string().serialize(serializer)
    }
}

#[derive(Debug, Clone, derive_more::Deref)]
pub struct RankChar(Rank);
impl<'de> Deserialize<'de> for RankChar {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let c = char::deserialize(deserializer)?;
        let rank = Rank::try_from(c).map_err(serde::de::Error::custom)?;
        Ok(RankChar(rank))
    }
}

impl Serialize for RankChar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.as_char().serialize(serializer)
    }
}

#[derive(Debug, Clone, derive_more::Deref)]
pub struct SuitChar(Suit);

impl<'de> Deserialize<'de> for SuitChar {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let c = char::deserialize(deserializer)?;
        let suit = Suit::try_from(c).map_err(serde::de::Error::custom)?;
        Ok(SuitChar(suit))
    }
}

impl Serialize for SuitChar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.as_char().serialize(serializer)
    }
}
