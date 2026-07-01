# Multi-Layer Context Graph — Spec v0.1

**Date:** 2026-06-27  
**Status:** Design — pre-approved for implementation  
**Addresses:** Problem 1 (meaning density / rocket equation) + Problem 2 (shared cache / sub-thread dispatch)

---

## Problem Statement

**Problem 1 — The rocket equation of context.**  
Long sessions accumulate transcript. Passing raw transcript at every turn scales linearly with session length, degrades model attention, and defeats caching. Compressing the transcript into a smaller representation (CONTEXT_GRAPH) helps but doesn't escape the equation — it just delays the wall. The real escape is lazy loading: carry an index, retrieve content on demand. The cost is retrieval precision failure, specifically latent connection loss — cross-domain bridges that don't look relevant in isolation but are critical for multi-concept reasoning.

**Problem 2 — Multi-agent memory sharing.**  
Each agent in a round currently rebuilds context independently from the same ContextBuilder. No shared cache. Sequential dispatch. Sub-agents are new processes, not sub-threads. The goal: all agents in a round share a single cached context block; each agent is defined by a small per-agent delta over that shared block.

---

## Core Model

The session context is a **multi-layer graph**. Not a 2D DAG — a PCB stack. Independent layers, each with its own topology, connected by vias at specific points. The composite is the context; no single layer is sufficient alone.

### Layers

| Layer | What it encodes | Node property | Edge types |
|---|---|---|---|
| **Causal** | Reasoning progression — what leads to what | `node_type` (Axiom/Resolved/Question/Frontier/Dead) | `leads_to`, `supersedes`, `dead` |
| **Epistemic** | Resolution state — how settled is each concept | `epistemic_state: f32` (0.0=open, 1.0=resolved) | `resolves`, `opens`, `challenges` |
| **Economic** | Activation weight — how load-bearing right now | `activation_weight: f32` (0.0=dormant, 1.0=critical) | (no edges — per-node scalar) |
| **Semantic** | Conceptual proximity regardless of causal position | embedding-space position | `similar_to`, `analogy_of` |

### Vias

Cross-layer connections. A via is a first-class object — not an edge within a layer but a connection between layers at a specific point. Vias are where latent connections live: two nodes that are causally distant but semantically adjacent, or an analogy on the semantic layer that grounds a decision on the causal layer.

```
Via {
    from_layer: LayerKind,
    from_node:  NodeId,
    to_layer:   LayerKind,
    to_node:    NodeId,
    via_type:   ViaType,   // AnalogyOf | Grounds | Challenges | SimilarTo
    strength:   f32,
}
```

Example: the WDM fiber optic analogy is a via from `semantic:wdm_multichannel` to `causal:N3_context_encoding_design` with `via_type: AnalogyOf, strength: 0.85`. Without via traversal, the fiber optic node never scores relevant to a context encoding query. With via traversal, it's pulled as an active neighbor.

### Node Summary — the "pointer"

Each node serializes to a compact text representation: ≤20 tokens. This is what gets sent to Anthropic. It is not a UUID — it IS the content, just compressed. Sending the same summary bytes every turn triggers the cache hit.

```
[N3:resolved:aw=0.80] "hierarchy wins: axiom→decisions→state→recent"
[N8:frontier:aw=1.00] "multi-layer graph + lazy loading"
```

Full node content (the reasoning, source turns, backing corpus) lives in Farga. Retrieved on demand only when the agent needs depth on a specific node.

---

## Session Lifecycle

### Session start

Load prior `SessionGraph` from Farga (continuing session) or initialize empty. Graph lives in `SessionEngine` memory for the session duration. Farga reads happen once at start, not per turn.

### Per-turn context assembly (in memory, no LLM)

1. Determine agent scope — which nodes are active for this agent (moderator-assigned or threshold-filtered by `activation_weight`)
2. Retrieve relevant nodes from each layer via graph traversal
3. Follow active vias — include cross-layer neighbors of retrieved nodes
4. Serialize retrieved subset to text block
5. Build `TurnRequest`

### Round boundary (one LLM call per round, not per turn)

1. Collect `[GRAPH_PROPOSAL]` blocks from all agent responses
2. Run extraction pass (Haiku): read new transcript turns → extract new nodes, edges, vias
3. Merge proposals + extraction into graph
4. Recompute `activation_weight` and `epistemic_state` for affected nodes
5. Persist updated graph to Farga
6. New graph version ready for next round — triggers cache bust on `system[0]`

---

## API Payload Structure

What crosses the wire to Anthropic per agent per turn:

```json
{
  "system": [
    {
      "type": "text",
      "text": "<SESSION_CONTEXT block — serialized retrieved graph subset>",
      "cache_control": {"type": "ephemeral"}
    },
    {
      "type": "text",
      "text": "<agent persona + aporia preamble>",
      "cache_control": {"type": "ephemeral"}
    }
  ],
  "messages": [
    {
      "role": "user",
      "content": "[SCOPE: N3, N8]\n[whisper if any]\nYour task: ..."
    }
  ]
}
```

`system[0]` is identical for all agents in a round → one cache write, N cache reads.  
`system[1]` is per-agent → cache hit after first call for that persona.  
`user` is uncached — the only per-turn, per-agent delta.

---

## Serialization Format (SESSION_CONTEXT)

