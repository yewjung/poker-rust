use bcrypt::{hash, verify, DEFAULT_COST};
use eyre::{ensure, Result};
use sqlx::types::Uuid;

use crate::domain::auth::AuthUser;
use crate::repository::auth::AuthUserRepository;
use types::error::Error;

#[derive(Clone)]
pub struct AuthService {
    pub auth_repository: AuthUserRepository,
}

impl AuthService {
    pub async fn signup(&self, email: String, password: String) -> Result<AuthUser> {
        ensure!(
            !self.auth_repository.exists(email.clone()).await?,
            Error::EmailAlreadyExists
        );
        let hashed_password = hash(password, DEFAULT_COST)?;
        self.auth_repository
            .create_user(email, hashed_password)
            .await
    }

    pub async fn login(&self, email: String, password: String) -> Result<Uuid> {
        let user = self.auth_repository.get(email).await?;
        ensure!(
            verify(password, &user.hashed_password)?,
            Error::InvalidPassword
        );
        let token = Uuid::new_v4();
        self.auth_repository.update_token(user.id, token).await?;

        Ok(token)
    }

    pub async fn get_user_by_session_token(&self, token: Uuid) -> Result<Option<AuthUser>> {
        self.auth_repository.get_by_session_token(token).await
    }
}
