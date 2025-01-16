use std::sync::Arc;

use eyre::Result;
use poker::Evaluator;

use crate::domain::room::Room;

mod domain;
mod error;

fn main() -> Result<()> {
    // evaluator
    let evaluator = Arc::new(Evaluator::new());

    let mut room = Room::new("room1".to_string(), evaluator);
    room.add_player("Alice".to_string())?;
    room.add_player("Bob".to_string())?;

    for _ in 0..5 {
        room.deal_community_card()?;
    }
    room.winners()?;
    Ok(())
}
