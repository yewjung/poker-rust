use rand::distr::Alphanumeric;
use rand::{rng, Rng};
use reqwest::StatusCode;
use tap::TapFallible;

use crate::client::Client;
use crate::domain::{LoginRequest, SignupRequest, UpdateProfileRequest, User};

#[tokio::test]
async fn test_signup_and_login() -> Result<(), reqwest::Error> {
    let client = Client::new();
    // generate a random email
    let random_string: String = rng()
        .sample_iter(&Alphanumeric)
        .take(6)
        .map(char::from)
        .collect();

    let email = format!("{}@gmail.com", random_string);
    let request = SignupRequest {
        email: email.clone(),
        password: "password".to_string(),
    };
    let response = client.signup(request).await?;
    assert_eq!(response.status(), StatusCode::CREATED);

    // login with correct password
    let response = client
        .login(LoginRequest {
            email: email.clone(),
            password: "password".to_string(),
        })
        .await
        .tap_err(|e| println!("Error: {:?}", e))?;

    assert_eq!(response.status(), StatusCode::OK);

    let token = response.text().await?;
    println!("Token: {}", token);

    assert!(!token.is_empty());

    // login with incorrect password
    let response = client
        .login(LoginRequest {
            email: email.clone(),
            password: "wrong_password".to_string(),
        })
        .await
        .tap_err(|e| println!("Error: {:?}", e))?;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

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
        .update_profile(token.clone(), update_profile_request)
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
    let response = client.get_profile(token).await?;
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
