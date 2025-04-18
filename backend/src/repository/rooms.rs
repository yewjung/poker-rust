use std::sync::Arc;

use dashmap::mapref::one::RefMut;
use dashmap::DashMap;
use eyre::{bail, Result};
use sqlx::types::Uuid;
use sqlx::PgPool;

use types::domain::RoomInfo;
use types::error::Error;
use types::room::Room;

#[derive(Clone)]
pub struct RoomRepository {
    pub(crate) rooms: Arc<DashMap<Uuid, Room>>,
}

impl RoomRepository {
    pub fn new() -> Self {
        RoomRepository {
            rooms: Arc::new(DashMap::new()),
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

#[cfg_attr(test, faux::create)]
#[derive(Clone)]
pub struct RoomInfoRepository {
    pool: PgPool,
}
#[cfg_attr(test, faux::methods)]
impl RoomInfoRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub async fn get_all(&self) -> Result<Vec<RoomInfo>> {
        sqlx::query_as(
            r#"
            SELECT * FROM room_info
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
            SELECT * FROM room_info
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
        tx.commit().await.map_err(Into::into)
    }

    pub async fn zero_all_player_counts(&self) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE room_info
            SET player_count = 0
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
