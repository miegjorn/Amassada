# Multi-Layer Context Graph — Implementation Plan

> **Spec:** `docs/superpowers/specs/2026-06-27-multi-layer-context-graph.md`  
> **Status:** Pre-approved. Use `superpowers:subagent-driven-development` to execute.  
> **Baseline:** run `cargo test` in Amassada before starting — all tests must pass.

---

## Task 1 — Core types: `SessionGraph` + serializer

**Crate:** `amassada-core`  
**New module:** `crates/amassada-core/src/graph/mod.rs`  
**New test file:** `crates/amassada-core/tests/graph_tests.rs`

- [ ] Create `crates/amassada-core/src/graph/mod.rs` with:
  - `NodeId(pub String)` — newtype
  - `NodeType` enum: `Axiom | Resolved | Question | Supporting | Frontier | Dead`
  - `LayerKind` enum: `Causal | Epistemic | Semantic | Economic`
  - `ViaType` enum: `AnalogyOf | SimilarTo | Grounds | Challenges`
  - `Node` struct: `id, summary (≤20 tokens), node_type, activation_weight: f32, epistemic_state: f32, farga_ref: Option<String>`
  - `Edge` struct: `from, to, edge_type: EdgeType, weight: f32`
  - `EdgeType` enum: `LeadsTo | Supports | Supersedes | Challenges | Dead`
  - `Via` struct: `from_layer, from_node: NodeId, to_layer, to_node: NodeId, via_type, strength: f32`
  - `Layer` struct: `kind, nodes: HashMap<NodeId, Node>, edges: Vec<Edge>`
  - `LayerSet` struct: `causal, epistemic, semantic, economic: Layer`
  - `SessionGraph` struct: `version: u32, session_id: String, layers: LayerSet, vias: Vec<Via>`
  - `impl SessionGraph { pub fn new(session_id: &str) -> Self }`

- [ ] Add `mod graph;` + `pub use graph::*;` in `src/lib.rs`

- [ ] Write failing tests in `graph_tests.rs`:
  - `session_graph_initializes_empty` — new graph has version=0, empty layers
  - `node_id_equality` — two NodeId with same string are equal
  - `layer_insert_and_retrieve` — insert node, get it back by id
  - `via_connects_layers` — via from causal node to semantic node

- [ ] Implement until tests pass

- [ ] Add `pub fn serialize(&self) -> String` to `SessionGraph` — produces `SESSION_CONTEXT v0.1` text block as specified. Deterministic: sort nodes by id within each layer section.

- [ ] Test serialization:
  - `serialize_is_deterministic` — same graph state produces identical bytes on two calls
  - `serialize_frontier_nodes_listed` — frontier nodes appear in header
  - `serialize_active_vias_only` — vias with strength < 0.3 omitted

---

## Task 2 — Retrieval: `SessionGraph::retrieve`

**Same module:** `src/graph/mod.rs`

- [ ] `pub fn retrieve(&self, scope: &[NodeId], via_hops: u8) -> String`
  - Start from `scope` nodes
  - For each scope node, include its immediate causal edges (1 hop)
  - Follow active vias (`strength > 0.5`) from scope nodes: include the via target node
  - If `via_hops == 2`, follow one more hop from via targets
  - Serialize the resulting subset to `SESSION_CONTEXT` format
  - Only include epistemic section entries for nodes with `epistemic_state < 0.8`
  - Only include vias that connect at least one node in the retrieved subset

- [ ] Test retrieval:
  - `retrieve_returns_scope_nodes` — scope nodes always present in output
  - `retrieve_follows_one_hop_edges` — causal neighbor of scope node included
  - `retrieve_follows_vias` — via target included when strength above threshold
  - `retrieve_omits_weak_vias` — via target excluded when strength below threshold
  - `retrieve_output_is_subset_of_full` — retrieved token count < full serialization

---

## Task 3 — Extraction: `SessionGraph::from_transcript`

**New file:** `crates/amassada-core/src/graph/extractor.rs`

