use ansi_to_tui::IntoText;
use client::client::{reset_game_state, reset_hand_state, Client, GAME_STATE, HAND_STATE};
use color_eyre::eyre;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Line, Modifier, StatefulWidget, Style, Widget};
use ratatui::style::{Color, Stylize};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use std::cmp::PartialEq;
use std::fmt::Display;
use std::iter::zip;
use tui_big_text::{BigText, PixelSize};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;
use types::domain::{Action, ActionRequest};
use types::room::{Stage, MAX_NUM_OF_PLAYERS};
use types::state::{HandState, PlayerHand, PlayerState, SerdeCard, SharedGameState};
use uuid::Uuid;

use crate::data::{highlight, OnKeyEvent, OnTick, ScreenChange};
use crate::extension::Splittable;
use crate::{lobby, lookup_image};

const ACTION_BUTTONS: [InGameFocus; 5] = [
    InGameFocus::Check,
    InGameFocus::Call,
    InGameFocus::Raise,
    InGameFocus::Fold,
    InGameFocus::AllIn,
];

pub struct InGameWidget;

impl StatefulWidget for InGameWidget {
    type State = InGameData;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [community, hands, actions] =
            Layout::vertical(Constraint::from_percentages([70, 15, 15])).areas(area);
        let outer_community_block = Block::new()
            .borders(Borders::BOTTOM)
            .border_type(BorderType::Rounded)
            .title_bottom(state.game.stage.line().centered())
            .title_bottom(state.game.pots_line().left_aligned());
        let inner_community_block = outer_community_block.inner(community);

        // render outer block for community cards
        outer_community_block.render(community, buf);

        let hand_areas: [_; MAX_NUM_OF_PLAYERS] = Layout::split_equal(hands, Direction::Horizontal);
        let [_, actions, room_id_area] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Percentage(50),
            Constraint::Fill(1),
        ])
        .areas(actions);
        // community cards
        if state.game.community_cards.is_empty() {
            let [_, text_area, _] =
                Layout::vertical([Constraint::Fill(1), Constraint::Min(0), Constraint::Fill(1)])
                    .areas(inner_community_block);
            let big_text = BigText::builder()
                .pixel_size(PixelSize::Full)
                // .style(Style::new().blue())
                .lines(vec!["No Cards".white().into()])
                .centered()
                .build();
            big_text.render(text_area, buf);
        } else {
            let community_card_areas: [_; 5] =
                Layout::split_equal(inner_community_block, Direction::Horizontal);
            for (card_area, card) in zip(community_card_areas, &state.game.community_cards) {
                card_paragraph(card_area, card, buf);
            }
        }

        for (hand_area, player_state) in zip(hand_areas, &mut state.game.players) {
            if player_state.id == state.user_id
                && !state.hand.is_empty()
                && !matches!(player_state.hand, HandState::Revealed(_))
            {
                player_state.reveal(state.hand.clone());
            }
            let is_in_turn = state
                .game
                .current_player
                .is_some_and(|curr| curr == player_state.id);
            hand_paragraph(hand_area, player_state, is_in_turn, buf);
        }

        action_paragraph(actions, state, buf);
        room_id(room_id_area, state, buf);
    }
}

fn room_id(area: Rect, state: &InGameData, buf: &mut Buffer) {
    let [_, area] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
    let room_id = state.game.id.to_string();
    let room_id_text = format!("Room ID: {}", room_id);
    let room_id_paragraph = Paragraph::new(room_id_text)
        .right_aligned();
    room_id_paragraph.render(area, buf);
}

