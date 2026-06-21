# Amassada

Development language: **Rust**. Cargo workspace with two crates: `amassada-core` (library) and `amassada-server` (binary).

## Key crates
- `tokio` — async runtime
- `axum` — HTTP server (`amassada-server`)
- `serde` + `serde_yaml` — canvas YAML parsing
- `async-trait` — Transport trait
- `anthropic` (or `reqwest` against the Anthropic API directly) — agent dispatch

## Server state
`ServerState` (defined in `api.rs`) holds:
- `canvas_dir` — path to the baked-in canvas library (`/canvases/stdlib` in the container)
- `active_state: Arc<Mutex<SessionState>>` — current session state
- `event_tx: broadcast::Sender<SessionEvent>` — fan-out channel (capacity 256); every WebSocket subscriber and internal consumer receives all events

## HTTP + WebSocket surface (`amassada-server/src/`)
- `main.rs` — creates the broadcast channel at startup, wires routes
- `api.rs` — `POST /sessions`, `GET /state`, `POST /human_input`, `POST /events` (external producers publish `SessionEvent` JSON into the bus)
- `ws.rs` — `GET /ws` upgrades to WebSocket; each connection subscribes to the broadcast channel and receives all `SessionEvent`s as JSON; closes on terminal events (`SessionCompleted`, `SessionFailed`) or client disconnect; lagged subscribers get a warning, not a panic

## Agent dispatch
`dispatch(TurnRequest)` is a single non-streaming Claude API call (`reqwest` POST to `/v1/messages`, full JSON response awaited). The model is configured per-agent in Fondament (defaults to `claude-sonnet-4-6`). The system prompt injects the agent's persona and available blocks. The user turn is the assembled context from `build_context()`. The full response text is parsed by the block parser in `blocks.rs` once the call returns. Prompt caching is always enabled (`anthropic-beta: prompt-caching-2024-07-31`); extended thinking (`interleaved-thinking-2025-05-14`) is enabled only when `thinking_budget` is set.

Moderator turns receive an additional context envelope (transcript, budget state, artifact status, active personas, canvas hints — all labeled as advisory).

## Canvas library
Built-in canvases live in `canvases/stdlib/`. The `CanvasSelector` matches intake text to a canvas at session start.

## CI / image
Push to `main` → GitHub Actions builds and pushes `ghcr.io/miegjorn/amassada:latest` + `:sha-<sha>`, then commits the new `imageTag` into `miegjorn/Caissa` `deploy/charts/occitan/values.yaml` so ArgoCD rolls the cluster automatically.

## Design spec
`docs/superpowers/specs/2026-06-12-amassada-design.md`

## Sibling repos
- **Fondament** — source of all agent/persona definitions; Amassada loads agents from there
- **Charradissa** — consumes `amassada-core` as a library; implements `CharradissaTransport`
- **Gardian** — credential resolution for agent API keys
- **Farga** — future: session transcript persistence (Transport observer pattern)
