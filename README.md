# llm-api-proxy

[简体中文](README_zh.md) · English

A lightweight Axum-based reverse proxy that fronts OpenRouter's API. It fixes two Codex/OpenRouter interoperability issues while forwarding other traffic transparently.

## Features
- **Function-call argument repair**: `/responses` fixes SSE events where `function_call` items lose their `arguments` (observed when OpenRouter drops them unexpectedly).
- **Reasoning filter for Codex**: Strips `type == "reasoning"` entries from the `input` array before forwarding, preventing upstream `ZodError` responses.
- **Transparent proxying**: Any other GET/POST path is forwarded verbatim to `https://openrouter.ai/api/v1`, preserving headers and status codes.

## Quick Start
```bash
# build and run (debug)
cargo run
# server listens on 0.0.0.0:9723
```

Send requests with an OpenRouter API token in the `Authorization: Bearer <token>` header.

Example POST to `/responses` (streams Server-Sent Events):
```bash
curl -N \
  -H "Authorization: Bearer $OPENROUTER_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"input":[{"type":"text","content":"hello"}]}' \
  http://localhost:9723/responses
```

## API Notes
- Any path other than `/responses` is forwarded verbatim to the upstream API.
- SSE events from `/responses` are passed through with fixed `function_call` argument payloads when possible; a `[DONE]` event flushes any queued fixes.

## Development
- Rust 2024 edition; dependencies managed via Cargo (`axum`, `tokio`, `reqwest`, `serde_json`, etc.).
- Preferred workflow: `cargo fmt`, `cargo clippy --all-targets --all-features`, `cargo test` before committing.
- Logs use `colog` + `log` crate; adjust verbosity with `RUST_LOG` if needed.

## Deployment Tips
- Place behind your own ingress/reverse proxy when exposed publicly.
- Avoid logging full bearer tokens; only shortened values should appear in debug logs.
