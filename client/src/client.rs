use eyre::{bail, ContextCompat, Result};
use futures_util::FutureExt;
use lazy_static::lazy_static;
use log::debug;
use reqwest::Client as ReqwestClient;
use reqwest::StatusCode;
use rnglib::{Language, RNG};
use rust_socketio::asynchronous::Client as SocketClient;
use rust_socketio::asynchronous::ClientBuilder;
use rust_socketio::{Payload};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;
use types::domain::*;
use types::room::Winnings;
use types::state::{PlayerHand, SharedGameState, Timestamped};

lazy_static! {
    pub static ref GAME_STATE: RwLock<Option<Timestamped<SharedGameState>>> = RwLock::new(None);
    pub static ref HAND_STATE: RwLock<Option<Timestamped<PlayerHand>>> = RwLock::new(None);
    pub static ref OUTCOME_STATE: RwLock<Option<Timestamped<Vec<Winnings>>>> = RwLock::new(None);
    pub static ref CONNECTION_IS_CLOSE: AtomicBool = AtomicBool::new(false);
}

async fn reset_state<T>(state_lock: &RwLock<Option<T>>) {
    loop {
        if let Ok(state) = state_lock.try_write().as_deref_mut() {
            *state = None;
            return;
        } else {
            sleep(Duration::from_secs(1)).await;
        }
    }
}

pub async fn reset_game_state() {
    reset_state(&GAME_STATE).await;
}

pub async fn reset_hand_state() {
    reset_state(&HAND_STATE).await;
}

async fn update_connection_status() {
    // update CONNECTION_IS_CLOSE to true
    CONNECTION_IS_CLOSE.store(true, Ordering::Relaxed);
}

async fn update_state<T: for<'a> Deserialize<'a> + Debug>(
    payload: Payload,
    state: &RwLock<Option<Timestamped<T>>>,
) {
    if let Payload::Text(values) = payload {
        let states: Vec<Timestamped<T>> = values
            .into_iter()
            .filter_map(|value| match serde_json::from_value(value) {
                Ok(game_state) => Some(game_state),
                Err(e) => {
                    debug!("Error deserializing: {:?}", e);
                    None
                }
            })
            .collect();
        if let Some(new_state) = states.into_iter().next() {
            debug!("New state: {:#?}", new_state);
            let mut state_lock = state.write().await;
            if let Some(ref current_state) = *state_lock {
                new_state.is_newer(current_state).then(|| {
                    state_lock.replace(new_state);
                });
            } else {
                state_lock.replace(new_state);
            }
        }
    };
}

#[allow(deprecated)]
async fn default_callback(payload: Payload) {
    match payload {
        Payload::Text(values) => debug!("Received text: {:#?}", values),
        Payload::Binary(bin_data) => debug!("Received bytes: {:#?}", bin_data),
        // This is deprecated use Payload::Text instead
        Payload::String(str) => debug!("Received str: {}", str),
    }
}

pub struct Client {
    pub client: ReqwestClient,
    pub ws_client: Option<SocketClient>,
    pub token: Option<String>,
    pub user: Option<User>,
    generator: RNG,
}

