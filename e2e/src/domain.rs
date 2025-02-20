use eyre::Result;

use client::client::Client;
use types::domain::{JoinGameRequest, User};

use crate::util::register_user;

pub struct TestUser {
    pub user: User,
    pub client: Client,
}

impl TestUser {
    pub async fn new() -> Result<Self> {
        register_user().await
    }

    pub async fn join_game(&mut self, request: JoinGameRequest) -> Result<()> {
        self.client.join_game(request).await
    }
}
