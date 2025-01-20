use crate::domain::user::User;
use eyre::{ContextCompat, Result};
use std::collections::HashMap;
use uuid::Uuid;

pub struct UserRepository {
    pub users: HashMap<Uuid, User>,
}

impl UserRepository {
    pub fn new() -> Self {
        UserRepository {
            users: HashMap::new(),
        }
    }
    pub fn create_user(&mut self, name: String, balance: u32) -> Result<User> {
        let user = User {
            id: Uuid::new_v4(),
            name,
            balance,
        };
        self.users.insert(user.id, user.clone());
        Ok(user)
    }

    pub fn get(&self, id: Uuid) -> Result<User> {
        self.users
            .get(&id)
            .map(|u| u.clone())
            .wrap_err("User not found")
    }

    pub fn update(&mut self, id: Uuid, user: User) -> Result<()> {
        self.users.insert(id, user);
        Ok(())
    }
}
