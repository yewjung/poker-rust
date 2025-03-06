use eyre::Result;
use sqlx::types::Uuid;
use sqlx::Row;

use types::domain::User;

#[cfg_attr(test, faux::create)]
#[derive(Clone)]
pub struct UserRepository {
    pool: sqlx::PgPool,
}

const DEFAULT_BALANCE: i64 = 1000;

#[cfg_attr(test, faux::methods)]
impl UserRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    #[cfg(test)]
    pub async fn create_user(&self, name: String, balance: i64) -> Result<User> {
        sqlx::query_as(
            r#"
            INSERT INTO users (id, name, balance)
            VALUES ($1, $2, $3) RETURNING *
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(&name)
        .bind(balance)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn upsert_user_with_username(&self, id: Uuid, name: String) -> Result<User> {
        sqlx::query_as(
            r#"
            INSERT INTO users (id, name, balance)
            VALUES ($1, $2, $3)
            ON CONFLICT (id) DO UPDATE SET name = $2
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(&name)
        .bind(DEFAULT_BALANCE)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn get(&self, id: Uuid) -> Result<Option<User>> {
        sqlx::query_as(
            r#"
            SELECT * FROM users
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn update_balance_and_room(
        &self,
        id: Uuid,
        balance: i64,
        room_id: Uuid,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE users
            SET balance = $1, current_room = $2
            WHERE id = $3
            "#,
        )
        .bind(balance)
        .bind(room_id)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_player_room(
        &self,
        user_id: Uuid,
        room_id: Option<Uuid>,
    ) -> Result<Option<User>> {
        sqlx::query_as(
            r#"
            UPDATE users
            SET current_room = $1
            WHERE id = $2
            RETURNING *
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn is_user_in_room(&self, user_id: Uuid, room: Uuid) -> Result<bool> {
        sqlx::query(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM users
                WHERE id = $1 AND current_room = $2
            )
            "#,
        )
        .bind(user_id)
        .bind(room)
        .fetch_one(&self.pool)
        .await
        .map(|row| row.get(0))
        .map_err(Into::into)
    }
}
