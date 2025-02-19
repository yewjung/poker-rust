use eyre::{ensure, Result};
use socketioxide::socket::Sid;
use sqlx::types::Uuid;
use validator::Validate;

use types::domain::{
    ActionRequest, JoinGameRequest, LoginRequest, SignupRequest, UpdateProfileRequest, User,
};

use crate::domain::auth::AuthUser;
use crate::domain::room::Room;
use crate::error::Error;
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

    pub async fn get_user_by_session_token(&self, token: Uuid) -> Result<Option<AuthUser>> {
        self.auth_service.get_user_by_session_token(token).await
    }

    pub async fn get_profile(&self, user_id: Uuid) -> Result<Option<User>> {
        self.user_service.get(user_id).await
    }

    pub async fn join_game(
        &self,
        user_id: Uuid,
        request: JoinGameRequest,
        sid: Sid,
    ) -> Result<Room> {
        self.game_service
            .join_player(user_id, request.room_id, request.buy_in, sid)
            .await
    }

    pub async fn take_action(&self, user_id: Uuid, request: ActionRequest) -> Result<Room> {
        ensure!(
            self.user_service
                .is_user_in_room(user_id, request.room_id)
                .await?,
            Error::NotInRoom
        );
        self.game_service
            .take_action(request.room_id, user_id, request.action)
            .await
    }
}
