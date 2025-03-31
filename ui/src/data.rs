use std::borrow::Cow;
use std::io::Cursor;
use std::thread;

use client::client::Client;
use color_eyre::Result;
use crossterm::event::KeyEvent;
use ratatui::prelude::{Color, Span, Style};
use rodio::{Decoder, OutputStream, Sink};

use crate::game::InGameData;
use crate::lobby::LobbyScreenData;
use crate::login::LoginScreenData;

static DING_SOUND: &[u8] = include_bytes!("../sound_assets/ding.wav");
static CHIPS_SOUND: &[u8] = include_bytes!("../sound_assets/chips.wav");
static CHECK_SOUND: &[u8] = include_bytes!("../sound_assets/check.mp3");
static DEAL_SOUND: &[u8] = include_bytes!("../sound_assets/deal.wav");

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

pub enum Sound {
    Ding,
    Chips,
    Check,
    Deal,
}

impl Sound {
    fn sound(&self) -> &'static [u8] {
        match self {
            Sound::Ding => DING_SOUND,
            Sound::Chips => CHIPS_SOUND,
            Sound::Check => CHECK_SOUND,
            Sound::Deal => DEAL_SOUND,
        }
    }

    pub fn play(&self) {
        let bytes = self.sound();
        thread::spawn(move || {
            // _stream must live as long as the sink
            if let Ok((_stream, stream_handle)) = OutputStream::try_default() {
                if let Ok(sink) = Sink::try_new(&stream_handle) {
                    if let Ok(source) = Decoder::new(Cursor::new(bytes)) {
                        sink.append(source);
                        sink.sleep_until_end();
                    }
                }
            }
        });
    }

    pub fn play_repeat(&self, times: usize) {
        let bytes = self.sound();
        thread::spawn(move || {
            // _stream must live as long as the sink
            if let Ok((_stream, stream_handle)) = OutputStream::try_default() {
                if let Ok(sink) = Sink::try_new(&stream_handle) {
                    for _ in 0..times {
                        if let Ok(source) = Decoder::new(Cursor::new(bytes)) {
                            sink.append(source);
                        }
                    }
                    sink.sleep_until_end();
                }
            }
        });
    }
}