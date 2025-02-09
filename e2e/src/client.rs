use crate::payloads::{LoginRequest, SignupRequest};

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
    pub async fn signup(&self, request: SignupRequest) -> Result<(), reqwest::Error> {
        let url = format!("{}/signup", BASE_URL);
        self.client.post(url).json(&request).send().await?;
        Ok(())
    }

    pub async fn login(&self, request: LoginRequest) -> Result<String, reqwest::Error> {
        let url = format!("{}/login", BASE_URL);
        let response = self.client.post(url).json(&request).send().await?;
        response.text().await
    }
}
