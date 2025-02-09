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
