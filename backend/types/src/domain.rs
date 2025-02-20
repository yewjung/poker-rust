use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use strum_macros::AsRefStr;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize)]
pub struct JoinGameRequest {
    pub room_id: Uuid,
    pub buy_in: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActionRequest {
    pub room_id: Uuid,
    pub action: Action,
}

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Fold,
    Check,
    Call,
    Raise(u32),
    AllIn,
}

#[derive(Debug, Clone, FromRow, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub balance: i64,
    pub current_room: Option<Uuid>,
}

#[derive(Debug, AsRefStr)]
#[strum(serialize_all = "snake_case")]
pub enum Event {
    Join,
    Action,
    Leave,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RoomInfo {
    pub room_id: Uuid,
    pub player_count: i32,
}
