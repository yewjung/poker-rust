use crate::domain::auth::AuthUser;
use crate::error::Error;
use crate::repository::auth::AuthUserRepository;
use eyre::{ensure, Result};

#[derive(Clone)]
pub struct AuthService {
    pub auth_repository: AuthUserRepository,
}

impl AuthService {
    pub fn signup(&mut self, email: String, password: String) -> Result<AuthUser> {
        ensure!(
            !self.auth_repository.exists(email.clone()),
            Error::EmailAlreadyExists
        );
        self.auth_repository.create_user(email, password)
    }
}
