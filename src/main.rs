use std::str::FromStr;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use eyre::Result;
use poker::Evaluator;
use refinery::config::Config;

use crate::domain::auth::SignupRequest;
use crate::repository::auth::AuthUserRepository;
use crate::repository::rooms::RoomRepository;
use crate::repository::users::UserRepository;
use crate::service::auth::AuthService;
use crate::service::game::GameService;

mod domain;
mod error;
mod repository;
mod routes;
mod service;

refinery::embed_migrations!("migrations");

#[tokio::main]
async fn main() -> Result<()> {
    // run migrations
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is not set");
    let mut config = Config::from_str(&database_url)?;
    migrations::runner().run_async(&mut config).await?;

    // repositories
    let room_repository = RoomRepository::new();
    let user_repository = UserRepository::new();

    let api = routes::Api {
        game_service: GameService {
            evaluator: Evaluator::new(),
            room_repository,
            user_repository,
        },
        auth_service: AuthService {
            auth_repository: AuthUserRepository::new(),
        },
    };
    let router = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/signup", post(signup))
        .layer(Extension(api));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, router).await.unwrap();
    Ok(())
}

async fn signup(
    Extension(api): Extension<routes::Api>,
    Json(payload): Json<SignupRequest>,
) -> impl IntoResponse {
    (StatusCode::OK, "msg")
}