// const BASE_URL: &str = "http://yj-api-poker.ragib.cloudns.org:8080";
// const BASE_URL: &str = "https://yj-api-poker.apps.bancuh.net";
const BASE_URL: &str = "https://poker.yewjung.com";

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            ws_client: None,
            token: None,
            user: None,
            generator: RNG::from(&Language::Roman),
        }
    }

    pub async fn new_with_token(token: String) -> Result<Self> {
        let mut s = Self {
            client: reqwest::Client::new(),
            ws_client: None,
            token: Some(token),
            user: None,
            generator: RNG::from(&Language::Roman),
        };
        let user = s.get_profile().await?;
        s.user.replace(user);
        Ok(s)
    }

    pub async fn signup(&self, request: SignupRequest) -> Result<()> {
        let url = format!("{}/signup", BASE_URL);
        let response = self.client.post(url).json(&request).send().await?;
        let status = response.status();
        match status {
            StatusCode::CREATED => Ok(()),
            _ => bail!(response.text().await?),
        }
    }

    pub async fn login(&mut self, request: LoginRequest) -> Result<String> {
        let url = format!("{}/login", BASE_URL);
        let response = self.client.post(url).json(&request).send().await?;
        let status = response.status();
        let token = match status {
            StatusCode::OK => response.text().await?,
            _ => bail!(response.text().await?),
        };
        self.token = Some(token.clone());
        self.create_ws_connection().await?;
        Ok(token)
    }

    pub async fn update_profile(&mut self, request: UpdateProfileRequest) -> Result<User> {
        let url = format!("{}/profile", BASE_URL);
        let token = self.token.clone().expect("No token");
        let response = self
            .client
            .patch(url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&request)
            .send()
            .await?;
        let status = response.status();
        let user: User = match status {
            StatusCode::OK => response.json().await?,
            _ => bail!(response.text().await?),
        };
        self.user.replace(user.clone());
        Ok(user)
    }

    pub async fn update_profile_with_random_name(&mut self) -> Result<User> {
        let first_name = self.generator.generate_name();
        let last_name = self.generator.generate_name();

        let username = format!("{} {}", first_name, last_name);
        let request = UpdateProfileRequest { username };
        self.update_profile(request).await
    }

    pub async fn get_profile(&self) -> Result<User> {
        let url = format!("{}/profile", BASE_URL);
        let token = self.token.clone().expect("No token");
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?;
        let status = response.status();
        match status {
            StatusCode::OK => Ok(response.json().await?),
            _ => bail!(response.text().await?),
        }
    }

    pub async fn get_rooms(&self) -> Result<Vec<RoomInfo>> {
        let url = format!("{}/rooms", BASE_URL);
        let token = self.token.clone().expect("No token");
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?;
        let status = response.status();
        match status {
            StatusCode::OK => Ok(response.json().await?),
            _ => bail!(response.text().await?),
        }
    }

    pub async fn create_ws_connection(&mut self) -> Result<()> {
        let hand_callback = |payload, _| update_state(payload, &HAND_STATE).boxed();
        let room_callback = |payload, _| update_state(payload, &GAME_STATE).boxed();
        let outcome_callback = |payload, _| update_state(payload, &OUTCOME_STATE).boxed();
        let error_callback = |payload, _| default_callback(payload).boxed();
        let default_callback = |payload, _| default_callback(payload).boxed();
        let close_callback = |_, _| update_connection_status().boxed();

        // Creates a GET request, upgrades and sends it.
        let token = self.token.clone().expect("No token");
        self.ws_client = Some(
            ClientBuilder::new(BASE_URL)
                .namespace("/game")
                .auth(token)
                .on("hand", hand_callback)
                .on("room", room_callback)
                .on("outcome", outcome_callback)
                .on("service_error", error_callback)
                .on("error", default_callback)
                .on("close", close_callback)
                .connect()
                .await?,
        );
        Ok(())
    }

    pub async fn join_game(&mut self, payload: JoinGameRequest) -> Result<()> {
        self.emit(ClientEvent::Join, payload).await
    }

    pub async fn action(&mut self, payload: ActionRequest) -> Result<()> {
        self.emit(ClientEvent::Action, payload).await
    }

    pub async fn leave(&mut self) -> Result<()> {
        self.emit(ClientEvent::Leave, String::default()).await
    }

    async fn emit<T: Serialize>(&mut self, event: ClientEvent, payload: T) -> Result<()> {
        let ws_socket = self.ws_client.as_ref().wrap_err("No socket connection")?;
        ws_socket.emit(event.as_ref(), json!(payload)).await?;

        Ok(())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(ws_client) = &self.ws_client {
            let client = ws_client.clone();
            tokio::spawn(async move {
                client
                    .disconnect()
                    .await
                    .expect("Failed to disconnect in drop");
            });
        }
    }
}
