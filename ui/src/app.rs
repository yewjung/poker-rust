use chrono::{DateTime, Utc};
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use names::Generator;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Masked, Span};
use ratatui::widgets::{Clear, Widget, Wrap};
use ratatui::{
    widgets::{Block, Paragraph},
    DefaultTerminal, Frame,
};
use std::time::Duration;
use tokio::try_join;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use client::client::Client;
use types::domain::{LoginRequest, RoomInfo, SignupRequest, UpdateProfileRequest, User};

pub struct App<'a> {
    /// Is the application running?
    running: bool,
    client: Client,
    error_message: Option<ErrorMessage>,
    screen: Screen,
    generator: Generator<'a>,
}

#[derive(Debug, Default)]
struct LoginScreenData {
    email_input: Input,
    password_input: Input,
    focus: LoginScreenFocus,
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

    fn handle_input_event(&mut self, key: KeyEvent) {
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
}

#[derive(Debug, PartialEq, Default)]
enum LoginScreenFocus {
    #[default]
    Email,
    Password,
    Login,
    Signup,
}

#[derive(Debug)]
enum Screen {
    Login(LoginScreenData),
    InGame(InGameScreenData),
}

#[derive(Debug)]
struct InGameScreenData {
    user: User,
    rooms: Vec<RoomInfo>,
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

impl Default for App<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl App<'_> {
    /// Construct a new instance of [`App`].
    pub fn new() -> Self {
        Self {
            running: true,
            client: Client::new(),
            error_message: None,
            screen: Screen::Login(Default::default()),
            generator: Generator::default(),
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
            Screen::Login(ref screen_data) => self.draw_login_screen(screen_data, frame),
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

    fn draw_login_screen(&self, data: &LoginScreenData, frame: &mut Frame) {
        let [_, all, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ])
        .flex(Flex::Center)
        .areas(frame.area());
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
        frame.render_widget(
            Paragraph::new(data.email_input.value()).block(Block::bordered().title("Email")),
            email,
        );

        let [password] = Layout::horizontal([Constraint::Max(50)])
            .flex(Flex::Center)
            .areas(password);
        let password_text =
            Span::styled(Masked::new(data.password_input.value(), '*'), Color::White);
        frame.render_widget(
            Paragraph::new(password_text).block(Block::bordered().title("Password")),
            password,
        );
        let [_, login, signup, _] =
            Layout::horizontal(Constraint::from_percentages([25, 25, 25, 25])).areas(actions);

        frame.render_widget(
            Paragraph::new(highlight("Login", data.focus == LoginScreenFocus::Login))
                .centered()
                .block(Block::bordered()),
            login,
        );
        frame.render_widget(
            Paragraph::new(highlight("Signup", data.focus == LoginScreenFocus::Signup))
                .centered()
                .block(Block::bordered()),
            signup,
        );
        frame.render_widget(
            Paragraph::new("Press Tab to switch focus")
                .style(Style::default().add_modifier(Modifier::ITALIC))
                .centered(),
            instructions,
        );

        self.apply_cursor(data, frame, email, password);
    }

    fn apply_cursor(&self, data: &LoginScreenData, frame: &mut Frame, top: Rect, bottom: Rect) {
        match data.focus {
            LoginScreenFocus::Email => {
                frame.set_cursor_position((
                    top.x + data.email_input.visual_cursor() as u16 + 1,
                    top.y + 1,
                ));
            }
            LoginScreenFocus::Password => {
                frame.set_cursor_position((
                    bottom.x + data.password_input.visual_cursor() as u16 + 1,
                    bottom.y + 1,
                ));
            }
            _ => {}
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
        match self.screen {
            Screen::Login(_) => self.on_login_screen_event(key).await?,
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

    async fn on_login_screen_event(&mut self, key: KeyEvent) -> Result<()> {
        match (key.kind, key.modifiers, key.code) {
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Esc)
            | (KeyEventKind::Press, KeyModifiers::CONTROL, KeyCode::Char('c')) => self.quit(),
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Tab) => self.switch_focus(),
            (KeyEventKind::Press, KeyModifiers::NONE, KeyCode::Enter) => {
                self.handle_enter().await?
            }
            _ => {
                if let Screen::Login(ref mut data) = self.screen {
                    data.handle_input_event(key);
                }
            }
        }
        Ok(())
    }

    fn switch_focus(&mut self) {
        if let Screen::Login(ref mut data) = self.screen {
            data.switch_focus();
        }
    }

    async fn game_screen_data(&self) -> Result<InGameScreenData> {
        let (user, rooms) = try_join!(self.client.get_profile(), self.client.get_rooms())?;
        Ok(InGameScreenData { user, rooms })
    }

    async fn handle_enter(&mut self) -> Result<()> {
        if let Screen::Login(ref mut data) = self.screen {
            match data.focus {
                LoginScreenFocus::Login => {
                    self.client
                        .login(LoginRequest {
                            email: data.email_input.value().to_string(),
                            password: data.password_input.value().to_string(),
                        })
                        .await?;
                    self.screen = Screen::InGame(self.game_screen_data().await?);
                }
                LoginScreenFocus::Signup => {
                    self.client
                        .signup(SignupRequest {
                            email: data.email_input.value().to_string(),
                            password: data.password_input.value().to_string(),
                        })
                        .await?;
                    self.client
                        .login(LoginRequest {
                            email: data.email_input.value().to_string(),
                            password: data.password_input.value().to_string(),
                        })
                        .await?;
                    self.client
                        .update_profile(UpdateProfileRequest {
                            username: self.generator.next().unwrap(),
                        })
                        .await?;
                    self.screen = Screen::InGame(self.game_screen_data().await?);
                }
                _ => self.switch_focus(),
            }
        }
        Ok(())
    }

    /// Set running to false to quit the application.
    fn quit(&mut self) {
        self.running = false;
    }
}

fn highlight(text: &str, needed: bool) -> Span {
    if needed {
        Span::styled(text, Style::default().bg(Color::White).fg(Color::Black))
    } else {
        Span::styled(text, Style::default())
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