The extractor runs one Haiku API call per round boundary. It reads new transcript turns and returns a `GraphDelta` — new nodes, new edges, new vias, and channel updates for existing nodes.

- [ ] Define `GraphDelta` struct:
  ```rust
  pub struct GraphDelta {
      pub new_nodes:    Vec<Node>,
      pub new_edges:    Vec<Edge>,
      pub new_vias:     Vec<Via>,
      pub updates:      Vec<NodeUpdate>,  // activation_weight/epistemic_state changes
  }
  pub struct NodeUpdate { pub id: NodeId, pub activation_weight: Option<f32>, pub epistemic_state: Option<f32> }
  ```

- [ ] `pub async fn extract_delta(transcript_segment: &str, existing_nodes: &[NodeId], api_key: Option<String>) -> Result<GraphDelta>`
  - Calls Haiku (`claude-haiku-4-5-20251001`) with structured extraction prompt
  - Extraction prompt instructs model to return JSON with `nodes`, `edges`, `vias`, `updates`
  - Parse JSON response into `GraphDelta`
  - Existing node IDs passed in so model can reference them in edges/updates rather than creating duplicates

- [ ] `pub fn apply_delta(&mut self, delta: GraphDelta)` on `SessionGraph`
  - Insert new nodes into appropriate layer based on `node_type`
  - Semantic nodes go to `layers.semantic`, others to `layers.causal`
  - Apply updates to existing nodes
  - Bump `self.version += 1`

- [ ] Tests:
  - `apply_delta_increments_version`
  - `apply_delta_inserts_nodes`
  - `extract_delta_parses_valid_json` — test with a fixture JSON response, not a live API call

---

## Task 4 — `TurnRequest.shared_context` + dispatch injection

**File:** `crates/amassada-core/src/dispatch.rs`

- [ ] Add `shared_context: Option<String>` to `TurnRequest`

- [ ] In `dispatch()`, when `shared_context` is `Some(ctx)`:
  - Build `system` as a JSON array with two blocks:
    ```json
    [
      {"type": "text", "text": "<ctx>",               "cache_control": {"type": "ephemeral"}},
      {"type": "text", "text": "<req.system_prompt>", "cache_control": {"type": "ephemeral"}}
    ]
    ```
  - When `shared_context` is `None`, keep current single-block behavior

- [ ] Fix all `TurnRequest { .. }` struct literals to include `shared_context: None`

- [ ] Tests:
  - `dispatch_without_shared_context_uses_single_system_block`
  - `dispatch_with_shared_context_uses_two_system_blocks`
  - (unit test the JSON body construction, no live API call)

---

## Task 5 — `[GRAPH_PROPOSAL]` block type

**File:** `crates/amassada-core/src/blocks.rs`

- [ ] Add variant to `AgentBlock`:
  ```rust
  GraphProposal { ops: Vec<ProposalOp> }
  ```

- [ ] Define `ProposalOp` enum:
  ```rust
  pub enum ProposalOp {
      UpdateNode { id: NodeId, activation_weight: Option<f32>, epistemic_state: Option<f32> },
      NewNode    { node: Node },
      NewEdge    { edge: Edge },
      NewVia     { via: Via },
  }
  ```

- [ ] Extend `parse_blocks()` to handle `[GRAPH_PROPOSAL]` sections:
  - Parse line-by-line: `UPDATE <id>: key=val`, `NEW <type>: ...`, `EDGE <from>-><to>: ...`, `VIA <layer:node>-><layer:node>: ...`
  - Return as `AgentBlock::GraphProposal { ops }`

- [ ] Tests:
  - `parse_graph_proposal_update` — parses UPDATE line into `UpdateNode`
  - `parse_graph_proposal_new_node` — parses NEW line into `NewNode`
  - `parse_graph_proposal_new_edge` — parses EDGE line
  - `parse_graph_proposal_new_via` — parses VIA line
  - `parse_graph_proposal_mixed` — a block with all four op types

---

## Task 6 — Round-boundary graph update in `SessionEngine`

