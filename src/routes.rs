use eyre::Result;
use sqlx::types::Uuid;
use validator::Validate;

use crate::domain::auth::{LoginRequest, SignupRequest};
use crate::service::auth::AuthService;
use crate::service::game::GameService;

#[derive(Clone)]
pub struct Api {
    pub game_service: GameService,
    pub auth_service: AuthService,
}

impl Api {
    pub async fn signup(&self, request: SignupRequest) -> Result<()> {
        request.validate()?;
        self.auth_service
            .signup(request.email, request.password)
            .await?;
        Ok(())
    }

    pub async fn login(&self, request: LoginRequest) -> Result<Uuid> {
        request.validate()?;
        self.auth_service
            .login(request.email, request.password)
            .await
    }
}
