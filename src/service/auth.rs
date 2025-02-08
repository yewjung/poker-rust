use crate::domain::auth::AuthUser;
use crate::error::Error;
use crate::repository::auth::AuthUserRepository;

use crate::repository::sessions::SessionRepository;
use bcrypt::{hash, verify, DEFAULT_COST};
use eyre::{ensure, Result};
use sqlx::types::Uuid;

#[derive(Clone)]
pub struct AuthService {
    pub auth_repository: AuthUserRepository,
    pub session_repository: SessionRepository,
}

impl AuthService {
    pub async fn signup(&self, email: String, password: String) -> Result<AuthUser> {
        ensure!(
            self.auth_repository.exists(email.clone()).await?,
            Error::EmailAlreadyExists
        );
        let hashed_password = hash(password, DEFAULT_COST)?;
        self.auth_repository
            .create_user(email, hashed_password)
            .await
    }

    pub async fn login(&self, email: String, password: String) -> Result<Uuid> {
        let user = self.auth_repository.get(email.clone()).await?;
        ensure!(
            verify(password, &user.hashed_password)?,
            Error::InvalidPassword
        );
        let token = Uuid::new_v4();
        self.session_repository.upsert(user.user_id, token).await?;

        Ok(token)
    }
}
