use axum::{extract::Request, middleware::Next, response::Response};
use jsonwebtoken::{decode, DecodingKey, Validation};
use crate::{errors::AppError, models::Claims};

pub async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match token {
        Some(t) => {
            let claims = decode::<Claims>(
                t,
                &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
                &Validation::default(),
            )
            .map_err(|_| AppError::Unauthorized)?
            .claims;
            request.extensions_mut().insert(claims);
            Ok(next.run(request).await)
        }
        None => Err(AppError::Unauthorized),
    }
}
