use bcrypt::{hash, DEFAULT_COST};
use eyre::{ContextCompat, Result};
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
            INSERT INTO users (user_id, email, password)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(email)
        .bind(hashed_password)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn get(&self, email: String) -> Result<AuthUser> {
        sqlx::query_as(
            r#"
            SELECT * FROM users
            WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn exists(&self, email: String) -> Result<bool> {
        sqlx::query(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM users
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
}
