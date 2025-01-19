use crate::domain::room::Room;
use eyre::{ContextCompat, Result};
use std::collections::HashMap;
use uuid::Uuid;

pub struct RoomRepository {
    rooms: HashMap<Uuid, Room>,
}

impl RoomRepository {
    pub fn new() -> Self {
        RoomRepository {
            rooms: HashMap::new(),
        }
    }

    pub fn insert(&mut self, room: Room) -> Result<Room> {
        let id = room.id.clone();
        self.rooms.insert(room.id, room);
        self.get(id).map(|r| r.clone()).wrap_err("Room not found")
    }

    pub fn update(&mut self, id: Uuid, room: Room) -> Result<()> {
        self.rooms.insert(id, room);
        Ok(())
    }

    pub fn get(&self, id: Uuid) -> Option<&Room> {
        self.rooms.get(&id)
    }
}
