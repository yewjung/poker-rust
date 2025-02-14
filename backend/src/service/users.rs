use eyre::Result;
use sqlx::types::Uuid;
use std::sync::Arc;

use crate::domain::user::User;
use crate::repository::users::UserRepository;

#[derive(Clone)]
pub struct UserService {
    pub user_repository: Arc<UserRepository>,
}

impl UserService {
    pub async fn upsert_profile(&self, user_id: Uuid, username: String) -> Result<User> {
        self.user_repository
            .upsert_user_with_username(user_id, username)
            .await
    }

    pub async fn get(&self, user_id: Uuid) -> Result<Option<User>> {
        self.user_repository.get(user_id).await
    }
}
