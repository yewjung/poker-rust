use eyre::Result;
use tap::TapFallible;

use client::client::Client;
use types::domain::{JoinGameRequest, LoginRequest, SignupRequest, UpdateProfileRequest, User};

use crate::domain::TestUser;
use crate::util;

#[tokio::test]
async fn test_signup_and_login() -> Result<()> {
    let mut client = Client::new();

    let email = util::random_email();
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

    // login with incorrect password
    let login_result = client
        .login(LoginRequest {
            email: email.clone(),
            password: "wrong_password".to_string(),
        })
        .await;

    assert!(login_result.is_err());

    // test signup with the same email
    let request = SignupRequest {
        email,
        password: "password".to_string(),
    };
    let signup_result = client.signup(request).await;
    assert!(signup_result.is_err());

    // update profile
    let update_profile_request = UpdateProfileRequest {
        username: "new_username".to_string(),
    };
    let user = client
        .update_profile(update_profile_request)
        .await
        .tap_err(|e| println!("Error: {:?}", e))?;

    assert_eq!(
        user,
        User {
            id: user.id,
            name: "new_username".to_string(),
            balance: 1000,
            current_room: None,
        }
    );

    // get profile
    let user = client.get_profile().await?;
    assert_eq!(
        user,
        User {
            id: user.id,
            name: "new_username".to_string(),
            balance: 1000,
            current_room: None,
        }
    );
    Ok(())
}

#[tokio::test]
async fn test_join_game() -> Result<()> {
    let mut user = TestUser::new().await?;

    user.join_game(JoinGameRequest {
        room_id: Default::default(),
        buy_in: 100,
    })
    .await?;
    Ok(())
}
