use std::sync::Arc;

use eyre::Result;
use poker::Evaluator;

use crate::domain::room::Room;
use crate::service::game::GameService;

mod domain;
mod error;
mod service;

fn main() -> Result<()> {
    // evaluator
    let evaluator = Evaluator::new();

    // service
    let game_service = GameService { evaluator };

    let mut room = Room::new("room1".to_string());
    room.add_player("Alice".to_string(), 500)?;
    room.add_player("Bob".to_string(), 500)?;

    for _ in 0..5 {
        room.deal_community_card()?;
    }
    game_service.find_winners(&room)?;
    Ok(())
}
