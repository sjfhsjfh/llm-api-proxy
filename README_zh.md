# llm-api-proxy

[English](README.md) · 简体中文

一个基于 Axum 的轻量级反向代理，用于前置 OpenRouter API。它针对 Codex 与 OpenRouter 协作时的两个问题做修复，其余请求透明转发。

## 功能
- **修复 function_call 参数**：`/responses` 端点在 SSE 流中补全被 OpenRouter 意外丢弃的 `function_call.arguments`。
- **过滤 reasoning 项**：转发前移除 `input` 数组中 `type == "reasoning"` 的条目，避免上游返回 `ZodError`。
- **透明代理**：除 `/responses` 外的 GET/POST 路径均按原样代理到 `https://openrouter.ai/api/v1`，保留 headers 和状态码。

## 快速开始
```bash
# 构建并运行（调试模式）
cargo run
# 服务监听 0.0.0.0:9723
```

调用时在请求头中携带 OpenRouter API 令牌：`Authorization: Bearer <token>`。

示例：向 `/responses` 发送 POST 并流式接收 SSE：
```bash
curl -N \
  -H "Authorization: Bearer $OPENROUTER_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"input":[{"type":"text","content":"hello"}]}' \
  http://localhost:9723/responses
```

## API 说明
- `/responses` 以外的路径全部原样代理到上游。
- `/responses` 的 SSE 流会尽量补全 `function_call` 的 `arguments`；收到 `[DONE]` 后发送队列中的修复事件。

## 开发说明
- 使用 Rust 2024 版工具链，依赖由 Cargo 管理（`axum`、`tokio`、`reqwest`、`serde_json` 等）。
- 建议提交流程：`cargo fmt`、`cargo clippy --all-targets --all-features`、`cargo test`。
- 日志由 `colog` 与 `log` crate 驱动，可通过 `RUST_LOG` 环境变量调整级别。

## 部署提示
- 若对公网开放，建议置于自有入口或反向代理之后。
- 避免在日志中输出完整的 bearer token，只保留缩写或隐藏处理。