**File:** `crates/amassada-core/src/session.rs`

- [ ] Add `graph: SessionGraph` to `SessionEngine`

- [ ] After each round completes in `run()`:
  1. Collect `GraphProposal` blocks from all `TurnRecord`s of this round
  2. Convert proposals to `GraphDelta` and call `graph.apply_delta()`
  3. Call `graph.extract_delta(new_transcript_segment)` (Haiku call, await)
  4. Apply extraction delta
  5. Store updated `graph` — it becomes `shared_context` for next round's dispatch

- [ ] Update `RoundRunner::run()` to accept and thread `shared_context: Option<String>` into each `TurnRequest`

- [ ] Pass `graph.retrieve(&frontier_scope, 1)` as `shared_context` at round start

- [ ] Tests:
  - `session_graph_version_increments_each_round` — integration test using `LocalTransport`
  - `round_shared_context_injected` — verify `TurnRequest.shared_context` is `Some` on round 2+

---

## Task 7 — Parallel dispatch in `RoundRunner`

**File:** `crates/amassada-core/src/round.rs`

- [ ] Replace the sequential `for agent_id in &participant_ids` loop with:
  ```rust
  let handles: Vec<_> = participant_ids.iter()
      .map(|agent_id| {
          let req = build_request(agent_id, ...);
          tokio::spawn(async move { dispatch(req).await })
      })
      .collect();
  let responses = futures::future::join_all(handles).await;
  ```

- [ ] Collect responses, apply budget charges, push turn records, handle proposals — same logic as before, now over the joined results

- [ ] Add `futures` to `amassada-core/Cargo.toml`: `futures = "0.3"`

- [ ] Tests:
  - `round_dispatches_all_agents` — both agents produce responses (existing test should still pass)
  - `round_parallelism_does_not_duplicate_turns` — N agents → exactly N turn records

---

## Task 8 — `ComposedPart` convergence in Fondament

**File:** `Fondament/fondament-core/src/resolver.rs`

- [ ] Define in `fondament-core/src/types.rs`:
  ```rust
  pub struct ComposedPart {
      pub kind:       PartKind,
      pub name:       String,
      pub weight:     f32,
      pub corpus_ref: Option<String>,
  }
  pub enum PartKind { Domain, Discipline, Stance, SessionNode }
  ```

- [ ] Replace `collected_parts: Vec<(String, String)>` with `Vec<ComposedPart>` in `resolve()`

- [ ] Update `build_deconstructive_preamble` signature to `fn build_deconstructive_preamble(parts: &[ComposedPart]) -> String`

- [ ] Update preamble formatting to render `PartKind::SessionNode` with weight annotation:
  ```
    - [session-node: N8 — frontier — weight: 1.00]
  ```

- [ ] All existing resolver tests must still pass

---

## Task 9 — Farga persistence

**File:** `crates/amassada-core/src/session.rs` + Farga HTTP client

- [ ] At session end in `SessionEngine::run()`: serialize `graph` to JSON and POST to Farga as a node with kind `SessionGraph`, keyed by `session_id`

- [ ] At session start in `SessionEngine::new()` (or a `SessionEngine::load()` constructor): attempt GET from Farga by `session_id`; if found, deserialize into `SessionGraph`; if not found, initialize empty

- [ ] `SessionGraph` derives `serde::Serialize + serde::Deserialize`

- [ ] Tests:
  - `session_graph_roundtrips_serde` — serialize to JSON, deserialize, compare

---

## Self-review checklist (run before marking complete)

```bash
cd /Users/bedardpl/project/Amassada && cargo test 2>&1 | grep "test result"
cd /Users/bedardpl/project/Fondament && cargo test 2>&1 | grep "test result"
```

- All prior tests still pass
- No `unwrap()` on external data (API responses, serde)
- `shared_context` is `None` for round 1 (no graph yet)
- Extraction errors are logged and non-fatal — session continues without graph update on Haiku failure
- `SessionGraph::serialize()` is deterministic — verified by test
