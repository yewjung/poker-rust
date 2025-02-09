use rand::distr::Alphanumeric;
use rand::{rng, Rng};
use reqwest::StatusCode;
use tap::TapFallible;

use crate::client::Client;
use crate::payloads::{LoginRequest, SignupRequest};

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
            email,
            password: "wrong_password".to_string(),
        })
        .await
        .tap_err(|e| println!("Error: {:?}", e))?;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    Ok(())
}
