use crate::domain::room::Room;
use eyre::{ContextCompat, Result};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone)]
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
        self.get(id)
    }

    pub fn update(&mut self, id: Uuid, room: Room) -> Result<Room> {
        self.rooms.insert(id, room.clone());
        Ok(room)
    }

    pub fn get(&self, id: Uuid) -> Result<Room> {
        self.rooms
            .get(&id)
            .map(|r| r.clone())
            .wrap_err("Room not found")
    }
}
