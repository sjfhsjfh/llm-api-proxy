use std::{collections::HashMap, convert::Infallible};

use axum::{
    Json,
    response::{Sse, sse::KeepAlive},
};
use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt};
use serde_json::Value;

use crate::{API_ENDPOINT, HOST_HEADER, error::ProxyError, token::AuthToken};

#[derive(Default)]
struct WaitForFixState {
    pub args: HashMap<String, String>,
    pub queue: Vec<eventsource_stream::Event>,
}

fn forward_event(e: eventsource_stream::Event) -> axum::response::sse::Event {
    let ev = axum::response::sse::Event::default()
        .id(e.id)
        .event(e.event)
        .data(e.data);
    if let Some(retry) = e.retry {
        ev.retry(retry)
    } else {
        ev
    }
}

fn payload_pretransform(payload: &Value) -> Value {
    let mut transformed = payload.clone();
    // For OpenRouter compatibility,
    // delete the .type == "reasoning" item in the .input array
    if let Some(input) = transformed.get_mut("input").and_then(|v| v.as_array_mut()) {
        input.retain(|item| {
            if let Some(type_) = item.get("type").and_then(|v| v.as_str()) {
                type_ != "reasoning"
            } else {
                true
            }
        });
    } else {
        log::warn!("Payload does not contain an input array: {}", payload);
    }
    transformed
}

pub async fn post_responses_fix(
    token: AuthToken,
    Json(payload): Json<Value>,
) -> Result<Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>>, ProxyError> {
    log::debug!("Received /responses request");
    let transformed_payload = payload_pretransform(&payload);
    let resp = reqwest::Client::new()
        .post(format!("{}/responses", API_ENDPOINT))
        .header("Host", HOST_HEADER)
        .header("Authorization", token.to_bearer())
        .json(&transformed_payload)
        .send()
        .await?;
    log::debug!("Upstream response status: {}", resp.status());
    let resp = if resp.status().is_success() {
        resp
    } else {
        let status = resp.status();
        log::warn!(
            "Error response from upstream, payload: {}, transformed_payload: {}",
            payload,
            transformed_payload
        );
        log::warn!(
            "Upstream response body: {}",
            resp.text().await.unwrap_or_default()
        );
        return Err(ProxyError::new(&format!(
            "Upstream returned error status: {}",
            status
        )));
    };
    let stream = resp.bytes_stream().eventsource();
    let transformed = stream
        .scan(WaitForFixState::default(), |state, event| {
            log::debug!("Received event: {:?}", event);

            if let Ok(event) = &event
                && event.data == "[DONE]"
            {
                log::debug!("Received [DONE] event, flushing queue for safety");
                let done_event = forward_event(event.clone());
                let mut events = state.queue.drain(..).map(forward_event).collect::<Vec<_>>();
                if !events.is_empty() {
                    log::warn!("Flushing {} queued events before DONE", events.len());
                }
                events.push(done_event);
                return futures::future::ready(Some(events));
            }

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
                log::debug!("event type: {}", type_);
                log::debug!(
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
                log::debug!("event type: {}", type_);
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
                        let fixed_event = axum::response::sse::Event::default()
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
                    log::debug!("event type: {}", type_);
                } else {
                    log::warn!("Received non-JSON or malformed event: {:?}", event);
                }
                futures::future::ready(Some(vec![forward_event(event.expect("Malformed event"))]))
            }
        })
        .flat_map(|events| futures::stream::iter(events.into_iter().map(Ok)));

    Ok(Sse::new(transformed).keep_alive(KeepAlive::default()))
}
