use eyre::Result;
use uuid::Uuid;

use client::client::Client;
use types::domain::User;

use crate::util::register_user;

pub struct TestUser {
    pub client: Client,
}

impl TestUser {
    pub async fn new() -> Result<Self> {
        register_user().await
    }

    fn user(&self) -> Option<&User> {
        self.client.user.as_ref()
    }

    pub fn user_id(&self) -> Option<Uuid> {
        self.user().map(|user| user.id)
    }
}
