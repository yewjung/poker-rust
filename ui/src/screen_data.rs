use std::iter::zip;
use std::time::Duration;

use ansi_to_tui::IntoText;
use chrono::{DateTime, Utc};
use client::client::{Client, GAME_STATE, HAND_STATE};
use color_eyre::eyre::ContextCompat;
use color_eyre::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Position, Rect};
use ratatui::prelude::{Color, Masked, Modifier, Span, Style};
use ratatui::text::Line;
use ratatui::widgets::{
    Block, BorderType, Cell, Paragraph, Row, StatefulWidget, Table, TableState, Widget,
};
use tokio::time::sleep;
use tokio::try_join;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use types::domain::{JoinGameRequest, LoginRequest, RoomInfo, SignupRequest, User};
use types::error::Error;
use types::room::MAX_NUM_OF_PLAYERS;
use types::state::{HandState, PlayerHand, PlayerState, SerdeCard, SharedGameState};
use uuid::Uuid;

use crate::{IMAGE_CACHE, TOKEN_MANAGER};

#[derive(Debug, Default)]
pub struct LoginScreenData {
    email_input: Input,
    password_input: Input,
    focus: LoginScreenFocus,
    pub(crate) cursor_position: Position,
}

impl From<LoginScreenData> for ScreenChange {
    fn from(data: LoginScreenData) -> Self {
        ScreenChange::Switch(Screen::Login(data))
    }
}

impl LoginScreenData {
    fn switch_focus(&mut self) {
        match self.focus {
            LoginScreenFocus::Email => {
                self.focus = LoginScreenFocus::Password;
            }
            LoginScreenFocus::Password => {
                self.focus = LoginScreenFocus::Login;
            }
            LoginScreenFocus::Login => {
                self.focus = LoginScreenFocus::Signup;
            }
            LoginScreenFocus::Signup => {
                self.focus = LoginScreenFocus::Email;
            }
        }
    }

    pub(crate) fn handle_input_event(&mut self, key: KeyEvent) {
        match self.focus {
            LoginScreenFocus::Email => {
                self.email_input.handle_event(&Event::Key(key));
            }
            LoginScreenFocus::Password => {
                self.password_input.handle_event(&Event::Key(key));
            }
            _ => {}
        }
    }

    async fn handle_enter(&mut self, client: &mut Client) -> Result<ScreenChange> {
        let change = match self.focus {
            LoginScreenFocus::Login => {
                let token = client
                    .login(LoginRequest {
                        email: self.email_input.value().to_string(),
                        password: self.password_input.value().to_string(),
                    })
                    .await?;
                TOKEN_MANAGER.set_password(&token)?;
                game_screen_data(client).await?.into()
            }
            LoginScreenFocus::Signup => {
                client
                    .signup(SignupRequest {
                        email: self.email_input.value().to_string(),
                        password: self.password_input.value().to_string(),
                    })
                    .await?;
                let token = client
                    .login(LoginRequest {
                        email: self.email_input.value().to_string(),
                        password: self.password_input.value().to_string(),
                    })
                    .await?;
                TOKEN_MANAGER.set_password(&token)?;
                client.update_profile_with_random_name().await?;
                game_screen_data(client).await?.into()
            }
            _ => {
                self.switch_focus();
                ScreenChange::None
            }
        };
        Ok(change)
    }
}

#[derive(Debug, PartialEq, Default)]
pub enum LoginScreenFocus {
    #[default]
    Email,
    Password,
    Login,
    Signup,
}

impl LoginScreenData {
    fn update_cursor_position(&mut self, top: Rect, bottom: Rect) {
        match self.focus {
            LoginScreenFocus::Email => {
                self.cursor_position = (
                    top.x + self.email_input.visual_cursor() as u16 + 1,
                    top.y + 1,
                )
                    .into();
            }
            LoginScreenFocus::Password => {
                self.cursor_position = (
                    bottom.x + self.password_input.visual_cursor() as u16 + 1,
                    bottom.y + 1,
                )
                    .into();
            }
            _ => {}
        }
    }
}

fn highlight(text: &str, needed: bool) -> Span {
    if needed {
        Span::styled(text, Style::default().bg(Color::White).fg(Color::Black))
    } else {
        Span::styled(text, Style::default())
    }
}

pub struct LoginScreenWidget;

