use std::time::Duration;

use chrono::{DateTime, Utc};
use client::client::Client;
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Clear, Widget, Wrap, Block, Paragraph};
use ratatui::{
    DefaultTerminal, Frame,
};

use crate::screen_data::{LoginScreenWidget, OnKeyEvent, Screen, ScreenChange};

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

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Construct a new instance of [`App`].
    pub fn new() -> Self {
        Self {
            running: true,
            client: Client::new(),
            error_message: None,
            screen: Screen::Login(Default::default()),
        }
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
            Screen::Login(ref mut screen_data) => {
                frame.render_stateful_widget(LoginScreenWidget, frame.area(), screen_data);
                frame.set_cursor_position(screen_data.cursor_position);
            }
            Screen::InGame(_) => self.draw_game_screen(frame),
        }

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

    fn draw_game_screen(&mut self, frame: &mut Frame) {
        frame.render_widget(
            Paragraph::new("Welcome to the game!")
                .block(
                    Block::bordered()
                        .title("Game")
                        .title_bottom(Line::from("Press Esc to quit").centered()),
                )
                .centered(),
            frame.area(),
        );
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
        match self.screen {
            Screen::Login(ref mut data) => match data.on_key_event(key, &mut self.client).await? {
                ScreenChange::Quit => self.quit(),
                ScreenChange::Switch(screen) => self.screen = screen,
                ScreenChange::None => {}
            },
            Screen::InGame(_) => self.on_in_game_screen_event(key).await?,
        }
        Ok(())
    }

    async fn on_in_game_screen_event(&mut self, key: KeyEvent) -> Result<()> {
        match (key.kind, key.modifiers, key.code) {
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Esc) => {
                self.screen = Screen::Login(Default::default())
            }
            (KeyEventKind::Press, KeyModifiers::CONTROL, KeyCode::Char('c')) => self.quit(),
            _ => {}
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
