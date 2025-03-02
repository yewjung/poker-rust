use eyre::Result;
use sqlx::types::Uuid;
use sqlx::{PgPool, Row};

use crate::domain::auth::AuthUser;

#[derive(Clone)]
pub struct AuthUserRepository {
    pool: PgPool,
}

impl AuthUserRepository {
    pub fn new(pool: PgPool) -> Self {
        AuthUserRepository { pool }
    }

    pub async fn create_user(&self, email: String, hashed_password: String) -> Result<AuthUser> {
        sqlx::query_as(
            r#"
            INSERT INTO auth_users (id, email, hashed_password)
            VALUES ($1, $2, $3) RETURNING *
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(email)
        .bind(hashed_password)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn get(&self, email: String) -> Result<Option<AuthUser>> {
        sqlx::query_as(
            r#"
            SELECT * FROM auth_users
            WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn exists(&self, email: String) -> Result<bool> {
        sqlx::query(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM auth_users
                WHERE email = $1
            )
            "#,
        )
        .bind(email)
        .fetch_one(&self.pool)
        .await
        .map(|row| row.get(0))
        .map_err(Into::into)
    }

    pub async fn update_token(&self, user_id: Uuid, token: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE auth_users
            SET session_token = $1
            WHERE id = $2
            "#,
        )
        .bind(token)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_by_session_token(&self, token: Uuid) -> Result<Option<AuthUser>> {
        sqlx::query_as(
            r#"
            SELECT * FROM auth_users
            WHERE session_token = $1
            "#,
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }
}