impl StatefulWidget for LoginScreenWidget {
    type State = LoginScreenData;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [_, all, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ])
        .flex(Flex::Center)
        .areas(area);
        let [email, password, actions, instructions] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .areas(all);

        let [email] = Layout::horizontal([Constraint::Max(50)])
            .flex(Flex::Center)
            .areas(email);
        Paragraph::new(state.email_input.value())
            .block(Block::bordered().title("Email"))
            .render(email, buf);

        let [password] = Layout::horizontal([Constraint::Max(50)])
            .flex(Flex::Center)
            .areas(password);
        let password_text =
            Span::styled(Masked::new(state.password_input.value(), '*'), Color::White);
        Paragraph::new(password_text)
            .block(Block::bordered().title("Password"))
            .render(password, buf);
        let [_, login, signup, _] =
            Layout::horizontal(Constraint::from_percentages([25, 25, 25, 25])).areas(actions);

        Paragraph::new(highlight("Login", state.focus == LoginScreenFocus::Login))
            .centered()
            .block(Block::bordered())
            .render(login, buf);
        Paragraph::new(highlight("Signup", state.focus == LoginScreenFocus::Signup))
            .centered()
            .block(Block::bordered())
            .render(signup, buf);
        Paragraph::new("Press Tab to switch focus")
            .style(Style::default().add_modifier(Modifier::ITALIC))
            .centered()
            .render(instructions, buf);
        state.update_cursor_position(email, password);
    }
}

pub enum ScreenChange {
    Quit,
    Switch(Screen),
    None,
}

#[derive(Debug)]
pub enum Screen {
    Login(LoginScreenData),
    Lobby(LobbyScreenData),
    InGame(InGameData),
}

#[derive(Debug)]
pub struct LobbyScreenData {
    user: User,
    rooms: Vec<RoomInfo>,
    table_state: TableState,
    next_refresh_time: DateTime<Utc>,
}

impl LobbyScreenData {
    pub async fn refresh(&mut self, client: &mut Client) -> Result<()> {
        if Utc::now() > self.next_refresh_time {
            let data = game_screen_data(client).await?;
            self.user = data.user;
            self.rooms = data.rooms;
            self.next_refresh_time = data.next_refresh_time;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct InGameData {
    user_id: Uuid,
    hand: PlayerHand,
    game: SharedGameState,
}

pub fn in_game_data(user_id: Uuid, hand: PlayerHand, game: SharedGameState) -> InGameData {
    InGameData {
        user_id,
        hand,
        game,
    }
}

impl From<LobbyScreenData> for ScreenChange {
    fn from(data: LobbyScreenData) -> Self {
        ScreenChange::Switch(Screen::Lobby(data))
    }
}

#[async_trait::async_trait]
pub trait OnTick {
    async fn on_tick(&mut self, client: &mut Client) -> Result<()>;
}

#[async_trait::async_trait]
impl OnTick for LoginScreenData {
    async fn on_tick(&mut self, _client: &mut Client) -> Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl OnTick for LobbyScreenData {
    async fn on_tick(&mut self, client: &mut Client) -> Result<()> {
        self.refresh(client).await
    }
}

#[async_trait::async_trait]
impl OnTick for InGameData {
    async fn on_tick(&mut self, _client: &mut Client) -> Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait OnKeyEvent {
    async fn on_key_event(&mut self, key: KeyEvent, client: &mut Client) -> Result<ScreenChange>;
}

#[async_trait::async_trait]
impl OnKeyEvent for LoginScreenData {
    async fn on_key_event(&mut self, key: KeyEvent, client: &mut Client) -> Result<ScreenChange> {
        match (key.kind, key.modifiers, key.code) {
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Esc)
            | (KeyEventKind::Press, KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                Ok(ScreenChange::Quit)
            }
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Tab) => {
                self.switch_focus();
                Ok(ScreenChange::None)
            }
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Enter) => {
                self.handle_enter(client).await
            }
            _ => {
                self.handle_input_event(key);
                Ok(ScreenChange::None)
            }
        }
    }
}

pub struct LobbyWidget;

impl StatefulWidget for LobbyWidget {
    type State = LobbyScreenData;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [user, rooms] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(area);
        let [user_left, user_right] =
            Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(user);
        Paragraph::new(state.user.name.as_str())
            .block(Block::bordered().title("Username"))
            .render(user_left, buf);
        Paragraph::new(state.user.balance.to_string())
            .block(Block::bordered().title("Balance"))
            .render(user_right, buf);
        let header = ["Room", "Player Count"]
            .into_iter()
            .map(Cell::from)
            .collect::<Row>()
            .height(1);
        let selected_row_style = Style::default().add_modifier(Modifier::REVERSED);
        let rows = state
            .rooms
            .iter()
            .map(|room| {
                [
                    room.room_id.to_string(),
                    format!("{}/{}", room.player_count, MAX_NUM_OF_PLAYERS),
                ]
            })
            .map(Row::new)
            .collect::<Vec<_>>();
        let table = Table::new(rows, Constraint::from_percentages([70, 30]))
            .block(
                Block::bordered()
                    .title(Line::from("Rooms").centered())
                    .title_bottom(Line::from("Press Esc to quit").centered()),
            )
            .row_highlight_style(selected_row_style)
            .header(header);
        StatefulWidget::render(table, rooms, buf, &mut state.table_state);
    }
}

#[async_trait::async_trait]
impl OnKeyEvent for LobbyScreenData {
    async fn on_key_event(&mut self, key: KeyEvent, client: &mut Client) -> Result<ScreenChange> {
        let change = match (key.kind, key.modifiers, key.code) {
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Esc) => {
                LoginScreenData::default().into()
            }
            (KeyEventKind::Press, KeyModifiers::CONTROL, KeyCode::Char('c')) => ScreenChange::Quit,
            (
                KeyEventKind::Press,
                KeyModifiers::NONE,
                KeyCode::Down | KeyCode::Right | KeyCode::Tab,
            ) => {
                self.table_state.select_next();
                ScreenChange::None
            }
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Up | KeyCode::Left) => {
                self.table_state.select_previous();
                ScreenChange::None
            }
            // refresh data
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Char('r')) => {
                *self = game_screen_data(client).await?;
                ScreenChange::None
            }
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Enter) => {
                let room = self
                    .table_state
                    .selected()
                    .and_then(|selected| self.rooms.get(selected));

                let room = room.wrap_err(Error::NoRoomFound)?;
                client
                    .join_game(JoinGameRequest {
                        room_id: room.room_id,
                        buy_in: 100,
                    })
                    .await?;

                // poll GAME_STATE until it is Some
                loop {
                    if let Ok(Some(game_state)) = GAME_STATE.try_read().as_deref() {
                        let hand = HAND_STATE.read().await;
                        return Ok(ScreenChange::Switch(Screen::InGame(InGameData {
                            user_id: client.user.as_ref().map(|u| u.id).wrap_err("No user")?,
                            hand: hand
                                .as_ref()
                                .map_or(PlayerHand::default(), |h| h.data.clone()),
                            game: game_state.data.clone(),
                        })));
                    } else {
                        sleep(Duration::from_secs(1)).await;
                    }
                }
            }
            _ => ScreenChange::None,
        };
        Ok(change)
    }
}

