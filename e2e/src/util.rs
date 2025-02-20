use rand::distr::Alphanumeric;
use rand::{rng, Rng};
use tap::TapFallible;

use client::client::Client;
use types::domain::{LoginRequest, SignupRequest, UpdateProfileRequest};

use crate::domain::TestUser;

async fn register_user() -> eyre::Result<TestUser> {
    let mut client = Client::new();

    let email = random_email();
    let request = SignupRequest {
        email: email.clone(),
        password: "password".to_string(),
    };
    client.signup(request).await?;

    // login with correct password
    client
        .login(LoginRequest {
            email: email.clone(),
            password: "password".to_string(),
        })
        .await
        .tap_err(|e| println!("Error: {:?}", e))?;

    let user = client
        .update_profile(UpdateProfileRequest {
            username: "username".to_string(),
        })
        .await?;

    Ok(TestUser { user, client })
}

pub fn random_email() -> String {
    // generate a random email
    let random_string: String = rng()
        .sample_iter(&Alphanumeric)
        .take(6)
        .map(char::from)
        .collect();

    format!("{}@gmail.com", random_string)
}
