use axum::http::StatusCode;
use axum::response::IntoResponse;
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
}

impl Error {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Error::EmptyDeck => StatusCode::BAD_REQUEST,
            Error::InvalidPosition(_) => StatusCode::BAD_REQUEST,
            Error::EmailAlreadyExists => StatusCode::CONFLICT,
            Error::InvalidPassword => StatusCode::UNAUTHORIZED,
        }
    }

    pub fn into_response(self) -> (StatusCode, String) {
        (self.status_code(), self.to_string())
    }
}