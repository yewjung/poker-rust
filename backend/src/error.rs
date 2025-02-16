use axum::http::StatusCode;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Deck is empty")]
    EmptyDeck,
    #[error("Invalid position: {0}")]
    InvalidPosition(u64),
    #[error("Email already exists")]
    EmailAlreadyExists,
    #[error("Invalid password")]
    InvalidPassword,
    #[error("Insufficient balance")]
    InsufficientBalance,
    #[error("Room is full")]
    RoomIsFull,
    #[error("Invalid room id")]
    InvalidRoomId,
}

impl Error {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Error::EmptyDeck => StatusCode::BAD_REQUEST,
            Error::InvalidPosition(_) => StatusCode::BAD_REQUEST,
            Error::EmailAlreadyExists => StatusCode::CONFLICT,
            Error::InvalidPassword => StatusCode::UNAUTHORIZED,
            Error::InsufficientBalance => StatusCode::BAD_REQUEST,
            Error::RoomIsFull => StatusCode::BAD_REQUEST,
            Error::InvalidRoomId => StatusCode::NOT_FOUND,
        }
    }

    pub fn into_response_tuple(self) -> (StatusCode, String) {
        (self.status_code(), self.to_string())
    }
}
