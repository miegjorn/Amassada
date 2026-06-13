# Amassada — Multi-Agent Session Engine

**Amassada** (Occitan: *gathering*) is the session orchestration engine of the Occitan stack.

A **session** is a structured conversation between agents with different contexts, expertise, and tool access, assembled to address a challenge. Sessions are governed by a **Moderator agent** and structured into **rounds** and **turns**, following rules declared in a **canvas** (YAML file).

## Core concepts

| Concept | Description |
|---|---|
| **Canvas** | YAML file defining participants, budget pools, round bounds, and expected output |
| **Session** | A state machine driven by a canvas — `Initializing → Round(n) → … → Complete` |
| **Round** | A full cycle where all participants have a turn |
| **Turn** | A single agent's scheduled contribution |
| **`/btw`** | A labeled out-of-turn signal (agent→agent or human→agent) — never counts as a turn |
| **Consultation** | A private 2-turn mini-session between two agents, before the requester's turn |
| **`/call`** | Human-only binding directive — opens an advisory window, then closes the session |
| **Moderator** | AI agent that selects participants, drives state transitions, and detects convergence |

## Execution contexts

Amassada runs identically in two contexts — same rules, different transport:

- **Local / CLI** — no chatroom, auto or interactive mode, stdout/buffer I/O
- **Charradissa** — live Matrix rooms, agents as room members, real-time streaming

## Human authority

A human participant holds an **authoritative** slot. Agents may provide advisory pushback, but:
- Human `/btw` is never overridable by the Moderator
- Human `/call` is a hard terminal — one advisory window, then the session closes on confirmation

## Built-in canvases

| Canvas | Mode |
|---|---|
| `debate` | auto |
| `design-session` | interactive |
| `code-review-council` | auto |
| `architectural-design` | interactive |
| `planning` | interactive |

Canvas selection is automatic — the engine matches the intake prompt to the best canvas by description, tags, and examples.

## Structure

```
crates/
  amassada-core/    library — pure session logic, Transport trait, canvas parser
  amassada-server/  thin Axum service — REST + WebSocket
canvases/stdlib/    built-in canvas YAML files
```

## Design spec

`docs/superpowers/specs/2026-06-12-amassada-design.md`

## Language

Rust. Cargo workspace. `tokio` for async, `axum` for the server, `serde`/`serde_yaml` for canvas parsing.
