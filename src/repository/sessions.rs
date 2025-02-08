use eyre::Result;
use sqlx::types::Uuid;
use sqlx::PgPool;

#[derive(Clone)]
pub struct SessionRepository {
    pub pool: PgPool,
}

impl SessionRepository {
    pub async fn upsert(&self, user_id: Uuid, token: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO sessions (user_id, token)
            VALUES ($1, $2)
            ON CONFLICT (user_id) DO UPDATE SET token = $2
            "#,
        )
        .bind(user_id)
        .bind(token)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