```
SESSION_CONTEXT v0.1
session: <id>  round: <N>  nodes: <count>
frontier: [N8]  unresolved: [N6]

CAUSAL
id    type      aw    ep    summary
N1    axiom     0.90  0.95  "context mgmt: long convs degrade signal"
N3    resolved  0.80  0.05  "hierarchy: axiom→decisions→state→recent"
N8    frontier  1.00  0.20  "multi-layer graph + lazy loading"

EPISTEMIC (open nodes only)
N6    0.60  "open: extraction fidelity — how reliably can model extract nodes"
N8    0.20  "partial: via detection and layer count not settled"

VIAS (active)
N_wdm → N3   analogy_of  0.85  "WDM as multi-channel encoding model"
```

Target: ≤900 tokens for a mature session with 20-30 nodes. Full Farga content never included unless explicitly retrieved.

---

## Data Structures (Rust)

### SessionGraph

```rust
pub struct SessionGraph {
    pub version: u32,
    pub session_id: String,
    pub layers: LayerSet,
    pub vias: Vec<Via>,
}

pub struct LayerSet {
    pub causal:    Layer,
    pub epistemic: Layer,
    pub semantic:  Layer,
    pub economic:  Layer,
}

pub struct Layer {
    pub kind:  LayerKind,
    pub nodes: HashMap<NodeId, Node>,
    pub edges: Vec<Edge>,
}

pub struct Node {
    pub id:                NodeId,
    pub summary:           String,        // ≤20 tokens — the pointer text
    pub node_type:         NodeType,
    pub activation_weight: f32,
    pub epistemic_state:   f32,
    pub farga_ref:         Option<String>, // Farga node id for full content
}

pub struct Via {
    pub from_layer: LayerKind,
    pub from_node:  NodeId,
    pub to_layer:   LayerKind,
    pub to_node:    NodeId,
    pub via_type:   ViaType,
    pub strength:   f32,
}

pub enum NodeType    { Axiom, Resolved, Question, Supporting, Frontier, Dead }
pub enum LayerKind   { Causal, Epistemic, Semantic, Economic }
pub enum ViaType     { AnalogyOf, SimilarTo, Grounds, Challenges }

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(pub String);
```

### TurnRequest changes

```rust
pub struct TurnRequest {
    pub shared_context:  Option<String>,  // system[0] — graph subset, shared across agents
    pub system_prompt:   String,          // system[1] — persona + preamble
    pub context:         String,          // user turn — scope + whispers + task
    pub model:           String,
    pub max_tokens:      u32,
    pub thinking_budget: Option<u32>,
    pub api_key:         Option<String>,
}
```

### GraphProposal (new block type)

Agents emit proposals; they do not mutate the graph directly.

```
[GRAPH_PROPOSAL]
UPDATE N8: activation_weight=0.95, epistemic_state=0.80
NEW N9: type=resolved, summary="parallel dispatch via tokio::join_all confirmed"
EDGE N8->N9: leads_to weight=1.00
VIA semantic:N_analogy->causal:N9: analogy_of strength=0.70
```

Parsed in `blocks.rs` as `AgentBlock::GraphProposal(Vec<ProposalOp>)`. Merged at round boundary by the graph engine. Moderator proposals take precedence on conflict.

### ComposedPart (convergence with Fondament)

Replace `Vec<(String, String)>` in `resolver.rs` with:

```rust
pub struct ComposedPart {
    pub kind:       PartKind,
    pub name:       String,
    pub weight:     f32,             // 0.0 for static agent parts; activation_weight for session nodes
    pub corpus_ref: Option<String>,  // Farga node id
}

pub enum PartKind { Domain, Discipline, Stance, SessionNode }
```

`build_aporia_preamble` takes `&[ComposedPart]`. At dispatch time, Amassada appends frontier session nodes (those with `activation_weight > 0.6` and `epistemic_state < 0.5`) to the agent's `collected_parts`. The aporia instruction then covers both identity decomposition and session state decomposition with one template.

---

## Parallel Dispatch (RoundRunner change)

Replace the sequential `for agent_id in &participant_ids` loop with concurrent dispatch:

```rust
let handles: Vec<_> = participant_ids
    .iter()
    .map(|agent_id| {
        let req = build_request(agent_id, &shared_context, ...);
        tokio::spawn(dispatch(req))
    })
    .collect();

let responses = futures::future::join_all(handles).await;
```

All agents in a round fire concurrently. `system[0]` is identical across all → single cache write at first call, cache reads for all subsequent calls within the round.

---

## Implementation Order

1. `SessionGraph` struct + serializer (`amassada-core/src/graph/`)
2. `SessionGraph::from_transcript(&ContextBuilder)` — extraction pass (Haiku call)
3. `SessionGraph::retrieve(&AgentScope) -> String` — in-memory traversal + serialization
4. `TurnRequest.shared_context` + dispatch injection as `system[0]`
5. `[GRAPH_PROPOSAL]` block type in `blocks.rs` + `ProposalOp` parser
6. `SessionGraph::merge_proposals(Vec<ProposalOp>)` — round-end merge
7. Parallel `RoundRunner` — `tokio::join_all`
8. `ComposedPart` in `fondament-core/src/resolver.rs` — convergence point
9. Farga persistence: `SessionGraph` → Farga node at session end, load at session start
