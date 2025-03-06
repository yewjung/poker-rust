use std::time::Duration;

use chrono::{DateTime, Utc};
use client::client::Client;
use color_eyre::Result;
use crossterm::event::{self, Event, KeyEvent};
use keyring::Entry;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Clear, Paragraph, Widget, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::screen_data::{
    game_screen_data, InGameScreenWidget, LoginScreenWidget, OnKeyEvent, Screen, ScreenChange,
};
use crate::TOKEN_MANAGER;

pub struct App {
    /// Is the application running?
    running: bool,
    client: Client,
    error_message: Option<ErrorMessage>,
    screen: Screen,
}

struct ErrorMessage {
    message: String,
    expiry_time: DateTime<Utc>,
}

impl ErrorMessage {
    fn is_expired(&self) -> bool {
        Utc::now() > self.expiry_time
    }
}

fn get_token() -> Result<String> {
    TOKEN_MANAGER.get_password().map_err(Into::into)
}

impl App {
    /// Construct a new instance of [`App`].
    pub async fn new() -> Result<Self> {
        let token = get_token().ok();
        let app = match token {
            Some(token) => {
                let mut client = Client::new_with_token(token);
                client.create_ws_connection().await;
                let game_data = game_screen_data(&mut client).await?;
                Self {
                    running: true,
                    client,
                    error_message: None,
                    screen: Screen::InGame(game_data),
                }
            }
            None => Self {
                running: true,
                client: Client::new(),
                error_message: None,
                screen: Screen::Login(Default::default()),
            },
        };
        Ok(app)
    }

    /// Run the application's main loop.
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        self.running = true;
        while self.running {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_crossterm_events().await?;
        }
        Ok(())
    }

    /// Renders the user interface.
    ///
    /// This is where you add new widgets. See the following resources for more information:
    /// - <https://docs.rs/ratatui/latest/ratatui/widgets/index.html>
    /// - <https://github.com/ratatui/ratatui/tree/master/examples>
    fn draw(&mut self, frame: &mut Frame) {
        match self.screen {
            Screen::Login(ref mut data) => {
                frame.render_stateful_widget(LoginScreenWidget, frame.area(), data);
                frame.set_cursor_position(data.cursor_position);
            }
            Screen::InGame(ref mut data) => {
                frame.render_stateful_widget(InGameScreenWidget, frame.area(), data);
            }
        }

        self.render_error_message(frame);
    }

    fn render_error_message(&mut self, frame: &mut Frame) {
        if let Some(error_message) = &self.error_message {
            if error_message.is_expired() {
                self.error_message = None;
            } else {
                let [_, popup_area] =
                    Layout::vertical(Constraint::from_percentages([90, 10])).areas(frame.area());
                let [_, popup_area, _] =
                    Layout::horizontal(Constraint::from_ratios([(1, 3), (1, 3), (1, 3)]))
                        .areas(popup_area);
                frame.render_widget(
                    ErrorPopup {
                        message: error_message.message.clone(),
                    },
                    popup_area,
                );
            }
        }
    }

    /// Reads the crossterm events and updates the state of [`App`].
    ///
    /// If your application needs to perform work in between handling events, you can use the
    /// [`event::poll`] function to check if there are any events available with a timeout.
    async fn handle_crossterm_events(&mut self) -> Result<()> {
        if event::poll(Duration::from_millis(16))? {
            let event = event::read()?;
            if let Event::Key(key_event) = event {
                if let Err(e) = self.on_key_event(key_event).await {
                    self.error_message.replace(e.to_string().into());
                }
            }
        }
        Ok(())
    }

    /// Handles the key events and updates the state of [`App`].
    async fn on_key_event(&mut self, key: KeyEvent) -> Result<()> {
        let change = match self.screen {
            Screen::Login(ref mut data) => data.on_key_event(key, &mut self.client).await?,
            Screen::InGame(ref mut data) => data.on_key_event(key, &mut self.client).await?,
        };

        match change {
            ScreenChange::Switch(screen) => self.screen = screen,
            ScreenChange::Quit => self.quit(),
            ScreenChange::None => {}
        }

        Ok(())
    }
    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
    }
}

pub struct ErrorPopup {
    message: String,
}

impl Widget for ErrorPopup {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        Clear.render(area, buf);
        Paragraph::new(self.message)
            .block(
                Block::bordered()
                    .title("Error occurred")
                    .style(Style::default().fg(Color::Red)),
            )
            .wrap(Wrap { trim: true })
            .render(area, buf);
    }
}

const ERROR_DURATION: Duration = Duration::from_secs(3);

impl From<String> for ErrorMessage {
    fn from(message: String) -> Self {
        Self {
            message,
            expiry_time: Utc::now() + ERROR_DURATION,
        }
    }
}
