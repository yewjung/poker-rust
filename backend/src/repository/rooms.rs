use dashmap::mapref::one::RefMut;
use dashmap::DashMap;
use uuid::Uuid;

use crate::domain::room::Room;

#[derive(Clone)]
pub struct RoomRepository {
    rooms: DashMap<Uuid, Room>,
}

impl RoomRepository {
    pub fn new() -> Self {
        RoomRepository {
            rooms: DashMap::new(),
        }
    }

    pub fn upsert(&mut self, room: Room) {
        self.rooms.insert(room.id, room);
    }

    pub fn get(&self, id: Uuid) -> Option<Room> {
        self.rooms.get(&id).map(|r| r.clone())
    }

    pub fn get_mut_lock(&self, id: Uuid) -> Option<RefMut<Uuid, Room>> {
        self.rooms.get_mut(&id)
    }
}
