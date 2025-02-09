use std::str::FromStr;
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, patch, post};
use axum::{Extension, Json, Router};
use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::Authorization;
use axum_extra::TypedHeader;
use eyre::Result;
use log::{error, info};
use poker::Evaluator;
use refinery::config::Config;
use sqlx::types::Uuid;
use sqlx::PgPool;
use socketioxide::{
    extract::SocketRef,
    SocketIo,
};
use crate::domain::auth::{AuthUser, LoginRequest, SignupRequest, UpdateProfileRequest};
use crate::error::Error;
use crate::repository::auth::AuthUserRepository;
use crate::repository::rooms::RoomRepository;
use crate::repository::users::UserRepository;
use crate::routes::Api;
use crate::service::auth::AuthService;
use crate::service::game::GameService;
use crate::service::users::UserService;

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
    let user_repository = Arc::new(UserRepository::new(pool.clone()));
    let auth_repository = AuthUserRepository::new(pool.clone());

    // API
    let api = Api {
        game_service: GameService {
            evaluator: Evaluator::new(),
            room_repository,
            user_repository: user_repository.clone(),
        },
        auth_service: AuthService { auth_repository },
        user_service: UserService { user_repository },
    };

    // setting up websocket
    let (layer, io) = SocketIo::new_layer();

    // Register a handler for the default namespace
    io.ns("/game", |s: SocketRef| {
        // For each "message" event received, send a "message-back" event with the "Hello World!" event
        s.on("message", |s: SocketRef| {
            s.emit("message-back", "Hello World!").ok();
        });
    });


    // routes
    let router = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/signup", post(signup))
        .route("/login", post(login))
        .route("/profile", patch(update_profile))
        .route("/profile", get(get_profile))
        .route("/game", get(|| async { "WebSocket endpoint at /game" }))
        .layer(layer)
        .layer(Extension(api));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, router).await.unwrap();
    Ok(())
}

async fn signup(
    Extension(api): Extension<Api>,
    Json(payload): Json<SignupRequest>,
) -> impl IntoResponse {
    match api.signup(payload).await {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(e) => report_into_response(e).into_response(),
    }
}

async fn login(
    Extension(api): Extension<Api>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    match api.login(payload).await {
        Ok(token) => (StatusCode::OK, token.to_string()),
        Err(e) => report_into_response(e),
    }
}

async fn update_profile(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(api): Extension<Api>,
    Json(payload): Json<UpdateProfileRequest>,
) -> impl IntoResponse {
    let auth_user = match get_auth_user(auth, &api).await {
        Ok(value) => value,
        Err(status_code) => return status_code.into_response(),
    };
    match api.update_profile(auth_user.id, payload).await {
        Ok(user) => (StatusCode::OK, Json(user)).into_response(),
        Err(e) => report_into_response(e).into_response(),
    }
}

async fn get_profile(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Extension(api): Extension<Api>,
) -> impl IntoResponse {
    let auth_user = match get_auth_user(auth, &api).await {
        Ok(value) => value,
        Err(status_code) => return status_code.into_response(),
    };
    match api.get_profile(auth_user.id).await {
        Ok(user) => (StatusCode::OK, Json(user)).into_response(),
        Err(e) => report_into_response(e).into_response(),
    }
}

async fn get_auth_user(auth: Authorization<Bearer>, api: &Api) -> Result<AuthUser, StatusCode> {
    let token = match Uuid::from_str(auth.token()) {
        Ok(token) => token,
        Err(_) => return Err(StatusCode::UNAUTHORIZED),
    };
    let auth_user = match api.get_user(token).await {
        Ok(Some(auth_user)) => auth_user,
        _ => return Err(StatusCode::UNAUTHORIZED),
    };
    Ok(auth_user)
}

fn report_into_response(e: eyre::Report) -> (StatusCode, String) {
    error!("Error occurred: {:?}", e);
    match e.downcast::<Error>() {
        Ok(error) => error.into_response_tuple(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "".to_string()),
    }
}
