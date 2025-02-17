use sqlx::types::chrono::{DateTime, Utc};
use sqlx::types::Uuid;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
    pub hashed_password: String,
    pub session_token: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
