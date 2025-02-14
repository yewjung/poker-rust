use eyre::Result;
use sqlx::types::Uuid;
use validator::Validate;

use crate::domain::auth::{AuthUser, LoginRequest, SignupRequest, UpdateProfileRequest};
use crate::domain::request::JoinGameRequest;
use crate::domain::room::Room;
use crate::domain::user::User;
use crate::service::auth::AuthService;
use crate::service::game::GameService;
use crate::service::users::UserService;

#[derive(Clone)]
pub struct Api {
    pub game_service: GameService,
    pub auth_service: AuthService,
    pub user_service: UserService,
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

    pub async fn update_profile(
        &self,
        user_id: Uuid,
        request: UpdateProfileRequest,
    ) -> Result<User> {
        self.user_service
            .upsert_profile(user_id, request.username)
            .await
    }

    pub async fn get_user(&self, token: Uuid) -> Result<Option<AuthUser>> {
        self.auth_service.get_user(token).await
    }

    pub async fn get_profile(&self, user_id: Uuid) -> Result<Option<User>> {
        self.user_service.get(user_id).await
    }

    pub async fn join_game(&self, user_id: Uuid, request: JoinGameRequest) -> Result<Room> {
        todo!()
    }
}
