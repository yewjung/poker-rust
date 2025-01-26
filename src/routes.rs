use eyre::Result;

use crate::domain::auth::SignupRequest;
use crate::service::auth::AuthService;
use crate::service::game::GameService;

#[derive(Clone)]
pub struct Api {
    pub game_service: GameService,
    pub auth_service: AuthService,
}

impl Api {
    pub fn signup(&mut self, SignupRequest { email, password }: SignupRequest) -> Result<()> {
        self.auth_service.signup(email, password)?;
        Ok(())
    }
}
