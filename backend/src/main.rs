use std::str::FromStr;
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, patch, post};
use axum::{Extension, Json, Router};
use eyre::Result;
use log::{debug, error, info};
use poker::Evaluator;
use refinery::config::Config;
use socketioxide::extract::Extension as SocketExtension;
use socketioxide::extract::{Data, HttpExtension};
use socketioxide::{extract::SocketRef, SocketIo};
use sqlx::types::Uuid;
use sqlx::PgPool;

use types::domain::{
    ActionRequest, JoinGameRequest, LoginRequest, SignupRequest, UpdateProfileRequest,
};

use crate::error::Error;
use crate::extensions::ExtractUserFromToken;
use crate::repository::auth::AuthUserRepository;
use crate::repository::rooms::RoomRepository;
use crate::repository::users::UserRepository;
use crate::routes::Api;
use crate::service::auth::AuthService;
use crate::service::game::GameService;
use crate::service::users::UserService;

mod domain;
mod error;
mod extensions;
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

    // setting up websocket
    let (socket_layer, io) = SocketIo::new_layer();

    // Register a handler for the default namespace
    io.ns("/game", connection_handler);

    // API
    let api = Api {
        game_service: GameService {
            evaluator: Evaluator::new(),
            room_repository,
            user_repository: user_repository.clone(),
            io,
        },
        auth_service: AuthService { auth_repository },
        user_service: UserService { user_repository },
    };

    // routes
    let router = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/signup", post(signup))
        .route("/login", post(login))
        .route("/profile", patch(update_profile))
        .route("/profile", get(get_profile))
        .layer(socket_layer)
        .layer(Extension(api));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, router).await?;
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
    ExtractUserFromToken(user_id): ExtractUserFromToken,
    Extension(api): Extension<Api>,
    Json(payload): Json<UpdateProfileRequest>,
) -> impl IntoResponse {
    match api.update_profile(user_id, payload).await {
        Ok(user) => (StatusCode::OK, Json(user)).into_response(),
        Err(e) => report_into_response(e).into_response(),
    }
}

async fn get_profile(
    Extension(api): Extension<Api>,
    ExtractUserFromToken(user_id): ExtractUserFromToken,
) -> impl IntoResponse {
    match api.get_profile(user_id).await {
        Ok(Some(user)) => (StatusCode::OK, Json(user)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => report_into_response(e).into_response(),
    }
}

async fn join_game(
    s: SocketRef,
    SocketExtension(user_id): SocketExtension<Uuid>,
    Data(request): Data<JoinGameRequest>,
    HttpExtension(api): HttpExtension<Api>,
) {
    match api.join_game(user_id, request, s.id).await {
        Ok(room) => {
            debug!("User {} joined room {}", user_id, room.id);
            s.join(room.id.to_string());
        }
        Err(e) => {
            let (_, message) = report_into_response(e);
            let _ = s.emit("error", &message);
        }
    }
}

async fn take_action(
    s: SocketRef,
    SocketExtension(user_id): SocketExtension<Uuid>,
    Data(request): Data<ActionRequest>,
    HttpExtension(api): HttpExtension<Api>,
) {
    let action = request.action.clone();
    match api.take_action(user_id, request).await {
        Ok(room) => {
            debug!(
                "User {} took action, {:?} in room {}",
                user_id, action, room.id
            );
        }
        Err(e) => {
            let (_, message) = report_into_response(e);
            let _ = s.emit("error", &message);
        }
    }
}

async fn leave_game(
    s: SocketRef,
    SocketExtension(user_id): SocketExtension<Uuid>,
    HttpExtension(api): HttpExtension<Api>,
) {
    match api.game_service.leave_player(user_id).await {
        Ok(_) => debug!("User {} left the room", user_id),
        Err(e) => {
            let (_, message) = report_into_response(e);
            let _ = s.emit("error", &message);
        }
    }
}
async fn handle_disconnect(
    s: SocketRef,
    SocketExtension(user_id): SocketExtension<Uuid>,
    HttpExtension(api): HttpExtension<Api>,
) {
    debug!("User {} disconnected", user_id);
    leave_game(s, SocketExtension(user_id), HttpExtension(api)).await;
}

async fn connection_handler(
    s: SocketRef,
    Data(token): Data<Uuid>,
    HttpExtension(api): HttpExtension<Api>,
) {
    let user_id = match api.get_user_by_session_token(token).await {
        Ok(Some(auth_user)) => auth_user.id,
        _ => {
            error!("Failed to get user from token");
            return;
        }
    };
    debug!("User {} connected", user_id);
    s.extensions.insert(user_id);
    s.on("join", join_game);
    s.on("action", take_action);
    s.on("leave", leave_game);
    s.on_disconnect(handle_disconnect);
}

fn report_into_response(e: eyre::Report) -> (StatusCode, String) {
    error!("Error occurred: {:?}", e);
    match e.downcast::<Error>() {
        Ok(error) => error.into_response_tuple(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "".to_string()),
    }
}
