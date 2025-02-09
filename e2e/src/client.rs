use reqwest::{Error, Response};

use crate::domain::{LoginRequest, SignupRequest, UpdateProfileRequest};

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

    pub async fn update_profile(&self, token: String, request: UpdateProfileRequest) -> Result<Response, Error> {
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
}
