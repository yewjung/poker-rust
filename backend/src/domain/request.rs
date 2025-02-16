use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct JoinGameRequest {
    pub room_id: Uuid,
    pub buy_in: i64,
}
