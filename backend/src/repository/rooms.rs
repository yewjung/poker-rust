use dashmap::mapref::one::RefMut;
use dashmap::DashMap;
use eyre::{bail, Result};
use sqlx::types::Uuid;
use sqlx::PgPool;

use crate::domain::room::{Room, RoomInfo};
use crate::error::Error;

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

#[derive(Clone)]
pub struct RoomPlayerCountRepository {
    pub pool: PgPool,
}

impl RoomPlayerCountRepository {
    pub async fn get_all(&self) -> Result<Vec<RoomInfo>> {
        sqlx::query_as(
            r#"
            SELECT * as player_count FROM room_players
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn get_room_for_update(
        &self,
        room_id: Uuid,
    ) -> Result<(RoomInfo, sqlx::Transaction<'_, sqlx::Postgres>)> {
        let mut tx = self.pool.begin().await?;
        let room_info: Option<RoomInfo> = sqlx::query_as(
            r#"
            SELECT * FROM room_players
            WHERE room_id = $1
            FOR UPDATE
            "#,
        )
        .bind(room_id)
        .fetch_optional(&mut *tx)
        .await?;
        match room_info {
            None => {
                tx.rollback().await?;
                bail!(Error::InvalidRoomId);
            }
            Some(room_info) => Ok((room_info, tx)),
        }
    }

    pub async fn update(
        &self,
        room_id: Uuid,
        player_count: i32,
        mut tx: sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE room_info
            SET player_count = $1
            WHERE room_id = $2
            "#,
        )
        .bind(player_count)
        .bind(room_id)
        .execute(&mut *tx)
        .await?;
        Ok(())
    }
}
