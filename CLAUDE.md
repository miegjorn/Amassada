# Amassada

Development language: **Rust**. Cargo workspace with two crates: `amassada-core` (library) and `amassada-server` (binary).

## Key crates
- `tokio` — async runtime
- `axum` — HTTP server (`amassada-server`)
- `serde` + `serde_yaml` — canvas YAML parsing
- `async-trait` — Transport trait
- `anthropic` (or `reqwest` against the Anthropic API directly) — agent dispatch

## Agent dispatch
`dispatch(TurnRequest)` is a streaming Claude API call. The model is configured per-agent in Fondament (defaults to `claude-sonnet-4-6`). The system prompt injects the agent's persona and available blocks. The user turn is the assembled context from `build_context()`. The response is streamed and fed to the block parser in `blocks.rs` as it arrives.

Moderator turns receive an additional context envelope (transcript, budget state, artifact status, active personas, canvas hints — all labeled as advisory).

## Canvas library
Built-in canvases live in `canvases/stdlib/`. The `CanvasSelector` matches intake text to a canvas at session start.

## Design spec
`docs/superpowers/specs/2026-06-12-amassada-design.md`

## Sibling repos
- **Fondament** — source of all agent/persona definitions; Amassada loads agents from there
- **Charradissa** — consumes `amassada-core` as a library; implements `CharradissaTransport`
- **Gardian** — credential resolution for agent API keys
- **Farga** — future: session transcript persistence (Transport observer pattern)
