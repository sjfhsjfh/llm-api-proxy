mod token;

use std::{collections::HashMap, convert::Infallible, net::SocketAddr};

use axum::{
    Json, Router,
    body::Body,
    extract::Path,
    http::{HeaderMap, Response},
    response::{
        IntoResponse, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt};
use log::{info, warn};
use serde_json::Value;

use crate::token::AuthToken;

const API_ENDPOINT: &str = "https://openrouter.ai/api/v1";
const HOST_HEADER: &str = "openrouter.ai";

#[derive(Default)]
struct WaitForFixState {
    pub args: HashMap<String, String>,
    pub queue: Vec<eventsource_stream::Event>,
}

fn forward_event(e: eventsource_stream::Event) -> Event {
    let ev = Event::default().id(e.id).event(e.event).data(e.data);
    if let Some(retry) = e.retry {
        ev.retry(retry)
    } else {
        ev
    }
}

struct ProxyError {
    msg: String,
}

impl From<reqwest::Error> for ProxyError {
    fn from(err: reqwest::Error) -> Self {
        ProxyError {
            msg: format!("Proxy request error: {}", err),
        }
    }
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> axum::response::Response {
        axum::response::Response::builder()
            .status(axum::http::StatusCode::BAD_GATEWAY)
            .body(self.msg.into())
            .unwrap()
    }
}

async fn responses_api(
    token: AuthToken,
    Json(payload): Json<Value>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ProxyError> {
    log::info!("Received /responses request");
    let resp = reqwest::Client::new()
        .post(format!("{}/responses", API_ENDPOINT))
        .header("Host", HOST_HEADER)
        .header("Authorization", token.to_bearer())
        .json(&payload)
        .send()
        .await?;
    log::info!("Upstream response status: {}", resp.status());
    let resp = if resp.status().is_success() {
        resp
    } else {
        let status = resp.status();
        log::warn!("Error response from upstream, payload: {}", payload);
        log::warn!(
            "Upstream response body: {}",
            resp.text().await.unwrap_or_default()
        );
        return Err(ProxyError {
            msg: format!("Upstream returned error status: {}", status),
        });
    };
    let stream = resp.bytes_stream().eventsource();
    let transformed = stream
        .scan(WaitForFixState::default(), |state, event| {
            log::debug!("Received event: {:?}", event);
            if let Ok(event) = &event
                && let Ok(value) = serde_json::from_str::<Value>(&event.data)
                && let Some(type_) = value.get("type").and_then(|v| v.as_str())
                && (type_ == "response.output_item.added"
                    || type_ == "response.function_call_arguments.done"
                    || type_ == "response.output_item.done")
                && let Some(item) = value.get("item")
                && let Some("function_call") = item.get("type").and_then(|v| v.as_str())
                && let Some(call_id) = item.get("call_id").and_then(|v| v.as_str())
                && Some("") == item.get("arguments").and_then(|v| v.as_str())
            {
                log::info!("event type: {}", type_);
                log::info!(
                    "Get Malformed function call `{}` with empty arguments: {:?}",
                    call_id,
                    value
                );
                state.queue.push(event.clone());
                futures::future::ready(Some(Vec::new()))
            } else if let Ok(event) = &event
                && let Ok(value) = serde_json::from_str::<Value>(&event.data)
                && let Some(type_) = value.get("type").and_then(|v| v.as_str())
                && type_ == "response.completed"
                && let Some(resp) = value.get("response")
                && let Some(output) = resp.get("output")
                && let Some(output) = output.as_array()
            {
                log::info!("event type: {}", type_);
                for item in output {
                    if let Some(type_) = item.get("type").and_then(|v| v.as_str())
                        && type_ == "function_call"
                        && let Some(call_id) = item.get("call_id").and_then(|v| v.as_str())
                        && let Some(args) = item.get("arguments").and_then(|v| v.as_str())
                        && !args.is_empty()
                    {
                        log::info!("Found function call `{}` arguments: {}", call_id, args);
                        state.args.insert(call_id.to_string(), args.to_string());
                    }
                }
                let mut events = Vec::new();
                for queued in state.queue.drain(..) {
                    if let Ok(queued_value) = serde_json::from_str::<Value>(&queued.data)
                        && let Some(item) = queued_value.get("item")
                        && let Some("function_call") = item.get("type").and_then(|v| v.as_str())
                        && let Some(call_id) = item.get("call_id").and_then(|v| v.as_str())
                        && let Some(args) = state.args.get(call_id)
                    {
                        let mut fixed_item = item.clone();
                        fixed_item["arguments"] = Value::String(args.clone());
                        let mut fixed_event_value = queued_value.clone();
                        fixed_event_value["item"] = fixed_item;
                        let fixed_event = Event::default()
                            .id(queued.id.clone())
                            .event(queued.event.clone())
                            .data(fixed_event_value.to_string());
                        events.push(fixed_event);
                        continue;
                    }
                    log::warn!("Could not fix queued event: {:?}, sending as-is", queued);
                    events.push(forward_event(queued));
                }
                events.push(forward_event(event.clone()));

                futures::future::ready(Some(events))
            } else {
                if let Ok(event) = &event
                    && let Ok(value) = serde_json::from_str::<Value>(&event.data)
                    && let Some(type_) = value.get("type").and_then(|v| v.as_str())
                {
                    log::info!("event type: {}", type_);
                } else {
                    log::warn!("Received non-JSON or malformed event: {:?}", event);
                }
                futures::future::ready(Some(vec![forward_event(event.expect("Malformed event"))]))
            }
        })
        .flat_map(|events| futures::stream::iter(events.into_iter().map(Ok)));

    Ok(Sse::new(transformed).keep_alive(KeepAlive::default()))
}

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
        .route("/responses", post(responses_api))
        .route(
            "/{*rest}",
            get(default_get_handler).post(default_post_handler),
        )
}

#[tokio::main]
async fn main() {
    colog::init();
    let router = router();
    let addr = SocketAddr::from(([127, 0, 0, 1], 9723));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    info!("Listening on http://{}", addr);
    axum::serve(listener, router).await.unwrap();
}
