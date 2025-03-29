use crate::data::{OnKeyEvent, OnTick, Screen, ScreenChange};
use crate::extension::Splittable;
use crate::{data, lobby, TOKEN_MANAGER};
use client::client::Client;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Flex, Layout, Position, Rect};
use ratatui::prelude::{Color, Masked, Modifier, Span, StatefulWidget, Style, Widget};
use ratatui::widgets::{Block, Paragraph};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;
use types::domain::{LoginRequest, SignupRequest};

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

    async fn handle_enter(&mut self, client: &mut Client) -> color_eyre::Result<ScreenChange> {
        let change = match self.focus {
            LoginScreenFocus::Login => {
                let token = client
                    .login(LoginRequest {
                        email: self.email_input.value().to_string(),
                        password: self.password_input.value().to_string(),
                    })
                    .await?;
                TOKEN_MANAGER.set_password(&token)?;
                lobby::lobby_screen_data(client).await?.into()
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
                lobby::lobby_screen_data(client).await?.into()
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
        let [_, login, signup, _] = Layout::split_equal(actions, Direction::Horizontal);

        Paragraph::new(data::highlight(
            "Login",
            state.focus == LoginScreenFocus::Login,
        ))
        .centered()
        .block(Block::bordered())
        .render(login, buf);
        Paragraph::new(data::highlight(
            "Signup",
            state.focus == LoginScreenFocus::Signup,
        ))
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

#[async_trait::async_trait]
impl OnTick for LoginScreenData {
    async fn on_tick(&mut self, _client: &mut Client) -> color_eyre::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl OnKeyEvent for LoginScreenData {
    async fn on_key_event(
        &mut self,
        key: KeyEvent,
        client: &mut Client,
    ) -> color_eyre::Result<ScreenChange> {
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
