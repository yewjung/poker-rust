use eyre::{ContextCompat, Result};
use std::collections::HashMap;

use uuid::Uuid;

use crate::domain::auth::AuthUser;

#[derive(Clone)]
pub struct AuthUserRepository {
    pub users: HashMap<String, AuthUser>,
}

impl AuthUserRepository {
    pub fn new() -> Self {
        AuthUserRepository {
            users: HashMap::new(),
        }
    }

    pub fn create_user(&mut self, email: String, password: String) -> Result<AuthUser> {
        let user = AuthUser {
            user_id: Uuid::new_v4(),
            email,
            password,
        };
        self.users.insert(user.email.clone(), user.clone());
        Ok(user)
    }

    pub fn get(&self, email: String) -> Result<AuthUser> {
        self.users
            .get(&email)
            .map(|u| u.clone())
            .wrap_err("User not found")
    }

    pub fn exists(&self, email: String) -> bool {
        self.users.contains_key(&email)
    }
}
