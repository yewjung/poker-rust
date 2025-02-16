use std::str::FromStr;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::Extension;
use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::Authorization;
use axum_extra::TypedHeader;
use log::{error, info};
use tap::TapFallible;
use uuid::Uuid;

use crate::routes::Api;

#[derive(Debug, Clone)]
pub struct ExtractUserFromToken(pub Uuid);

impl<S> FromRequestParts<S> for ExtractUserFromToken
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(req: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        info!("Extracting user from token");
        let TypedHeader(Authorization(bearer)) =
            TypedHeader::<Authorization<Bearer>>::from_request_parts(req, state)
                .await
                .tap_err(|e| error!("Failed to extract Authorization header: {}", e))
                .map_err(|_| StatusCode::UNAUTHORIZED)?;

        let token = Uuid::from_str(bearer.token())
            .tap_err(|e| error!("Failed to parse token: {}", e))
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        let Extension(api) = Extension::<Api>::from_request_parts(req, state)
            .await
            .tap_err(|e| error!("Failed to extract API: {}", e))
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        let auth_user = match api.get_user_by_session_token(token).await {
            Ok(Some(auth_user)) => auth_user,
            _ => {
                error!("Failed to get user from token");
                return Err(StatusCode::UNAUTHORIZED);
            }
        };
        Ok(ExtractUserFromToken(auth_user.id))
    }
}
