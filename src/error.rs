pub struct ProxyError {
    pub msg: String,
}

impl ProxyError {
    pub fn new(msg: &str) -> Self {
        ProxyError {
            msg: msg.to_string(),
        }
    }
}

impl From<reqwest::Error> for ProxyError {
    fn from(err: reqwest::Error) -> Self {
        ProxyError {
            msg: format!("Proxy request error: {}", err),
        }
    }
}

impl axum::response::IntoResponse for ProxyError {
    fn into_response(self) -> axum::response::Response {
        axum::response::Response::builder()
            .status(axum::http::StatusCode::BAD_GATEWAY)
            .body(self.msg.into())
            .unwrap()
    }
}
