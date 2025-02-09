use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
pub struct SignupRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, Serialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, Serialize)]
pub struct UpdateProfileRequest {
    pub username: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub balance: i64,
    pub current_room: Option<Uuid>,
}