pub async fn game_screen_data(client: &mut Client) -> Result<LobbyScreenData> {
    let (user, rooms) = try_join!(client.get_profile(), client.get_rooms())?;
    Ok(LobbyScreenData {
        user,
        rooms,
        table_state: TableState::default().with_selected(0),
        next_refresh_time: Utc::now() + Duration::from_secs(5),
    })
}

#[async_trait::async_trait]
impl OnKeyEvent for InGameData {
    async fn on_key_event(&mut self, key: KeyEvent, client: &mut Client) -> Result<ScreenChange> {
        let change = match (key.kind, key.modifiers, key.code) {
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Esc) => {
                client.leave().await?;
                game_screen_data(client).await?.into()
            }
            (KeyEventKind::Press, KeyModifiers::CONTROL, KeyCode::Char('c')) => ScreenChange::Quit,
            _ => ScreenChange::None,
        };
        Ok(change)
    }
}

pub struct InGameWidget;

impl StatefulWidget for InGameWidget {
    type State = InGameData;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [top, bottom, _] =
            Layout::vertical(Constraint::from_percentages([70, 15, 15])).areas(area);
        let community_card_areas: [_; 5] =
            Layout::horizontal(Constraint::from_ratios([(1, 5); 5])).areas(top);
        let hand_areas: [_; MAX_NUM_OF_PLAYERS] = Layout::horizontal(Constraint::from_ratios(
            [(1, MAX_NUM_OF_PLAYERS as u32); MAX_NUM_OF_PLAYERS],
        ))
        .areas(bottom);
        // community cards
        for (card_area, card) in zip(community_card_areas, &state.game.community_cards) {
            card_paragraph(card_area, card, buf);
        }

        for (hand_area, player_state) in zip(hand_areas, &mut state.game.players) {
            if player_state.id == state.user_id
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
    }
}

fn hand_paragraph(area: Rect, state: &PlayerState, in_turn: bool, buf: &mut Buffer) {
    let mut outer_block = Block::bordered()
        .title(Line::from(state.top_title()).centered())
        .title_bottom(Line::from(state.bottom_title()).left_aligned())
        .border_type(BorderType::Rounded);

    if in_turn {
        outer_block = outer_block.border_style(Style::default().add_modifier(Modifier::SLOW_BLINK));
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
    let image_text = IMAGE_CACHE.get(&card.rank_suit_string());
    let paragraph = image_text
        .and_then(|c| c.into_text().ok())
        .map_or(Paragraph::new(card.rank_suit_string()), Paragraph::new);
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
