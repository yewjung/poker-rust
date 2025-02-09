use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::types::Uuid;
use sqlx::FromRow;
use validator::Validate;

#[derive(Debug, Validate, Deserialize, Serialize)]
pub struct SignupRequest {
    #[validate(email)]
    pub email: String,
    pub password: String,
}

#[derive(Debug, Validate, Deserialize, Serialize)]
pub struct LoginRequest {
    #[validate(email)]
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateProfileRequest {
    pub username: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
    pub hashed_password: String,
    pub session_token: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
