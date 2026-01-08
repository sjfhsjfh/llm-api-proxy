# Repository Guidelines

This document is a contributor-oriented overview for the **llm-api-proxy** project. It focuses on expectations and conventions rather than step-by-step build instructions (see `README.md` for detailed commands).

## Project Structure & Modules
- `src/` — core Rust modules: `main.rs` (Axum HTTP proxy entry), `responses_fix.rs` (SSE response corrections), `token.rs` (auth extraction), `error.rs` (error helpers).
- `Cargo.toml` / `Cargo.lock` — crate metadata and dependency lockfile.
- `target/` — build outputs (ignored in VCS).

## Development Expectations
- Standard Rust 2024 toolchain; typical workflow uses `cargo` commands for build/test/lint (documented in `README.md`).
- Keep changes small and well-scoped; prefer incremental PRs.
- Log thoughtfully: `info!` for normal flow, `warn!` for recoverable anomalies, `debug!` for deep tracing.

## Coding Style & Naming
- Follow rustfmt defaults and idiomatic Rust patterns.
- Naming: snake_case for functions/variables, PascalCase for types, SCREAMING_SNAKE_CASE for consts (e.g., `API_ENDPOINT`).
- Avoid logging secrets (e.g., bearer tokens); redact or shorten when necessary.

## Testing Guidance
- Favor fast, deterministic tests; isolate network I/O behind traits or mocks where possible.
- Place integration tests in `tests/` mirroring routes/features; name like `test_handles_<case>()` for clarity.
- Add a regression test when fixing a bug to document behavior.

## Commit & Pull Request Practices
- Use concise, conventional-commit style summaries (e.g., `fix: ...`, `chore: ...`, `refactor: ...`).
- Include a brief description of intent, and note relevant tests executed in the PR body.
- Keep commits logically grouped to simplify review and reverts.

## Security & Configuration Notes
- Authentication relies on `Authorization: Bearer <token>`; do not print raw tokens in logs.
- Upstream routing targets OpenRouter (`API_ENDPOINT`, `HOST_HEADER`); adjust via config/env rather than code when deploying.
- Run the proxy behind appropriate network controls when exposed publicly.
