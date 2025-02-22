use eyre::{bail, Result};
use futures_util::FutureExt;
use reqwest::Client as ReqwestClient;
use reqwest::StatusCode;
use rust_socketio::asynchronous::Client as SocketClient;
use rust_socketio::asynchronous::ClientBuilder;
use rust_socketio::Payload;
use serde::Serialize;
use serde_json::json;
use types::domain::*;

pub struct Client {
    pub client: ReqwestClient,
    pub ws_client: Option<SocketClient>,
    pub token: Option<String>,
}

const BASE_URL: &str = "http://localhost:8080";

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
        }
    }
    pub async fn signup(&self, request: SignupRequest) -> Result<()> {
        let url = format!("{}/signup", BASE_URL);
        let response = self.client.post(url).json(&request).send().await?;
        let status = response.status();
        match status {
            StatusCode::CREATED => Ok(()),
            _ => bail!(status),
        }
    }

    pub async fn login(&mut self, request: LoginRequest) -> Result<String> {
        let url = format!("{}/login", BASE_URL);
        let response = self.client.post(url).json(&request).send().await?;
        let status = response.status();
        match status {
            StatusCode::OK => {
                let token = response.text().await?;
                self.token = Some(token.clone());
                Ok(token)
            }
            _ => bail!(status),
        }
    }

    pub async fn update_profile(&self, request: UpdateProfileRequest) -> Result<User> {
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
        match status {
            StatusCode::OK => Ok(response.json().await?),
            _ => bail!(status),
        }
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
            _ => bail!(status),
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
            _ => bail!(status),
        }
    }

    pub async fn create_ws_connection(&mut self) {
        let hand_callback = |payload: Payload, _socket: SocketClient| {
            async move {
                println!("hand event:");
                match payload {
                    Payload::Text(values) => println!("Received: {:#?}", values),
                    Payload::Binary(bin_data) => println!("Received bytes: {:#?}", bin_data),
                    // This is deprecated use Payload::Text instead
                    Payload::String(str) => println!("Received: {}", str),
                }
            }
            .boxed()
        };
        let room_callback = |payload: Payload, _socket: SocketClient| {
            async move {
                println!("room event:");
                match payload {
                    Payload::Text(values) => println!("Received: {:#?}", values),
                    Payload::Binary(bin_data) => println!("Received bytes: {:#?}", bin_data),
                    // This is deprecated use Payload::Text instead
                    Payload::String(str) => println!("Received: {}", str),
                }
            }
            .boxed()
        };
        let error_callback = |payload: Payload, _socket: SocketClient| {
            async move {
                println!("service_error event:");
                match payload {
                    Payload::Text(values) => println!("Received: {:#?}", values),
                    Payload::Binary(bin_data) => println!("Received bytes: {:#?}", bin_data),
                    // This is deprecated use Payload::Text instead
                    Payload::String(str) => println!("Received: {}", str),
                }
            }
            .boxed()
        };
        let default_callback = |payload: Payload, _socket: SocketClient| {
            async move {
                println!("default event:");
                match payload {
                    Payload::Text(values) => println!("Received: {:#?}", values),
                    Payload::Binary(bin_data) => println!("Received bytes: {:#?}", bin_data),
                    // This is deprecated use Payload::Text instead
                    Payload::String(str) => println!("Received: {}", str),
                }
            }
            .boxed()
        };

        // Creates a GET request, upgrades and sends it.
        let token = self.token.clone().expect("No token");
        self.ws_client = Some(
            ClientBuilder::new("http://localhost:8080/")
                .namespace("/game")
                .auth(token)
                .on("hand", hand_callback)
                .on("room", room_callback)
                .on("service_error", error_callback)
                .on("error", default_callback)
                .connect()
                .await
                .expect("Connection failed"),
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
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
        if self.ws_client.is_none() {
            self.create_ws_connection().await;
        }
        let ws_socket = self.ws_client.as_ref().expect("No socket connection");
        ws_socket
            .emit(event.as_ref(), json!(payload))
            .await
            .expect("Server unreachable");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

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
