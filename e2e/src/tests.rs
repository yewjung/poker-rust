use rand::distr::Alphanumeric;
use rand::{rng, Rng};
use reqwest::StatusCode;
use tap::TapFallible;

use client::client::Client;
use types::domain::{JoinGameRequest, LoginRequest, SignupRequest, UpdateProfileRequest, User};

#[tokio::test]
async fn test_signup_and_login() -> Result<(), reqwest::Error> {
    let mut client = Client::new();

    let email = random_email();
    let request = SignupRequest {
        email: email.clone(),
        password: "password".to_string(),
    };
    let response = client.signup(request).await?;
    assert_eq!(response.status(), StatusCode::CREATED);

    // login with correct password
    let login_status = client
        .login(LoginRequest {
            email: email.clone(),
            password: "password".to_string(),
        })
        .await
        .tap_err(|e| println!("Error: {:?}", e))?;

    assert_eq!(login_status, StatusCode::OK);

    // login with incorrect password
    let login_status = client
        .login(LoginRequest {
            email: email.clone(),
            password: "wrong_password".to_string(),
        })
        .await
        .tap_err(|e| println!("Error: {:?}", e))?;

    assert_eq!(login_status, StatusCode::UNAUTHORIZED);

    // test signup with the same email
    let request = SignupRequest {
        email,
        password: "password".to_string(),
    };
    let response = client.signup(request).await?;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // update profile
    let update_profile_request = UpdateProfileRequest {
        username: "new_username".to_string(),
    };
    let response = client
        .update_profile(update_profile_request)
        .await
        .tap_err(|e| println!("Error: {:?}", e))?;

    assert_eq!(response.status(), StatusCode::OK);
    let user: User = response.json().await?;
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
    let response = client.get_profile().await?;
    assert_eq!(response.status(), StatusCode::OK);
    let user: User = response.json().await?;
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

fn random_email() -> String {
    // generate a random email
    let random_string: String = rng()
        .sample_iter(&Alphanumeric)
        .take(6)
        .map(char::from)
        .collect();

    format!("{}@gmail.com", random_string)
}

#[tokio::test]
async fn test_join_game() -> eyre::Result<()> {
    let mut client = Client::new();
    let email = random_email();
    client
        .signup(SignupRequest {
            email: email.clone(),
            password: "password".to_string(),
        })
        .await?;

    let login_status = client
        .login(LoginRequest {
            email,
            password: "password".to_string(),
        })
        .await?;
    assert_eq!(login_status, StatusCode::OK);

    client
        .join_game(JoinGameRequest {
            room_id: Default::default(),
            buy_in: 100,
        })
        .await?;
    Ok(())
}
