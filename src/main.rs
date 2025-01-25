use eyre::Result;
use poker::Evaluator;

use crate::domain::room::{Player, Room};
use crate::repository::rooms::RoomRepository;
use crate::repository::users::UserRepository;
use crate::service::game::GameService;

mod domain;
mod error;
mod repository;
mod service;

fn main() -> Result<()> {
    // evaluator
    let evaluator = Evaluator::new();

    // repository
    let room_repository = RoomRepository::new();
    let user_repository = UserRepository::new();

    // service
    let game_service = GameService {
        evaluator,
        room_repository,
        user_repository,
    };

    let mut room = Room::new();
    room.join_player(Player::new("Alice".to_string(), 500))?;
    room.join_player(Player::new("Bob".to_string(), 500))?;

    Ok(())
}
