use std::str::FromStr;

use eyre::{ensure, ContextCompat, Result};
use socketioxide::socket::Sid;
use sqlx::types::Uuid;
use validator::Validate;

use types::domain::{
    ActionRequest, JoinGameRequest, LoginRequest, SignupRequest, UpdateProfileRequest, User,
};
use types::error::Error;
use types::room::Room;

use crate::domain::auth::AuthUser;
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
        request
            .validate()
            .map_err(|_| Error::InvalidEmailOrPassword)?;
        self.auth_service
            .signup(request.email, request.password)
            .await?;
        Ok(())
    }

    pub async fn login(&self, request: LoginRequest) -> Result<Uuid> {
        request
            .validate()
            .map_err(|_| Error::InvalidEmailOrPassword)?;
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

    pub async fn connect_player_by_token(&self, token: Uuid, sid: Sid) -> Result<Option<AuthUser>> {
        // get user by token, disconnect user from old socket, update socket id
        let user = self
            .get_user_by_session_token(token)
            .await?
            .wrap_err("User not found")?;

        if let Some(old_sid) = user.sid {
            let old_sid = Sid::from_str(&old_sid)?;
            // remove player from old room
            self.game_service.leave_player(user.id, old_sid).await?;
            // break old connection
            self.game_service.disconnect_socket(old_sid)?;
        }
        self.auth_service.update_sid(user.id, sid).await
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
            .join_player(request.room_id, user_id, request.buy_in, sid)
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
