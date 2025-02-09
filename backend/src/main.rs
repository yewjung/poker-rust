use std::str::FromStr;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use env_logger;
use eyre::Result;
use log::{error, info};
use poker::Evaluator;
use refinery::config::Config;
use sqlx::PgPool;

use crate::domain::auth::{LoginRequest, SignupRequest};
use crate::error::Error;
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
    // setup log
    env_logger::init(); // Initialize the logger
    info!("server starts with logging");

    // run migrations
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is not set");
    let mut config = Config::from_str(&database_url)?;
    migrations::runner().run_async(&mut config).await?;
    let pool = PgPool::connect(&database_url).await.unwrap();

    // repositories
    let room_repository = RoomRepository::new();
    let user_repository = UserRepository::new(pool.clone());
    let auth_repository = AuthUserRepository::new(pool.clone());

    let api = routes::Api {
        game_service: GameService {
            evaluator: Evaluator::new(),
            room_repository,
            user_repository,
        },
        auth_service: AuthService { auth_repository },
    };
    let router = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/signup", post(signup))
        .route("/login", post(login))
        .layer(Extension(api));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, router).await.unwrap();
    Ok(())
}

async fn signup(
    Extension(api): Extension<routes::Api>,
    Json(payload): Json<SignupRequest>,
) -> impl IntoResponse {
    match api.signup(payload).await {
        Ok(_) => (StatusCode::CREATED, "User created".to_string()),
        Err(e) => report_into_response(e),
    }
}

async fn login(
    Extension(api): Extension<routes::Api>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    match api.login(payload).await {
        Ok(token) => (StatusCode::OK, token.to_string()),
        Err(e) => report_into_response(e),
    }
}


fn report_into_response(e: eyre::Report) -> (StatusCode, String) {
    error!("Error occurred: {:?}", e);
    match e.downcast::<Error>() {
        Ok(error) => error.into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "".to_string()),
    }
}