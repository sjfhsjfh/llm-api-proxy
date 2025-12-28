mod error;
mod responses_fix;
mod token;

use std::net::SocketAddr;

use axum::{
    Json, Router,
    body::Body,
    extract::Path,
    http::{HeaderMap, Response},
    routing::{get, post},
};
use log::{info, warn};
use serde_json::Value;

use crate::token::AuthToken;

const API_ENDPOINT: &str = "https://openrouter.ai/api/v1";
const HOST_HEADER: &str = "openrouter.ai";

async fn default_get_handler(Path(rest): Path<String>, headers: HeaderMap) -> Response<Body> {
    warn!("Unknown GET request for path: /{}", rest);
    warn!("Headers: {:?}", headers);
    let resp = match reqwest::Client::new()
        .get(format!("{}/{}", API_ENDPOINT, rest))
        .header("Host", HOST_HEADER)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(err) => {
            warn!("Error forwarding GET request: {}", err);
            return Response::new("Error forwarding request".into());
        }
    };
    let mut response_builder = Response::builder().status(resp.status());
    *response_builder.headers_mut().unwrap() = resp.headers().clone();
    response_builder
        .body(Body::from_stream(resp.bytes_stream()))
        .unwrap()
}

async fn default_post_handler(
    Path(rest): Path<String>,
    token: AuthToken,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response<Body> {
    warn!("Unknown POST request for path: /{}", rest);
    warn!("Headers: {:?}", headers);
    warn!("Token: {}", token.to_bearer());
    warn!("Payload: {}", payload);
    let resp = match reqwest::Client::new()
        .post(format!("{}/{}", API_ENDPOINT, rest))
        .header("Host", HOST_HEADER)
        .header("Authorization", token.to_bearer())
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(err) => {
            warn!("Error forwarding POST request: {}", err);
            return Response::new("Error forwarding request".into());
        }
    };
    let mut response_builder = Response::builder().status(resp.status());
    *response_builder.headers_mut().unwrap() = resp.headers().clone();
    let data = resp.bytes().await.unwrap_or_default();
    info!("Response Data: {:?}", data);
    response_builder.body(Body::from(data)).unwrap()
}

fn router() -> Router {
    Router::new()
        .route("/responses", post(responses_fix::post_responses_fix))
        .route(
            "/{*rest}",
            get(default_get_handler).post(default_post_handler),
        )
}

#[tokio::main]
async fn main() {
    colog::init();
    let router = router();
    let addr = SocketAddr::from(([0, 0, 0, 0], 9723));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    info!("Listening on http://{}", addr);
    axum::serve(listener, router).await.unwrap();
}