fn action_paragraph(area: Rect, state: &mut InGameData, buf: &mut Buffer) {
    let mut outer_block = Block::bordered()
        .title(Line::from("Actions").centered())
        .border_type(BorderType::Rounded)
        .style(Color::DarkGray);

    if state.is_in_turn() {
        outer_block = outer_block
            .title_bottom(Line::from("It's Your Turn").centered())
            .style(Color::White);
    }

    let inner_area = outer_block.inner(area);
    outer_block.render(area, buf);
    let [_, button_area, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .areas(inner_area);
    // top = check, call, raise, fold, all-in
    let buttons: [_; 5] = Layout::split_equal(button_area, Direction::Horizontal);
    buttons
        .into_iter()
        .zip(ACTION_BUTTONS)
        .for_each(|(button, action)| {
            action.paragraph(state).render(button, buf);
        });
}

fn hand_paragraph(area: Rect, state: &PlayerState, in_turn: bool, buf: &mut Buffer) {
    let mut outer_block = Block::bordered()
        .title(Line::from(state.top_title()).centered())
        .title_bottom(Line::from(state.bottom_title()).left_aligned())
        .border_type(BorderType::Rounded);

    if in_turn {
        outer_block = outer_block.border_style(Style::default().add_modifier(Modifier::SLOW_BLINK));
    }

    if state.is_dealer() {
        outer_block = outer_block.title_bottom(Line::from("Dealer").right_aligned());
    }

    let inner_block_area = outer_block.inner(area);
    let [_, bet_area, chips_area] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner_block_area);
    Paragraph::new(state.hand.line())
        .block(outer_block)
        .centered()
        .render(area, buf);
    Paragraph::new(state.bet_display().right_aligned()).render(bet_area, buf);
    Paragraph::new(state.chips_display().right_aligned()).render(chips_area, buf);
}

fn card_paragraph(area: Rect, card: &SerdeCard, buf: &mut Buffer) {
    let image_key = card.rank_suit_string();
    let image_text = lookup_image(&image_key);
    let paragraph = image_text
        .and_then(|c| c.into_text().ok())
        .map_or(Paragraph::new(image_key), Paragraph::new);
    paragraph
        .block(
            Block::bordered()
                .title(card.span())
                .title_bottom(card.span())
                .title_alignment(Alignment::Center)
                .border_type(BorderType::Rounded),
        )
        .render(area, buf);
}

#[derive(Debug, Default)]
pub struct InGameData {
    pub user_id: Uuid,
    pub hand: PlayerHand,
    pub game: SharedGameState,
    pub raise_input: Input,
    pub focus: Option<InGameFocus>,
}

impl InGameData {
    pub fn is_in_turn(&self) -> bool {
        self.game
            .current_player
            .as_ref()
            .is_some_and(|id| *id == self.user_id)
    }

    pub fn bet(&self) -> u32 {
        self.game
            .players
            .iter()
            .find(|p| p.id == self.user_id)
            .map(|p| p.bet)
            .unwrap_or_default()
    }

    pub fn chips(&self) -> u32 {
        self.game
            .players
            .iter()
            .find(|p| p.id == self.user_id)
            .map(|p| p.chips)
            .unwrap_or_default()
    }

    pub fn folded(&self) -> bool {
        self.game
            .players
            .iter()
            .find(|p| p.id == self.user_id)
            .map(|p| p.has_folded)
            .unwrap_or_default()
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum InGameFocus {
    #[default]
    Check,
    Call,
    Raise,
    Fold,
    AllIn,
}

impl Display for InGameFocus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: String = match self {
            Self::Check => "Check",
            Self::Call => "Call",
            Self::Raise => "Raise",
            Self::Fold => "Fold",
            Self::AllIn => "All-In",
        }
        .into();
        write!(f, "{}", s)
    }
}

impl InGameFocus {
    fn paragraph(&self, state: &mut InGameData) -> Paragraph {
        let color = if self.enabled(state) {
            Color::White
        } else {
            Color::DarkGray
        };
        let line = match self {
            InGameFocus::Raise => Line::from(state.raise_input.value().to_string()).centered(),
            InGameFocus::Call => highlight(
                format!("{} ({})", self, state.game.max_bet() - state.bet()),
                state.focus.as_ref().is_some_and(|f| f == self),
            )
            .into_centered_line(),
            InGameFocus::AllIn => highlight(
                format!("{} ({})", self, state.chips()),
                state.focus.as_ref().is_some_and(|f| f == self),
            )
            .into_centered_line(),
            _ => highlight(
                self.to_string(),
                state.focus.as_ref().is_some_and(|f| f == self),
            )
            .into_centered_line(),
        };
        match self {
            InGameFocus::Raise => Paragraph::new(line).block(
                Block::bordered()
                    .title(highlight(
                        self.to_string(),
                        state.focus.as_ref().is_some_and(|f| f == self),
                    ))
                    .style(color),
            ),
            _ => Paragraph::new(line).block(Block::bordered().style(color)),
        }
    }

