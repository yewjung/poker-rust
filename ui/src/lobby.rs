use std::default::Default;
use std::time::Duration;

use chrono::{DateTime, Utc};
use client::client::{Client, GAME_STATE, HAND_STATE};
use color_eyre::eyre::ContextCompat;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Line, Modifier, StatefulWidget, Style, Widget};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table, TableState};
use tokio::time::sleep;
use tokio::try_join;
use types::domain::{JoinGameRequest, RoomInfo, User};
use types::error::Error;
use types::room::MAX_NUM_OF_PLAYERS;
use types::state::PlayerHand;

use crate::data::{OnKeyEvent, OnTick, Screen, ScreenChange};
use crate::extension::Splittable;
use crate::game::InGameData;
use crate::login::LoginScreenData;

#[derive(Debug)]
pub struct LobbyScreenData {
    pub user: User,
    pub rooms: Vec<RoomInfo>,
    pub table_state: TableState,
    pub next_refresh_time: DateTime<Utc>,
}

impl LobbyScreenData {
    pub async fn refresh(&mut self, client: &mut Client) -> color_eyre::Result<()> {
        if Utc::now() > self.next_refresh_time {
            let data = lobby_screen_data(client).await?;
            self.user = data.user;
            self.rooms = data.rooms;
            self.next_refresh_time = data.next_refresh_time;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl OnTick for LobbyScreenData {
    async fn on_tick(&mut self, client: &mut Client) -> color_eyre::Result<()> {
        self.refresh(client).await
    }
}

pub struct LobbyWidget;

impl StatefulWidget for LobbyWidget {
    type State = LobbyScreenData;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [user, rooms] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(area);
        let [user_left, user_right] = Layout::split_equal(user, Direction::Vertical);
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
    async fn on_key_event(
        &mut self,
        key: KeyEvent,
        client: &mut Client,
    ) -> color_eyre::Result<ScreenChange> {
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
                *self = lobby_screen_data(client).await?;
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
                            ..Default::default()
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

pub async fn lobby_screen_data(client: &mut Client) -> color_eyre::Result<LobbyScreenData> {
    let (user, rooms) = try_join!(client.get_profile(), client.get_rooms())?;
    Ok(LobbyScreenData {
        user,
        rooms,
        table_state: TableState::default().with_selected(0),
        next_refresh_time: Utc::now() + Duration::from_secs(5),
    })
}

impl From<LobbyScreenData> for ScreenChange {
    fn from(data: LobbyScreenData) -> Self {
        ScreenChange::Switch(Screen::Lobby(data))
    }
}
