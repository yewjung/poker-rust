use std::sync::Arc;
use std::time::Duration;

use dashmap::DashSet;
use eyre::Result;
use lazy_static::lazy_static;
use tap::TapFallible;
use tokio::time::sleep;
use uuid::Uuid;

use client::client::Client;
use types::domain::{
    JoinGameRequest, LoginRequest, RoomInfo, SignupRequest, UpdateProfileRequest, User,
};

use crate::domain::TestUser;
use crate::util;

lazy_static! {
    static ref room_map: Arc<DashSet<Uuid>> = Arc::new(DashSet::new());
}

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

    let rooms = user.client.get_rooms().await?;

    let room_id = get_empty_room_id(rooms).await;

    user.client
        .join_game(JoinGameRequest {
            room_id,
            buy_in: 100,
        })
        .await?;
    sleep(Duration::from_secs(1)).await;

    // check if the player count is 1
    let rooms = user.client.get_rooms().await?;
    let room = rooms.iter().find(|r| r.room_id == room_id).unwrap();
    assert_eq!(room.player_count, 1);
    drop(user);

    // make sure the player count is 0
    sleep(Duration::from_secs(1)).await;
    let mut new_user = TestUser::new().await?;
    let rooms = new_user.client.get_rooms().await?;
    let room = rooms.iter().find(|r| r.room_id == room_id).unwrap();
    assert_eq!(room.player_count, 0);

    // joining another player to the same room, player count should be 1
    new_user
        .client
        .join_game(JoinGameRequest {
            room_id,
            buy_in: 100,
        })
        .await?;
    sleep(Duration::from_secs(1)).await;
    let rooms = new_user.client.get_rooms().await?;
    let room = rooms.iter().find(|r| r.room_id == room_id).unwrap();
    assert_eq!(room.player_count, 1);
    drop(new_user);

    // make sure the player count is 0
    sleep(Duration::from_secs(1)).await;
    let new_user = TestUser::new().await?;
    println!("new user: {}", new_user.user_id().unwrap());
    let rooms = new_user.client.get_rooms().await?;
    let room = rooms.iter().find(|r| r.room_id == room_id).unwrap();
    assert_eq!(room.player_count, 0);
    Ok(())
}

#[tokio::test]
async fn test_2_players_join_game() -> Result<()> {
    let mut user1 = TestUser::new().await?;
    println!("user1: {}", user1.client.user.as_ref().unwrap().id);
    let mut user2 = TestUser::new().await?;
    println!("user2: {}", user2.client.user.as_ref().unwrap().id);

    let rooms = user1.client.get_rooms().await?;

    let room_id = get_empty_room_id(rooms).await;

    user1
        .client
        .join_game(JoinGameRequest {
            room_id,
            buy_in: 100,
        })
        .await?;
    sleep(Duration::from_secs(1)).await;

    user2
        .client
        .join_game(JoinGameRequest {
            room_id,
            buy_in: 100,
        })
        .await?;
    sleep(Duration::from_secs(1)).await;

    // check if the player count is 2
    let rooms = user1.client.get_rooms().await?;
    let room = rooms.iter().find(|r| r.room_id == room_id).unwrap();
    println!("waiting for 2 players to join");
    assert_eq!(room.player_count, 2);

    Ok(())
}

async fn get_empty_room_id(rooms: Vec<RoomInfo>) -> Uuid {
    let mut already_used = true;
    let mut empty_room_id = Uuid::default();
    while already_used {
        let room_id = rooms
            .iter()
            .find(|r| !room_map.contains(&r.room_id) && r.player_count == 0)
            .unwrap()
            .room_id;
        empty_room_id = room_id;
        already_used = !room_map.insert(room_id);
    }
    println!("empty room id: {}", empty_room_id);
    empty_room_id
}
