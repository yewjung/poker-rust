use std::fmt::format;

use rand::distr::Alphanumeric;
use rand::{rng, Rng};

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
    let response = client.signup(request).await;
    assert!(response.is_ok());

    let response = client
        .login(LoginRequest {
            email,
            password: "password".to_string(),
        })
        .await;

    match response {
        Ok(ref token) => println!("Token: {}", token),
        Err(ref e) => println!("Error: {:?}", e),
    }

    assert!(response.is_ok());
    assert!(response.unwrap().len() > 0);
    Ok(())
}