    fn position_in_array(&self) -> usize {
        match self {
            InGameFocus::Check => 0,
            InGameFocus::Call => 1,
            InGameFocus::Raise => 2,
            InGameFocus::Fold => 3,
            InGameFocus::AllIn => 4,
        }
    }

    fn switch(&self, state: &InGameData) -> Option<Self> {
        (1..ACTION_BUTTONS.len())
            .map(|i| (i + self.position_in_array()) % ACTION_BUTTONS.len())
            .map(|i| &ACTION_BUTTONS[i])
            .find(|action| action.enabled(state))
            .cloned()
    }

    fn first_enabled(state: &InGameData) -> Option<Self> {
        ACTION_BUTTONS
            .iter()
            .find(|action| action.enabled(state))
            .cloned()
    }

    fn enabled(&self, state: &InGameData) -> bool {
        if matches!(state.game.stage, Stage::NotEnoughPlayers | Stage::Showdown) {
            return false;
        }
        match self {
            InGameFocus::Check => state.bet() >= state.game.max_bet(),
            InGameFocus::Call => state.game.max_bet().saturating_sub(state.bet()) <= state.chips(),
            InGameFocus::Raise => state.chips() > state.game.max_bet(),
            InGameFocus::Fold => !state.folded(),
            InGameFocus::AllIn => state.chips() > 0,
        }
    }

    fn to_action_request(&self, state: &InGameData) -> eyre::Result<ActionRequest> {
        Ok(ActionRequest {
            room_id: state.game.id,
            action: match self {
                InGameFocus::Check => Action::Check,
                InGameFocus::Call => Action::Call,
                InGameFocus::Raise => Action::Raise(state.raise_input.value().parse()?),
                InGameFocus::Fold => Action::Fold,
                InGameFocus::AllIn => Action::AllIn,
            },
        })
    }
}

pub fn in_game_data(user_id: Uuid, hand: PlayerHand, game: SharedGameState) -> InGameData {
    let mut game = InGameData {
        user_id,
        hand,
        game,
        ..Default::default()
    };
    game.focus = InGameFocus::first_enabled(&game);
    game
}

#[async_trait::async_trait]
impl OnTick for InGameData {
    async fn on_tick(&mut self, _client: &mut Client) -> color_eyre::Result<()> {
        // read GAME_STATE and HAND_STATE, then update self.game and self.hand
        if let Ok(Some(game_state)) = GAME_STATE.try_read().as_deref() {
            self.game = game_state.data.clone();
        }

        if let Ok(Some(hand_state)) = HAND_STATE.try_read().as_deref() {
            self.hand = hand_state.data.clone();
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl OnKeyEvent for InGameData {
    async fn on_key_event(
        &mut self,
        key: KeyEvent,
        client: &mut Client,
    ) -> eyre::Result<ScreenChange> {
        let change = match (key.kind, key.modifiers, key.code) {
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Esc) => {
                client.leave().await?;
                reset_game_state().await;
                reset_hand_state().await;
                lobby::lobby_screen_data(client).await?.into()
            }
            (KeyEventKind::Press, KeyModifiers::CONTROL, KeyCode::Char('c')) => ScreenChange::Quit,
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Tab) => {
                self.focus = self
                    .focus
                    .as_ref()
                    .map_or_else(|| InGameFocus::first_enabled(self), |f| f.switch(self));
                ScreenChange::None
            }
            (KeyEventKind::Press, KeyModifiers::NONE, _)
                if self
                    .focus
                    .as_ref()
                    .is_some_and(|f| f == &InGameFocus::Raise) =>
            {
                if let KeyCode::Char(c) = key.code {
                    if c.is_numeric() {
                        self.raise_input.handle_event(&Event::Key(key));
                    }
                } else {
                    self.raise_input.handle_event(&Event::Key(key));
                }
                ScreenChange::None
            }
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Enter) => {
                if let Some(focus) = &self.focus {
                    let action = focus.to_action_request(self)?;
                    client.action(action).await?;
                };
                ScreenChange::None
            }
            _ => ScreenChange::None,
        };
        Ok(change)
    }
}
