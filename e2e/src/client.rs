use crate::domain::{LoginRequest, SignupRequest, UpdateProfileRequest};
use futures_util::FutureExt;
use reqwest::{Error, Response};
use rust_socketio::asynchronous::ClientBuilder;
use rust_socketio::Payload;
use serde_json::json;

pub struct Client {
    pub client: reqwest::Client,
}

const BASE_URL: &str = "http://localhost:8080";

impl Client {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
    pub async fn signup(&self, request: SignupRequest) -> Result<Response, Error> {
        let url = format!("{}/signup", BASE_URL);
        self.client.post(url).json(&request).send().await
    }

    pub async fn login(&self, request: LoginRequest) -> Result<Response, Error> {
        let url = format!("{}/login", BASE_URL);
        self.client.post(url).json(&request).send().await
    }

    pub async fn update_profile(
        &self,
        token: String,
        request: UpdateProfileRequest,
    ) -> Result<Response, Error> {
        let url = format!("{}/profile", BASE_URL);
        self.client
            .patch(url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&request)
            .send()
            .await
    }

    pub async fn get_profile(&self, token: String) -> Result<Response, Error> {
        let url = format!("{}/profile", BASE_URL);
        self.client
            .get(url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
    }

    pub async fn join_game(&mut self, token: String) -> eyre::Result<()> {
        let callback = |payload: Payload, _socket: rust_socketio::asynchronous::Client| {
            async move {
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
        let mut socket = ClientBuilder::new("http://localhost:8080/")
            .namespace("/game")
            .auth(token)
            .on("message-back", callback)
            .connect()
            .await
            .expect("Connection failed");

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        socket
            .emit("join", json!({"hello": true}))
            .await
            .expect("Server unreachable");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        Ok(())
    }
}
