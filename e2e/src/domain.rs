use eyre::Result;

use client::client::Client;
use types::domain::User;

use crate::util::register_user;

pub struct TestUser {
    pub user: User,
    pub client: Client,
}

impl TestUser {
    pub async fn new() -> Result<Self> {
        register_user().await
    }
}
