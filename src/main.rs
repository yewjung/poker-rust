use crate::domain::auth::SignupRequest;
use crate::repository::auth::AuthUserRepository;
use crate::repository::rooms::RoomRepository;
use crate::repository::users::UserRepository;
use crate::service::auth::AuthService;
use crate::service::game::GameService;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Extension, Json, Router};
use poker::Evaluator;

mod domain;
mod error;
mod repository;
mod routes;
mod service;

#[tokio::main]
async fn main() {
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
    let router = Router::new();
    let router = router.route("/signup", post(signup)).layer(Extension(api));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, router).await.unwrap();
}

async fn signup(
    Extension(api): Extension<routes::Api>,
    Json(payload): Json<SignupRequest>,
) -> impl IntoResponse {
    (StatusCode::OK, "msg")
}
