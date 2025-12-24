use axum::{extract::FromRequestParts, response::IntoResponse};

pub struct AuthToken(pub String);

impl AuthToken {
    pub fn to_bearer(&self) -> String {
        format!("Bearer {}", self.0)
    }
}

pub struct NoToken;

impl IntoResponse for NoToken {
    fn into_response(self) -> axum::response::Response {
        axum::response::Response::builder()
            .status(axum::http::StatusCode::UNAUTHORIZED)
            .body("Token Not Provided".into())
            .unwrap()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for AuthToken {
    type Rejection = NoToken;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        if let Some(auth_header) = parts.headers.get("Authorization")
            && let Ok(auth_str) = auth_header.to_str()
            && auth_str.starts_with("Bearer ")
        {
            let token = auth_str.trim_start_matches("Bearer ").to_string();
            return Ok(AuthToken(token));
        }
        Err(NoToken)
    }
}
