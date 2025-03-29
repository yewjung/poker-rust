use crate::game::InGameData;
use crate::lobby::LobbyScreenData;
use crate::login::LoginScreenData;
use client::client::Client;
use color_eyre::Result;
use crossterm::event::KeyEvent;
use ratatui::prelude::{Color, Span, Style};
use std::borrow::Cow;

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

#[async_trait::async_trait]
pub trait OnTick {
    async fn on_tick(&mut self, client: &mut Client) -> Result<()>;
}

#[async_trait::async_trait]
pub trait OnKeyEvent {
    async fn on_key_event(&mut self, key: KeyEvent, client: &mut Client) -> Result<ScreenChange>;
}

pub fn highlight<'a>(text: impl Into<Cow<'a, str>>, needed: bool) -> Span<'a> {
    if needed {
        Span::styled(text, Style::default().bg(Color::White).fg(Color::Black))
    } else {
        Span::styled(text, Style::default())
    }
}
