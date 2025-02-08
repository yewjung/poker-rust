use crate::domain::user::User;
use eyre::Result;
use sqlx::types::Uuid;

#[cfg_attr(test, faux::create)]
#[derive(Clone)]
pub struct UserRepository {
    pool: sqlx::PgPool,
}

#[cfg_attr(test, faux::methods)]
impl UserRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_user(&mut self, name: String, balance: i64) -> Result<User> {
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

    pub async fn get(&self, id: Uuid) -> Result<User> {
        sqlx::query_as(
            r#"
            SELECT * FROM users
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn update(&mut self, id: Uuid, user: User) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE users
            SET name = $1, balance = $2
            WHERE id = $3
            "#,
        )
        .bind(&user.name)
        .bind(user.balance)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_balance(&mut self, id: Uuid, balance: i64) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE users
            SET balance = $1
            WHERE id = $2
            "#,
        )
        .bind(balance)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
