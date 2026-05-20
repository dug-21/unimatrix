# Risk-Based Test Strategy: vnc-019

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | BFS visited-set keyed by node ID but resolve_supersessions substitutes to terminal ID — if substitution happens AFTER visited check, the deprecated node is inserted and its terminal successor is never explored | High | Med | Critical |
| R-02 | direction="both" dedup uses canonical (source_id, target_id, rel_type) triple — if edge_key is built from the iteration variable direction rather than the canonical stored direction, duplicate edges appear or edges are silently dropped (lesson #4077) | High | Med | Critical |
| R-03 | max_nodes cap checked pre-enqueue against collected_node_ids.len() — seeds are added in the seed phase before BFS; if seed count alone equals or exceeds max_nodes, BFS must set truncated=true and depth_reached=0 without executing | High | Med | Critical |
| R-04 | Post-BFS OR-chain SQL built dynamically — empty WHERE clause when collected_edges is empty produces syntax error or full-table scan; must be skipped entirely per ADR-003 | High | Low | High |
| R-05 | validate_no_unsupported_params modified to add "subgraph" arm — regression risk: seed_ids or max_depth passing through on chain/current/neighbors after the arm is added | High | Low | High |
| R-06 | resolve_supersessions=true: follow_to_current called inline per deprecated neighbor — if supersession chain is circular (A superseded_by B, B superseded_by A), the 50-hop guard must terminate; BFS must not loop infinitely | High | Low | High |
| R-07 | max_nodes > 200 behavior is unresolved in spec (FR-07: "clamped or rejected — architect decision") — whatever is implemented must be consistent and tested; the cap must never be exceeded | High | Low | High |
| R-08 | depth_reached computed as max depth across collected_edges — when truncation occurs at depth N, depth_reached must reflect N (the depth at which truncation fired), not max_depth; when no edges collected, depth_reached=0 | Med | Med | High |
| R-09 | Batch node hydration (get_many) called after BFS — if a node ID is in collected_node_ids but absent from ENTRIES (e.g., deleted between graph rebuild and hydration), the batch query behavior (skip vs. error) determines whether truncated nodes cause a panic or silent omission | Med | Med | High |
| R-10 | #[path]-declared submodule graph_read_subgraph.rs — SubgraphResponse defined in graph_read.rs must be visible (pub) to the subgraph module; import path errors surface only at compile time but are easy to introduce when type and constructor are in different files | Med | Low | Med |
| R-11 | Staleness contract: in-memory BFS silently omits edges written within the current tick interval — no runtime signal; tool description is the only disclosure per ADR-004; agents acting on stale subgraph data may draw incorrect conclusions | Med | High | High |
| R-12 | edges collected during "both"-direction traversal include depth from the direction first discovered — if A→B is discovered from A (depth=1) and again from B (depth=0 when B is a seed), the canonical edge's recorded depth is non-deterministic without explicit first-write-wins dedup ordering | Med | Med | Med |
| R-13 | follow_to_current returns None when terminal cannot be resolved (50-hop exceeded or chain broken) — BFS pseudocode uses .unwrap_or(neighbor_id) fallback, which re-enqueues the deprecated original; this is correct but must be tested explicitly | Med | Low | Med |
| R-14 | edge_types absent/empty defaults to all 16 RelationType variants via all_non_supersedes_types — if that function is not imported correctly from graph_read_neighbors.rs or the list is stale, silent traversal reduction occurs | Med | Low | Med |
| R-15 | metadata TEXT column in GRAPH_EDGES is parsed via serde_json — malformed JSON in the column (stored by a buggy writer) must produce metadata=None or a logged warning, not a panic or tool-call failure | Med | Low | Med |
| R-16 | SR-02: truncated=true with no structured reason — agents may retry with same params, looping indefinitely; tool description must document re-query pattern (smaller max_depth, targeted edge_types) | Low | Med | Low |

## Risk-to-Scenario Mapping

### R-01: Visited-set / resolve_supersessions ordering
**Severity**: High
**Likelihood**: Med
**Impact**: Deprecated nodes appear in results when resolve_supersessions=true, or terminal successors are double-enqueued via multiple paths.

**Test Scenarios**:
1. Graph: A(active) → B(deprecated, superseded_by C) → C(active). Call with seed=[A], resolve_supersessions=true. Assert: nodes contains A and C; B is absent. Assert: C appears exactly once.
2. Graph: A → B(deprecated, superseded_by C); D → C(reachable from two paths). Call with seed=[A, D], resolve_supersessions=true. Assert: C appears exactly once in nodes (dedup via visited set keyed on terminal ID).
3. Call with resolve_supersessions=false on same graph. Assert: B present in nodes; C also present if reachable via separate edge.

**Coverage Requirement**: Pre-enqueue substitution ordering; visited-set keyed on terminal (not deprecated) ID.

---

### R-02: direction="both" edge deduplication and canonical direction
**Severity**: High
**Likelihood**: Med
**Impact**: Duplicate edges in response (each edge appears twice) or missing edges; direction field wrong. Lesson #4077 — direction semantics bugs survive review when the code uses the opposite enum value from what the spec describes.

**Test Scenarios**:
1. Graph: A Supports B (single stored edge). Call with seed=[A, B], direction="both". Assert: edges contains exactly one EdgeRecord with source_id=A, target_id=B, relation_type="Supports", direction="outgoing".
2. Same call, assert len(edges)==1 (no duplicate).
3. Graph: A Supports B; B Supports C. Call with seed=[A], direction="both", max_depth=2. Assert: A→B and B→C each appear once; no reverse duplicates.
4. Verify edge_key construction in code review: canonical triple must use stored direction (source→target), not iteration variable direction.

**Coverage Requirement**: Single-edge dedup; multi-hop dedup; direction="outgoing" on all returned EdgeRecords.

---

### R-03: Seed count at or exceeding max_nodes cap
**Severity**: High
**Likelihood**: Med
**Impact**: Response contains more than 200 nodes, violating the wire contract; or seeds are silently dropped.

**Test Scenarios**:
1. Call with 201 seed IDs (all present in graph), max_nodes=200 (or default). Assert: nodes has exactly 200 entries; truncated=true; depth_reached=0.
2. Call with exactly 200 seed IDs. Assert: nodes has exactly 200 entries; truncated=true; depth_reached=0 (BFS skipped).
3. Call with 1 seed + dense graph, max_nodes=5. Assert: nodes len <= 5; truncated=true when graph expands beyond 5.
4. Call with 1 seed + isolated node (no edges). Assert: nodes has 1 entry; truncated=false; depth_reached=0.

**Coverage Requirement**: Cap enforcement at seed phase; cap enforcement during BFS; truncated+depth_reached correctness in both phases.

---

### R-04: Empty-edges OR-chain SQL guard
**Severity**: High
**Likelihood**: Low
**Impact**: Syntax error in SQLite query or full-table scan when collected_edges is empty (isolated seeds, cold-start).

**Test Scenarios**:
1. Call with seed=[N] where N exists but has no edges of the requested type. Assert: response has nodes=[N], edges=[], truncated=false, depth_reached=0. Assert: no SQL error returned. (AC-19)
2. Call with all seeds absent from in-memory graph (cold-start). Assert: empty result, no error, no metadata query issued.
3. Positive case: call with seed=[A] and A→B edge with non-null metadata. Assert: EdgeRecord.metadata is populated (not null). (AC-18)

**Coverage Requirement**: Metadata query skipped on empty edge set; metadata populated when non-null; no SQL syntax errors.

---

### R-05: validate_no_unsupported_params regression on existing modes
**Severity**: High
**Likelihood**: Low
**Impact**: seed_ids or max_depth silently accepted on chain/current/neighbors after the subgraph arm is added, breaking the forward-compat guard.

**Test Scenarios**:
1. Call context_graph(mode="chain", seed_ids=[1]). Assert: validation error returned. (AC-11)
2. Call context_graph(mode="current", seed_ids=[1]). Assert: validation error returned. (AC-11)
3. Call context_graph(mode="neighbors", seed_ids=[1]). Assert: validation error returned. (AC-11)
4. Call context_graph(mode="chain", max_depth=2). Assert: validation error with message containing "max_depth is not supported in chain mode". (AC-16)
5. Call context_graph(mode="current", max_depth=2). Assert: validation error. (AC-16)
6. Call context_graph(mode="neighbors", max_depth=2). Assert: validation error. (AC-16)
7. Call context_graph(mode="subgraph", seed_ids=[1], max_depth=3). Assert: no validation error for these params.
8. Call context_graph(mode="subgraph", from_id=1). Assert: validation error (from_id rejected on subgraph mode).

**Coverage Requirement**: All 6 mode/param combinations for regression; subgraph acceptance; subgraph rejection of path-mode params.

---

### R-06: Circular supersession chain loop termination
**Severity**: High
**Likelihood**: Low
**Impact**: BFS hangs indefinitely or panics with stack overflow if follow_to_current does not terminate on a circular chain.

**Test Scenarios**:
1. Create entries A and B where A.superseded_by = B.id and B.superseded_by = A.id (circular). Call with resolve_supersessions=true, seed=[A]. Assert: call returns within timeout; no panic; result is either empty or contains the fallback (original neighbor ID per .unwrap_or(neighbor_id)).
2. Create a supersession chain of exactly 50 hops. Call with resolve_supersessions=true. Assert: follow_to_current returns within the 50-hop guard; BFS completes.
3. Call with resolve_supersessions=true on a graph with no deprecated nodes. Assert: normal subgraph result returned.

**Coverage Requirement**: 50-hop guard termination; circular chain fallback to original ID; no infinite loop.

---

### R-07: max_nodes > 200 clamping vs. validation
**Severity**: High
**Likelihood**: Low
**Impact**: Response exceeds the 200-node / ~290 KB payload bound if clamping is not applied before cap enforcement; or callers receive an unexpected error if validation is chosen.

**Test Scenarios**:
1. Call with max_nodes=201 (or 500). Assert: response nodes len <= 200 (clamped silently) OR a validation error is returned. Assert the actual behavior is consistent with the spec/tool description.
2. Call with max_nodes=0. Assert: validation error (0 is below the useful minimum).
3. Call with max_nodes=200 (at cap). Assert: accepted, nodes len <= 200.

**Coverage Requirement**: The 200-node hard cap is never exceeded in the response regardless of caller input.

---

### R-08: depth_reached accuracy under truncation
**Severity**: Med
**Likelihood**: Med
**Impact**: Agents misinterpret how far BFS actually reached; may re-query with wrong depth reduction.

**Test Scenarios**:
1. Graph: A→B→C→D (linear chain). Call with max_depth=10. Assert: depth_reached=3 (the actual depth traversed).
2. Same graph, max_nodes=2. Assert: truncated=true; depth_reached=1 (only one hop before cap hit).
3. Isolated seed, no edges. Assert: depth_reached=0.
4. max_depth=1 on a deep graph. Assert: depth_reached=1 (bounded by max_depth, not by graph structure).

**Coverage Requirement**: depth_reached reflects actual traversal depth, not the requested max; 0 when no edges; correct under early truncation.

---

### R-09: Batch node hydration with missing ENTRIES rows
**Severity**: Med
**Likelihood**: Med
**Impact**: If an entry is deleted from ENTRIES between tick-rebuild and get_many, the batch query behavior determines whether the tool panics or returns a partial result silently.

**Test Scenarios**:
1. Normal case: all collected node IDs exist in ENTRIES. Assert: nodes count equals collected_node_ids count.
2. Review get_many implementation: confirm it returns partial results (skipping missing IDs) rather than returning an error. If it errors on missing IDs, this is a latent panic vector.
3. (If testable) Delete an entry after graph rebuild, then call subgraph with that entry as a BFS-discovered node. Assert: call completes without error; missing node is absent from response.

**Coverage Requirement**: get_many graceful handling of missing IDs verified; no panic path on partial hydration.

---

### R-10: SubgraphResponse visibility across module boundary
**Severity**: Med
**Likelihood**: Low
**Impact**: Compile error if SubgraphResponse is not pub in graph_read.rs; import path errors if #[path] declaration is wrong.

**Test Scenarios**:
1. Compile check: cargo build with no errors. (This is a Gate 3a/compile check, not a behavioral test.)
2. Verify graph_read_subgraph.rs uses `super::SubgraphResponse` (or equivalent) — code review.
3. Verify the #[path] declaration in graph_read.rs matches the actual filename graph_read_subgraph.rs.

**Coverage Requirement**: Module compiles cleanly; no import path errors; SubgraphResponse accessible from subgraph module.

---

### R-11: Tick-window staleness — silent missing edges
**Severity**: Med
**Likelihood**: High
**Impact**: Agents making decisions on a subgraph that is up to 60 seconds stale may miss recently-added edges; no runtime signal is provided (ADR-004).

**Test Scenarios**:
1. Verify tool description in tools.rs contains all four required disclosures from AC-13/FR-19: (a) in-memory BFS + tick-window, (b) depth_reached + truncated semantics, (c) unknown seed behavior, (d) direction always "outgoing". (Code review / string match.)
2. Write an integration test that calls subgraph immediately after writing a new edge (before the next tick). Assert: the edge MAY be absent from the result (staleness is correct behavior; test documents the contract rather than asserting presence).

**Coverage Requirement**: Disclosure text verified by code review; staleness contract documented in test comments.

---

### R-12: Edge depth non-determinism under multi-path discovery
**Severity**: Med
**Likelihood**: Med
**Impact**: EdgeRecord.depth value is non-deterministic if the same edge is reachable at different depths from different seeds; agents using depth for reasoning get inconsistent values across calls.

**Test Scenarios**:
1. Graph: seeds=[A, B]; A→C (depth 1 from A); B→C (depth 1 from B). Same edge A→C appears once. Assert: depth=1 (first discovery wins; dedup preserves first-inserted depth).
2. Graph: seed=[A]; A→B (depth 1); A→C→B (depth 2 — B reachable via two paths). Assert: B's edge A→B (or C→B) depth is the first-discovered depth; B appears once in nodes.
3. Verify BFS processes frontier in FIFO order (VecDeque) so shallowest discovery is always first.

**Coverage Requirement**: Depth value stability; first-discovery-wins behavior; no duplicate edges.

---

### R-13: follow_to_current None fallback behavior
**Severity**: Med
**Likelihood**: Low
**Impact**: When follow_to_current returns None (chain broken or 50-hop exceeded), fallback to original neighbor_id re-enqueues a deprecated node when resolve_supersessions=true. This is correct per spec but must be tested to ensure it does not cause a later panic or unexpected behavior.

**Test Scenarios**:
1. Supersession chain of 51 hops (exceeds 50-hop guard). Call with resolve_supersessions=true. Assert: the 51st-hop entry appears in nodes as the deprecated original (fallback); no error.
2. Entry with superseded_by pointing to a non-existent ID. Assert: follow_to_current returns None; original deprecated node is included in result.

**Coverage Requirement**: None fallback path tested; deprecated node included in result when resolution fails.

---

### R-14: Default edge_types expansion
**Severity**: Med
**Likelihood**: Low
**Impact**: If all_non_supersedes_types is not correctly imported or returns a stale list, some RelationType variants are silently excluded from default traversal.

**Test Scenarios**:
1. Call with edge_types absent. Assert: edges of all 16 RelationType variants are traversable (if present in graph).
2. Verify all_non_supersedes_types returns exactly the expected set of variants (unit test listing).
3. Call with edge_types=[] (empty list). Assert: behavior identical to absent edge_types (defaults to all types). Verify this is consistent with FR-04.

**Coverage Requirement**: All 16 types reachable on default; empty list treated same as absent.

---

### R-15: Malformed JSON in GRAPH_EDGES.metadata
**Severity**: Med
**Likelihood**: Low
**Impact**: serde_json parsing of metadata TEXT column panics or returns an error, causing the entire tool call to fail for a data-quality issue in a single edge.

**Test Scenarios**:
1. Insert an edge with metadata='invalid json{' directly in the test DB. Call subgraph including that edge. Assert: call succeeds; EdgeRecord.metadata is None (or the raw string is returned and gracefully handled); no panic.
2. Insert an edge with metadata=NULL. Assert: EdgeRecord.metadata is JSON null.
3. Insert an edge with metadata='{"key":"value"}'. Assert: EdgeRecord.metadata is parsed JSON object.

**Coverage Requirement**: All three metadata column states handled without panic.

---

## Integration Risks

**IR-01: graph_read_neighbors.rs follow_to_current re-use visibility**
`follow_to_current` must be `pub(super)` or duplicated. If left `pub(crate)` in neighbors and imported in subgraph, a future refactor could break it. Code review must verify the chosen visibility is intentional and documented.

**IR-02: all_non_supersedes_types scope**
If `all_non_supersedes_types` is private to graph_read_neighbors.rs, subgraph must either import via `pub(super)` or maintain its own copy. A stale copy that misses new RelationType variants is a silent correctness bug. The integration test for default traversal (R-14, scenario 1) is the detection mechanism.

**IR-03: vnc-018 graph_read.rs dispatch — handle_graph "subgraph" arm**
The dispatch arm added in graph_read.rs calls `graph_read_subgraph::handle_subgraph`. If the module declaration (#[path]) or function name differs, the error is a compile error — but the exact function signature (store, typed_graph_state, params) must match the callee. Signature mismatch is a compile-time check, not a behavioral test.

**IR-04: Schema v27 index dependency for OR-chain metadata query**
The OR-chain batch query relies on `idx_graph_edges_source_type` and `idx_graph_edges_target_type` from vnc-018 migration v26→v27. If the test DB does not run the migration (or uses an older schema), the metadata query will perform full-table scans silently — no error, just slow. The infra-001 integration test suite must run against a migrated schema.

**IR-05: TypedRelationGraph cold-start**
On cold-start (background tick not yet run), `TypedRelationGraph` is empty. All seeds are absent from `node_index`. Result is empty SubgraphResponse — not an error. The integration test must exercise both warm-graph and cold-graph states to verify both are handled.

---

## Edge Cases

| Edge Case | Risk | Test Scenario |
|-----------|------|---------------|
| seed_ids=[N] where N is valid but has no edges | R-04 | nodes=[N], edges=[], truncated=false, depth_reached=0 |
| seed_ids=[N] where N not in graph | IR-05 | Empty SubgraphResponse; no error (AC-17) |
| max_depth=1 on deep graph | R-08 | Only direct neighbors of seeds; depth_reached=1 |
| All seed IDs are the same (duplicates) | R-01 | Visited set deduplicates; single node in result |
| edge_types=["UnknownType"] | — | Validation error naming the bad value and listing 16 valid types (AC-08) |
| direction="invalid" | — | Validation error (FR-05) |
| max_depth=0 | R-07 | Validation error (AC-06) |
| max_depth=11 | R-07 | Validation error (AC-06) |
| Large seed set (50 seeds) + max_depth=1 | R-03 | Seeds occupy 50 of 200 budget; BFS expands remaining 150 slots |
| Graph with self-loop (A → A via some edge) | R-01 | Visited set prevents A being enqueued twice; no infinite loop |
| edge with metadata that is valid JSON array | R-15 | metadata deserialized as serde_json::Value::Array; no panic |

---

## Security Risks

**SEC-01: seed_ids input — integer injection**
`seed_ids: Vec<u64>` are used as bind parameters in SQL batch queries (node hydration, metadata fetch). Bind parameters prevent SQL injection. Risk is negligible but must be confirmed: no string interpolation of seed_ids into SQL query text.

**SEC-02: edge_types input — string-to-enum validation**
`edge_types: Vec<String>` are validated via `RelationType::from_str` before any use. Unrecognized values are rejected with a validation error (AC-08). No raw string from caller is interpolated into SQL. Risk: negligible given the enum gate.

**SEC-03: OR-chain SQL construction from collected_edges**
The dynamically built OR-chain SQL uses bind parameters (`?1`, `?2`, ...) — not string concatenation of user-supplied values. The triples used (source_id, target_id, relation_type) come from the in-memory graph traversal result, not directly from caller input. Blast radius: limited to the read-only metadata query on a bounded edge set.

**SEC-04: max_nodes/max_depth — resource exhaustion**
A caller supplying max_depth=10 + no edge_types filter + direction="both" on a dense 3k-node graph triggers worst-case BFS. The 200-node cap ensures BFS terminates. The lock is held only for the graph clone. Worst-case: 200 Store::get() calls (resolve_supersessions=true) + 1 batch hydration + 1 OR-chain query. This is bounded and not an amplification vector.

**SEC-05: metadata TEXT deserialization**
`GRAPH_EDGES.metadata` is TEXT, deserialized via `serde_json::from_str`. Malformed JSON must not panic the server. The correct posture is `serde_json::from_str(...).ok()` → `Option<Value>` with None on error. If the implementation uses `.unwrap()` or `.expect()`, a single bad metadata row brings down the tool call. (See R-15.)

---

## Failure Modes

| Failure | Expected Behavior |
|---------|------------------|
| TypedRelationGraph empty (cold-start) | Empty SubgraphResponse; no error; no SQL fallback |
| All seeds absent from graph | Empty SubgraphResponse; depth_reached=0; no error (AC-17) |
| max_nodes hit during seed phase | truncated=true; depth_reached=0; BFS skipped |
| max_nodes hit during BFS at depth N | truncated=true; depth_reached=N; partial node+edge set returned |
| OR-chain empty (no edges collected) | Metadata query skipped; all EdgeRecord.metadata=None (AC-19) |
| follow_to_current exceeds 50-hop guard | Returns None; BFS uses original deprecated node ID as fallback |
| follow_to_current chain is circular | 50-hop guard terminates; None returned; fallback to original |
| Batch node hydration: missing ENTRIES row | Node absent from nodes silently (no panic); partial result |
| metadata TEXT column: malformed JSON | metadata=None for that edge; tool call succeeds |
| RwLock poisoned | Poison recovery via unwrap_or_else(|e| e.into_inner()); BFS proceeds with recovered state |
| Serialization error on SubgraphResponse | INTERNAL_ERROR returned; no panic |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (in-memory BFS staleness — silent data loss) | R-11 | ADR-004: tool description text is sole disclosure. No graph_rebuilt_at field. Accepted; disclosure verified by code review of tools.rs. |
| SR-02 (truncated bool insufficient — no reason for truncation) | R-16 | Accepted per C-09. Structured truncation reason deferred to W1B-2c. Test must document re-query pattern in tool description. |
| SR-03 (edge batch query unbounded within cap) | R-04 | ADR-003: OR-chain bounded by max_nodes=200 cap (~600 edges max). Empty-edge guard added. Empty-edge path is now a test scenario (AC-19). |
| SR-04 (graph_read.rs file-limit risk) | R-10 | ADR-002: decided upfront — graph_read_subgraph.rs new file. Compile check is the test. |
| SR-05 (resolve_supersessions inline I/O) | R-06, R-13 | Accepted: 50-hop guard + max_nodes=200 cap bounds worst case to 200 Store::get calls. Circular chain and None fallback are now explicit test scenarios. |
| SR-06 (vnc-018 dependency) | IR-03, IR-04 | Accepted sequencing dependency. Delivery gated on vnc-018 merge. Integration tests require schema v27. |
| SR-07 (validate_no_unsupported_params regression) | R-05 | Extended: AC-11 covers seed_ids; AC-16 covers max_depth on non-subgraph modes. 8 specific regression scenarios in R-05. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | 10 scenarios minimum |
| High | 5 (R-04, R-05, R-06, R-07, R-11) | 15 scenarios minimum |
| Med | 7 (R-08, R-09, R-10, R-12, R-13, R-14, R-15) | 14 scenarios minimum |
| Low | 1 (R-16) | 1 scenario minimum |
