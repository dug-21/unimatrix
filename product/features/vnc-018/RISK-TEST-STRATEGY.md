# Risk-Based Test Strategy: vnc-018

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `chain`/`current` modes silently use `find_terminal_active` (in-memory graph) instead of SQL CTE — wrong results on cold-start or stale tick | High | Med | Critical |
| R-02 | `Truncated` struct serialized as flat `bool` instead of `{forward, backward}` — AC-03b untestable, agents cannot distinguish per-direction cap | High | Med | Critical |
| R-03 | depth=1 SQL path and depth>1 BFS path diverge in behavior after immediate writes — agents interpret depth>1 silence as missing data, not documented staleness | High | High | Critical |
| R-04 | `validate_no_unsupported_params` ordering wrong — unrecognized mode falls through to unsupported-field checks before `_ => error` arm, producing misleading error message | High | Med | Critical |
| R-05 | Schema cascade incomplete — one of the 7 v27 touch points missed (`migration.rs`, `db.rs`, `sqlite_parity.rs`, `server.rs`, previous migration test, new migration test, `test_schema_version` in db.rs); test failure not immediately traceable | High | High | Critical |
| R-06 | `Supersedes` in `edge_types` has two paths (explicit rejection vs. silent exclusion from "all types") — one path is untested, silent path emits unexpected `excluded_types` field or warning | High | Med | Critical |
| R-07 | `node_index` on `TypedRelationGraph` is `pub(crate)` within `unimatrix-engine`; `graph_read.rs` is in `unimatrix-server` — depth>1 BFS cannot access `node_index` without a new accessor or an engine-side BFS function | Low | Low | Low |
| R-08 | `resolve_supersessions=true` passed to `chain` mode is not rejected — scope mandates an error, but the field is optional and may be silently ignored if `validate_no_unsupported_params` doesn't check it | Med | Med | High |
| R-09 | PPR out-degree normalization changes for nodes with `Advances`/`Motivates` edges — search re-ranking shifts in ways existing tests do not detect | Med | Med | High |
| R-10 | `follow_to_current` 50-hop cap returns `None` — caller silently uses original deprecated ID instead of surfacing the cap-fire to the caller | Med | Med | High |
| R-11 | `depth` parameter upper bound (1..=10) not validated — `depth=0` or `depth=255` accepted, producing empty result or runaway BFS | Med | Med | High |
| R-12 | `neighbors` non-existent anchor ID behavior undefined (spec OQ-01 open) — implementation returns error while spec implies empty; integration test mismatches expected behavior | Med | High | High |
| R-13 | `tools.rs` wiring uses unqualified module path — compile error or wrong dispatch; Pattern #4436 violation | Med | Med | High |
| R-14 | `test_protocol.py` P-03 not updated from 13 to 14 tools — Gate 3b failure; historical precedent from lesson #4437 (vnc-015) | Med | High | High |
| R-15 | `EdgeRecord.metadata` serializes as something other than `null` — downstream consumers in #597/#598 build on wrong wire format assumption | Med | Low | Medium |
| R-16 | `GraphParams` forward-compat fields (`seed_ids`, `from_id`, `to_id`, `max_nodes`) silently dropped by serde `#[serde(deny_unknown_fields)]` absent — round-trip correctness not verified | Med | Low | Medium |
| R-17 | `direction` parameter not validated on `neighbors` mode — `direction="forward"` accepted without error (forward/backward are only valid for `chain`; neighbors uses incoming/outgoing/both) | Med | Med | Medium |
| R-18 | BFS visited set accidentally keyed on `(node_id, depth)` instead of `node_id` alone — same node reachable via two paths at different depths appears at both depths, producing duplicate records; correct behavior is one record per node at the shallowest depth | Med | Med | Medium |
| R-19 | vnc-017 not merged before delivery branch cut — base codebase missing 16 `RelationType` variants, `edge_write.rs`, `query_incoming_edges`; all neighbors tests fail at compile | High | Med | Critical |
| R-20 | `current` mode CTE includes `AND e.status = 'Active'` filter — an orphaned deprecated entry (deprecated with no successor) produces zero rows and the correct "no active terminal found" error, but only if the status filter is present; accidental omission of the filter returns the orphaned deprecated entry as if it were the terminal active | High | Med | Critical |
| R-21 | `current` mode on a non-existent ID returns an error, not empty — this is intentionally asymmetric with `chain` mode (AC-04 returns empty). The asymmetry is correct by design but a developer "fixing" consistency will silently break AC-05a | Med | Med | High |

---

## Risk-to-Scenario Mapping

### R-01: chain/current Modes Using In-Memory Graph Instead of SQL CTE
**Severity**: High
**Likelihood**: Med
**Impact**: On cold-start (before first tick completes), `find_terminal_active` returns `None` for any valid ID. Results silently wrong — handler may not detect the failure. Tick-window staleness causes intermittent correctness failures in CI if tests run immediately after server startup.

**Test Scenarios**:
1. Call `chain` mode before the first tick interval completes (cold-start database). Assert the correct chain is returned (not empty, not an error).
2. Call `current` mode on a deprecated entry immediately after `context_correct` creates a successor (before next tick). The corrected CTE uses `AND e.status = 'Active'` — so this tests the SQL path traverses `superseded_by` links and correctly returns the active terminal, not the deprecated seed entry. Assert the correct terminal active entry ID is returned.
3. Unit test `query_supersession_chain` in isolation against a freshly migrated database with zero tick cycles completed. Assert correct results.

**Coverage Requirement**: At least one test must explicitly verify that `find_terminal_active` (in-memory path) is NOT called by `handle_chain` or `handle_current`. The SQL path must be tested in a context where the in-memory graph is empty/cold.

---

### R-02: Truncated Struct Wire Format — Flat Bool vs. Per-Direction Struct
**Severity**: High
**Likelihood**: Med
**Impact**: If `truncated` serializes as `"truncated": true` instead of `"truncated": {"forward": true, "backward": false}`, AC-03b is untestable and the Python integration suite will either fail (TypeError on field access) or pass incorrectly. Wire contract break vs. #597/#598 consumers.

**Test Scenarios**:
1. AC-03b: Construct chain with 55 forward hops and 3 backward hops. Call `chain` mode with `direction="both"`. Assert JSON response contains `"truncated": {"forward": true, "backward": false}` — inspect the raw JSON shape, not just the deserialized fields.
2. Call `chain` mode on a short chain (no truncation). Assert response contains `"truncated": {"forward": false, "backward": false}` (struct always present, both false).
3. Call `chain` mode with `direction="forward"` on a 60-hop chain. Assert `truncated.forward=true` and `truncated.backward=false` (backward not set because direction was not queried).

**Coverage Requirement**: One test must inspect the raw JSON wire format of `truncated` — not just a deserialized Rust struct — to confirm the `{"forward": bool, "backward": bool}` shape is what agents see.

---

### R-03: depth=1 SQL vs. depth>1 BFS Behavioral Asymmetry After Writes
**Severity**: High
**Likelihood**: High
**Impact**: An agent writes an edge via `context_edge`, immediately calls `neighbors` with `depth=2`, and does not see the new edge. If the tool description is absent or vague, the agent reports a bug. If the staleness is not tested, a future "fix" that makes depth>1 always-fresh passes tests and breaks the documented contract.

**Test Scenarios**:
1. Write a typed edge from X to Y. Immediately call `context_graph(mode="neighbors", id=X, depth=1)`. Assert the new edge appears (live SQL path).
2. Write a typed edge from X to Y. Immediately call `context_graph(mode="neighbors", id=X, depth=2)`. Assert the new edge does NOT appear (expected staleness — BFS uses pre-tick graph). This tests the absence of the edge, not a bug.
3. Verify the tool description string in `#[tool(description = "...")]` contains the exact text mandated by FR-13 / ADR-005 about depth=1 vs. depth>1 freshness.

**Coverage Requirement**: The staleness test (scenario 2) must be present in the infra-001 suite and must assert the edge is absent. A comment in the test must state this is expected behavior, not a bug, to prevent future "fix" from deleting it.

---

### R-04: validate_no_unsupported_params Ordering — Unrecognized Mode Falls Through
**Severity**: High
**Likelihood**: Med
**Impact**: If the `match` arm for unrecognized mode is not the `_` fallthrough — or if mode validation runs after field validation — a caller passing `mode="subgraph", seed_ids=[1,2,3]` gets "seed_ids not supported in subgraph mode" instead of "unrecognized mode: subgraph". Confusing error; also means when #597 ships, removing the "unrecognized" error for `subgraph` is not enough — the field-check ordering matters.

**Test Scenarios**:
1. AC-15b (×4): Pass `seed_ids` to `mode="neighbors"`. Assert error contains "seed_ids" and "subgraph". Pass `from_id` to `mode="chain"`. Assert error contains "from_id" and "path". Repeat for `to_id` and `max_nodes`.
2. Pass `mode="subgraph", seed_ids=[1]`. Assert error message says "unrecognized mode" — not "seed_ids not supported".
3. Pass `mode="walk"` (unrecognized, no forward-compat fields). Assert error lists "chain, current, neighbors".

**Coverage Requirement**: One test must pass an unrecognized mode with a forward-compat field present and assert the "unrecognized mode" error fires first.

---

### R-05: Schema Cascade Incomplete — v27 Touch Points
**Severity**: High
**Likelihood**: High
**Impact**: Missing one of the 7 mandatory touch points causes a test failure that may be in an unrelated-looking test (e.g., `test_schema_version_is_26` still passing because the literal was not bumped). Pattern #4373 and #4153 document this as a recurring failure mode across features.

**Test Scenarios**:
1. AC-19: After migration, query `sqlite_master` for all four index names. Assert all present.
2. Assert `CURRENT_SCHEMA_VERSION == 27` in `migration.rs`.
3. Assert `db.rs::create_tables_if_needed` includes all four index DDL statements (code inspection test or unit test on a fresh DB).
4. Assert `server.rs` has no `assert_eq!(version, 26)` assertions remaining after the bump.
5. Assert previous migration test (`migration_v25_to_v26.rs`) uses `assert!(version >= 26)` not `== 26`.
6. Assert new migration test (`migration_v26_to_v27.rs`) exists and verifies all four index names.

**Coverage Requirement**: All 7 cascade items from ADR-007 checklist must be verified before Gate 3b sign-off. Delivery agent must run `grep -r 'schema_version.*== 26' crates/` and confirm zero matches (ADR-007 explicit instruction).

---

### R-06: Supersedes Exclusion — Two Paths, One Untested
**Severity**: High
**Likelihood**: Med
**Impact**: The explicit-rejection path ("Supersedes edges are not traversable via neighbors mode") and the silent-exclusion path ("all types" default excludes Supersedes without warning) must both be exercised. If only one is tested, the other may silently emit an `excluded_types` field (spec explicitly prohibits this — AC-10a).

**Test Scenarios**:
1. AC-15a: Call `neighbors` with `edge_types=["Supersedes"]`. Assert exact error string.
2. AC-10: Write edges of type Supports, Informs, and Supersedes from X to different targets. Call with `edge_types=[]`. Assert Supports and Informs targets present, Supersedes target absent. Assert no `excluded_types`, `warnings`, or similar field in the response JSON.
3. AC-10a: Inspect the raw JSON response from AC-10. Confirm no top-level extra field indicating exclusion.
4. Call `neighbors` with `edge_types=["Supersedes", "Supports"]`. Assert error fires — the presence of one valid type alongside Supersedes does not bypass rejection.

**Coverage Requirement**: Both code paths (explicit rejection and silent exclusion) must have independent tests. The raw response JSON shape from the silent-exclusion path must be inspected, not just the returned edge list.

---

### R-07: node_index Visibility — pub(crate) Cross-Crate Access [RESOLVED — ADR-008]
**Severity**: Low (downgraded from Critical)
**Likelihood**: Low
**Resolution**: ADR-008 resolves this by mandating a `pub fn node_index_for(id: u64) -> Option<NodeIndex>` accessor on `TypedRelationGraph` in `unimatrix-engine`. Delivery agent must implement this accessor as part of the feature — the design decision is made, the compile barrier is cleared by design.

**Test Scenarios**:
1. AC-11: `neighbors` depth=2 returns results (proves the accessor compiles and BFS executes). Create chain X→Y→Z via typed edges; call neighbors depth=2; assert Y at depth=1 and Z at depth=2.
2. Unit test on `TypedRelationGraph::node_index_for`: returns correct `NodeIndex` for a known node, returns `None` for unknown node.

**Coverage Requirement**: AC-11 is the end-to-end proof. Unit test on `node_index_for` confirms the accessor contract. No ADR deviation decision needed — ADR-008 is the authoritative record.

---

### R-08: resolve_supersessions on chain Mode Not Rejected
**Severity**: Med
**Likelihood**: Med
**Impact**: `chain` mode is semantically the supersession audit. Applying `resolve_supersessions` within it is circular (per FR-08 and AC-15c). If the field is silently ignored rather than rejected, agents may pass it expecting substitution behavior and receive the raw chain — a silent semantic failure with no error signal.

**Test Scenarios**:
1. AC-15c: Call `context_graph(mode="chain", resolve_supersessions=true)` via the MCP integration path. Assert exact error string: "resolve_supersessions is not applicable to chain mode — chain IS the supersession audit".
2. Unit test `validate_no_unsupported_params` directly with `mode="chain"` and `resolve_supersessions=true`. Assert the validation function returns the error — this is the canonical test location. Do not test this inside `handle_chain`; the check belongs in `validate_no_unsupported_params` so it is guaranteed to fire before any CTE executes.

**Coverage Requirement**: One unit test on `validate_no_unsupported_params` covering AC-15c. This tests the centralized rejection function, not a specific mode handler. The check must NOT be inside `handle_chain`.

---

### R-09: PPR Out-Degree Normalization Shift from Advances/Motivates
**Severity**: Med
**Likelihood**: Med
**Impact**: `positive_out_degree_weight` denominator increases for any node with `Advances` or `Motivates` edges. Nodes that previously had high PPR scores via other positive types may score slightly lower. This is mathematically correct but can shift existing test baselines. If there are hardcoded score assertions in PPR tests, they will fail after the addition.

**Test Scenarios**:
1. AC-17: Unit test enumerates positive types from `graph_ppr.rs`; asserts `Advances` and `Motivates` are present in the set queried by `positive_out_degree_weight` and `personalized_pagerank`.
2. AC-18: Unit test constructs a `TypedRelationGraph` with `Advances` and `Motivates` edges; runs `graph_expand` BFS; asserts nodes connected by those types are returned.
3. Regression: check existing PPR unit tests for hardcoded score values — any `assert_approx_eq!(score, 0.X)` will need updating if the test graph includes nodes with Advances/Motivates edges.

**Coverage Requirement**: ACs 17 and 18 are the regression baseline. Delivery agent must audit existing PPR tests for hardcoded score assertions before merging.

---

### R-10: follow_to_current Returns None — Silent Fallback to Deprecated ID
**Severity**: Med
**Likelihood**: Med
**Impact**: When `resolve_supersessions=true` and a neighbor's supersession chain exceeds 50 hops (or is orphaned), `follow_to_current` returns `None`. ADR-005 specifies "caller treats as: no substitution, use original id." This means a deprecated entry silently remains in the result instead of being substituted or flagged. Agents using this mode for "give me live entries only" will receive stale IDs without indication.

**Test Scenarios**:
1. AC-12: Standard `resolve_supersessions=true` substitution — write edge X→Y, correct Y→Z, call neighbors with resolve=true, assert Z returned.
2. AC-13: Standard `resolve_supersessions=false` — same setup, assert Y (deprecated) returned.
3. Edge case: `resolve_supersessions=true` where the neighbor's supersession chain is orphaned (deprecated entry with no `superseded_by`). Assert the deprecated entry is returned as-is (no substitution, no error). Verify no panic on `None` from `follow_to_current`.
4. Edge case: `resolve_supersessions=true` where `follow_to_current` returns `None` due to 50-hop chain. Assert original ID returned, no crash.

**Coverage Requirement**: The `None` path of `follow_to_current` must be exercised in at least one test. The test must assert graceful fallback, not panic or propagated error.

---

### R-11: depth Parameter Range Not Validated
**Severity**: Med
**Likelihood**: Med
**Impact**: `depth=0` would produce a BFS that starts but never expands — empty result with no error, confusing to callers. `depth=255` with a dense graph could produce a very large result set and high memory usage. The spec states `1..=10` as the valid range (SPECIFICATION.md Constraints §Safety).

**Test Scenarios**:
1. Call `neighbors` with `depth=0`. Assert error response with message indicating valid range (1..=10).
2. Call `neighbors` with `depth=11`. Assert error response.
3. Call `neighbors` with `depth=1` (boundary). Assert valid result.
4. Call `neighbors` with `depth=10` (boundary). Assert valid result (or gracefully bounded result).

**Coverage Requirement**: Both boundary-violation cases (0 and 11) must return errors before any BFS execution. One test per bound.

---

### R-12: neighbors Non-Existent Anchor ID — Error vs. Empty (Spec OQ-01 Open)
**Severity**: Med
**Likelihood**: High
**Impact**: SPECIFICATION.md OQ-01 leaves this open: "Recommend: return empty with no error (consistent with chain mode), but architect should confirm." If the delivery agent implements error and the integration test asserts empty (or vice versa), Gate 3c will fail. The behavior choice is not yet locked.

**Test Scenarios**:
1. Call `neighbors` with `id=999999` (absent entry). Once OQ-01 is resolved, assert either: empty `NeighborsResponse` (no error), or a structured error response. The test assertion must match the chosen behavior.
2. Verify the behavior is consistent with `chain` mode (AC-04 returns empty for non-existent ID) — if consistency is the design intent, both must return empty.

**Coverage Requirement**: OQ-01 must be resolved before delivery begins. The resolution must be recorded as an ADR update or spec amendment. The test assertion must match the final decision.

---

### R-13: tools.rs Wiring — Unqualified Module Path
**Severity**: Med
**Likelihood**: Med
**Impact**: Pattern #4436 (Unimatrix entry): every call from `tools.rs` to a sibling module must use the fully qualified module path. Missing the qualifier causes a compile error or (if there is a name collision) wrong dispatch. `tools.rs` is 9,610 lines — reviewers may miss a missing prefix.

**Test Scenarios**:
1. AC-20: At least one infra-001 integration test exercises the full dispatch chain — MCP call → `tools.rs` handler → `graph_read::handle_graph` → mode handler. This is the end-to-end proof the wiring is correct.
2. Code review check: search `tools.rs` for the `context_graph` handler and confirm `graph_read::handle_graph` is called with a fully qualified path.

**Coverage Requirement**: The integration test (AC-20) is the runtime proof. Static code inspection confirms the qualifier. Both must pass.

---

### R-14: test_protocol.py P-03 Not Updated
**Severity**: Med
**Likelihood**: High
**Impact**: Lesson #4437 documents this exact failure pattern from vnc-015: Gate 3b catches missing protocol test updates. P-03 currently asserts 13 tools. Adding `context_graph` without updating P-03 fails the infra-001 suite at Gate 3b.

**Test Scenarios**:
1. AC-16: `test_protocol.py` P-03 asserts exactly 14 `context_*` tools. Run infra-001 suite and confirm P-03 passes.

**Coverage Requirement**: This is a single mandatory test update. It must be included in the delivery checklist as a non-negotiable item (pattern from lesson #4437 and Unimatrix entry #2758 about non-negotiable test names).

---

### R-15: EdgeRecord.metadata Wire Format
**Severity**: Med
**Likelihood**: Low
**Impact**: `EdgeRecord.metadata` must serialize as `null` in vnc-018 (per spec NFR-07). If it serializes as an absent field (omitted from JSON via `#[serde(skip_serializing_if = "Option::is_none")]`), consumers in #597 that check for `metadata: null` will break.

**Test Scenarios**:
1. Call `neighbors` and inspect the raw JSON of returned `EdgeRecord` items. Assert the `metadata` field is present in the JSON output with value `null` — not absent.

**Coverage Requirement**: One test that inspects the raw JSON shape of an `EdgeRecord`. If serde is configured to skip `None` fields, the test will catch the discrepancy.

---

### R-16: GraphParams Forward-Compat Fields Silently Dropped by Serde
**Severity**: Med
**Likelihood**: Low
**Impact**: If `GraphParams` is deserialized with `#[serde(deny_unknown_fields)]` absent and the fields are not exercised in tests, serde may silently drop them (if they are somehow not in the struct) or accept them silently (which is correct but untested). The risk is discovering at #597 delivery that the round-trip behavior is wrong.

**Test Scenarios**:
1. AC-15b: Pass each forward-compat field (`seed_ids`, `from_id`, `to_id`, `max_nodes`) to a supported mode and assert the validation error fires. This proves the fields are deserialized and inspected, not dropped.

**Coverage Requirement**: The four AC-15b unit tests are sufficient. They prove round-trip correctness by triggering the validation path.

---

### R-17: direction Parameter Not Validated on neighbors Mode
**Severity**: Med
**Likelihood**: Med
**Impact**: `neighbors` uses `incoming`/`outgoing`/`both` for direction. `chain` uses `forward`/`backward`/`both`. If an agent calls `neighbors` with `direction="forward"` (intending the chain semantics), the handler may return empty results or an error depending on whether the string is validated. Undocumented acceptance of invalid direction values creates confusion.

**Test Scenarios**:
1. Call `neighbors` with `direction="forward"`. Assert error message identifies valid directions for neighbors mode: "incoming", "outgoing", "both".
2. Call `neighbors` with `direction="incoming"` (valid). Assert results returned.
3. Call `neighbors` with `direction="both"` (valid). Assert results returned.

**Coverage Requirement**: One negative test (invalid direction string) and two positive tests covering valid direction values. The error message must distinguish neighbors-mode directions from chain-mode directions.

---

### R-18: BFS Visited Set Keying — Same Node, Different Depths
**Severity**: Med
**Likelihood**: Med
**Impact**: If the visited set is `HashSet<u64>` (node IDs only), a node reachable at both depth=1 (via type A) and depth=2 (via type B through an intermediate) will appear only once — at whichever depth is encountered first. The BFS visited-set decision is: **key on node ID alone**, so the shallowest encounter wins and re-expansion is suppressed. This is the correct and decided behavior (prevents cycles, keeps result compact). The risk is that an implementation accidentally uses `HashSet<(u64, u8)>` keyed on `(node_id, depth)`, which allows duplicate node entries at different depths.

**Test Scenarios**:
1. Construct a graph where X connects to Z directly via one edge type (depth=1), AND X connects to Y which connects to Z via a different edge type (Z also reachable at depth=2). Call `context_graph(mode="neighbors", id=X, depth=2)`. Assert Z appears exactly once in the result, at depth=1. Verify no duplicate Z record at depth=2. This is the definitive test of the visited-set keying decision.
2. Verify R-18 scenario 1 passes with the BFS using `HashSet<u64>` (node ID only) — a test failure here means the visited set was inadvertently keyed on `(node_id, depth)`.

**Coverage Requirement**: One explicit test asserting a node reachable via two paths at different depths appears exactly once at the shallowest depth. This is the only way to catch an incorrect `(node_id, depth)` keying implementation.

---

### R-19: vnc-017 Not Merged Before Delivery Branch Cut
**Severity**: High
**Likelihood**: Med
**Impact**: vnc-018 requires the post-vnc-017 codebase state (16 `RelationType` variants in `graph.rs`, `edge_write.rs`, `query_incoming_edges`). Cutting the delivery branch from pre-vnc-017 state means `RelationType::from_str()` does not recognize the 10 new types — every `neighbors` call with a W1B-1 type string returns "unknown edge type" error. All AC-08 through AC-13 tests fail.

**Test Scenarios**:
1. Gate-0 check: verify the delivery branch was cut from a commit that includes vnc-017's merged state. Assert `graph.rs` contains all 16 `RelationType` variants.
2. Smoke test at delivery start: call `neighbors` with `edge_types=["Advances"]` and assert no "unknown edge type" error.

**Coverage Requirement**: This is a pre-delivery gate check, not a test scenario. Delivery must not begin without verifying the branch base. The smoke test is a guard against accidental wrong-base delivery.

---

### R-20: current Mode — Orphaned Deprecated Entry and status = 'Active' Filter
**Severity**: High
**Likelihood**: Med
**Impact**: The `current` mode CTE contains `AND e.status = 'Active'` in its base case. If this filter is accidentally omitted, an orphaned deprecated entry (deprecated with no successor, no `superseded_by` value — or one where the chain terminates at a deprecated node) is returned as if it were the terminal active entry. The symptom is silent wrong results, not an error. The `status = 'Active'` filter is the only guard against this — no other logic prevents it.

There are three distinct `current` mode failure paths, all of which must produce the "no active terminal found" error (not empty, not a wrong entry):
- Non-existent ID → CTE returns zero rows → error
- Chain terminates at orphaned deprecated entry → `status = 'Active'` filter drops it → zero rows → error
- Chain exceeds 50 hops without reaching an active terminal → depth cap fires → error (AC-07)

**Test Scenarios**:
1. Create an entry, then call `context_deprecate` on it with no successor (no `superseded_by` set). Call `context_graph(mode="current", id=that_entry)`. Assert the response is an error containing "no active terminal found" — not empty, not the deprecated entry itself.
2. Verify the error message for scenario 1 is identical to the error produced by a non-existent ID (AC-05a). The trigger is different but the observable error must be the same.
3. Unit test: construct a CTE result set containing a single row with `status = 'Deprecated'`. Assert the handler returns "no active terminal found" — this directly tests the `status = 'Active'` filter path without requiring a full DB fixture.

**Coverage Requirement**: Scenario 1 is the only integration test that can catch an accidentally omitted `AND e.status = 'Active'` filter. It is a non-negotiable test. The unit test (scenario 3) is the fast-feedback companion.

---

### R-21: current Mode on Non-Existent ID — Error Not Empty (Behavioral Asymmetry with chain)
**Severity**: Med
**Likelihood**: Med
**Impact**: `chain` mode on a non-existent ID returns empty (`ChainResponse` with no entries, AC-04). `current` mode on a non-existent ID returns an error ("no active terminal found", AC-05a). This asymmetry is intentional: `chain` is a traversal that may have zero hops; `current` is a lookup that must find exactly one active terminal or fail. A developer who perceives this as inconsistency and "fixes" it to return empty for `current` silently breaks the contract that callers depend on for error-signaling.

**Test Scenarios**:
1. AC-05a: Call `context_graph(mode="current", id=999999)` where 999999 does not exist. Assert an error response is returned — not an empty result.
2. Call `context_graph(mode="chain", id=999999)` (same non-existent ID). Assert an empty `ChainResponse` with no error. This pair of tests documents the intentional asymmetry in the test suite — both must exist and both must pass for the asymmetry to be considered verified.

**Note**: A test comment must explicitly state that `current` returning an error and `chain` returning empty for the same non-existent ID is intentional, designed behavior — not a bug to be fixed. This prevents future "consistency" regressions.

**Coverage Requirement**: Both AC-04 (chain empty) and AC-05a (current error) must be present as a pair in the infra-001 suite. The asymmetry is a semantic contract, not an accident.

---

## Integration Risks

### IR-01: Two SQL Functions in db.rs — Scope Growth Risk
Adding `query_supersession_chain` and `query_direct_neighbors` to `db.rs` (or a new store submodule) introduces two new async functions. The 500-line limit applies only to new modules, not `db.rs`, but the new functions must be tested in the store-layer unit tests, not just at the MCP integration level. A store-layer test that exercises `query_supersession_chain` with an empty database, a single-entry chain, and a capped chain independently of the MCP handler is required.

### IR-02: Arc<RwLock<TypedGraphState>> Lock Acquisition Under Concurrent Load
depth>1 BFS acquires `Arc<RwLock<TypedGraphState>>::read()` once and holds it for the duration of the BFS. The tick thread acquires a write lock to rebuild the graph. Under concurrent BFS queries, the read lock blocks tick rebuilds. For depth>1 with a dense graph (many hops, many edge types), the hold time could be non-trivial. No BFS time SLA exists in NFR-03, but the tick-blocking risk should be noted.

### IR-03: direction Parameter Overloading Across Modes
`direction` is used by both `chain` mode ("forward"/"backward"/"both") and `neighbors` mode ("incoming"/"outgoing"/"both"). The same field name carries different semantics per mode. If validation does not enforce mode-appropriate values, cross-mode direction strings produce wrong behavior. IR ties to R-17.

### IR-04: follows_to_current Store Reads on depth>1 BFS Hot Path
When `resolve_supersessions=true` at depth>1, `follow_to_current` is called per hop. For a wide graph at depth=10 with many deprecated nodes, this is many sequential `store.get()` calls inside the BFS loop (not parallelized). Under the tick cycle, these reads may observe a DB that is mid-migration. No data corruption risk (reads only), but latency risk.

---

## Edge Cases

| Edge | Risk | Required Scenario |
|------|------|-------------------|
| `chain` mode on a single-entry chain (no supersessions) | Returns only that entry; `truncated: {false, false}` | AC-05 analog for chain mode |
| `chain` on an entry that is both superseded and supersedes others (mid-chain node) | Both forward and backward branches return results | AC-01 with seed at mid-chain |
| `current` on an entry already active (no `superseded_by`) | Returns that entry unchanged, not an error | AC-05 |
| `current` with a 50-hop chain that has no terminal (all entries have `superseded_by`) | CTE terminates with zero rows WHERE superseded_by IS NULL — error path, not empty result | AC-07 |
| `neighbors` depth=1 with zero edges from anchor | Returns empty `NeighborsResponse`, no error | Needed for completeness |
| `neighbors` with all 15 non-Supersedes edge types explicitly listed | Same as `edge_types=[]` semantically — should not error on 15-element list | Boundary of explicit vs. default path |
| `chain` direction="both" on an entry with no forward chain (terminal descendant) | Forward CTE returns only the seed; backward CTE returns the full chain; `truncated.forward=false` | Directionality completeness |
| `neighbors` depth=2 with `resolve_supersessions=true` where first-hop target is deprecated | `follow_to_current` resolves first hop; BFS then expands from the resolved (live) ID, not the deprecated one | R-10 interaction with multi-hop |
| `chain` on ID=0 or very large ID (u64::MAX) | Empty result (non-existent IDs), no integer overflow | Numeric boundary |
| `neighbors` with `edge_types=["CoAccess"]` | CoAccess is a valid type — must not be accidentally excluded alongside Supersedes | Supersedes-exclusion precision |

---

## Security Risks

### Untrusted Input Surface
`context_graph` accepts: `mode` (string), `id` (u64), `direction` (string), `edge_types` (Vec<String>), `depth` (u8), `resolve_supersessions` (bool), `agent_id` (string), `format` (string), and four forward-compat fields (typed).

**mode string injection**: Mode is matched as a literal string in `validate_no_unsupported_params`. No SQL interpolation. Blast radius: error response for unrecognized mode. Low risk.

**id field**: Passed as a parameterized SQL bind variable (`?1`) — no injection risk. Invalid u64 values are rejected at serde deserialization before reaching the handler.

**edge_types strings**: Each string is validated via `RelationType::from_str()` before any SQL query. The SQL query uses `IN (?, ?, ...)` with bound parameters — the type strings are not interpolated into the query. Blast radius: error response for unknown types.

**depth field**: u8 — bounded by type at deserialization. Validation to 1..=10 adds a second layer. An adversarial `depth=10` with a maximally dense graph could produce a large result set (bounded by graph density × depth), but no SQL injection risk.

**agent_id string**: Used only for audit attribution. Not interpolated into queries. SQL-injection risk: none (parameterized). Stored in audit log — XSS risk is not applicable in the MCP context.

**No write path**: All three modes are read-only (`Capability::Read`). No GRAPH_EDGES or entries rows are written by this tool. Blast radius for any injection attempt is read-only exposure of the knowledge graph, not data mutation.

**Assessment**: Low security risk. The primary attack surfaces (SQL parameters, type validation) are protected by parameterized queries and enum validation. The `depth` parameter is the most likely source of resource exhaustion, not injection.

---

## Failure Modes

| Failure | Expected Behavior | Testable? |
|---------|------------------|-----------|
| `id` not found in entries — chain mode | Empty `ChainResponse`, no error (AC-04) | Yes — AC-04 |
| `id` not found — current mode | Error: "no active terminal found" — NOT empty (AC-05a; intentionally asymmetric with chain mode which returns empty) | Yes — AC-05a |
| Orphaned deprecated entry (deprecated, no successor) — current mode | Error: "no active terminal found" — `status = 'Active'` filter produces zero rows; same error as non-existent ID (R-20) | Yes — R-20 scenario 1 |
| `id` not found — neighbors mode | Empty `NeighborsResponse` or error (OQ-01 pending) | Blocked on OQ-01 resolution |
| Supersession chain forms a cycle (A supersedes B, B supersedes A) | CTE depth cap fires at 50; current mode returns "chain too long" error | Yes — requires test fixture with cycle via `UPDATE entries.supersedes` (ref: pattern #4104) |
| `follow_to_current` returns `None` (50-hop or orphaned) | BFS uses original deprecated ID, no error (ADR-005 spec) | Yes — R-10 scenario 4 |
| In-memory graph read lock unavailable (write lock held by tick) | `Arc::read()` blocks until tick releases — no error, latency impact only | No explicit test needed; covered by normal concurrent test execution |
| Database connection pool exhausted | `StoreError` propagated up to handler; MCP error response returned | Covered by existing error propagation tests |
| `validate_no_unsupported_params` called with all four forward-compat fields set simultaneously | All four errors reported or first-encountered reported; no panic | Yes — one test with all four fields set |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: Missing indexes cause O(N) CTE scans | R-05 | ADR-007 adds all four indexes atomically in migration v27. Schema cascade checklist (Pattern #4373) is the delivery gate. |
| SR-02: depth=1 vs. depth>1 behavioral asymmetry misleads agents | R-03 | ADR-005 locks the split and mandates exact tool description text (FR-13). R-03 scenario 2 tests expected staleness, not a bug. |
| SR-03: GraphParams/EdgeRecord forward-compat contracts | R-04, R-16 | ADR-003 locks struct layout and centralized validation. AC-15b (four unit tests) is the delivery gate for error-on-misuse. |
| SR-04: Silent Supersedes exclusion produces no warning | R-06 | Spec AC-10a explicitly prohibits `excluded_types` field. R-06 scenario 2 and 3 inspect raw JSON response to assert no warning field. |
| SR-05: TruncationStatus must be per-direction | R-02 | ADR-002 defines `Truncated { forward: bool, backward: bool }`. R-02 scenario 1 inspects raw JSON wire format. |
| SR-06: Advances/Motivates PPR/BFS bundled change | R-09 | ADR-006 adds both types. AC-17 and AC-18 are the regression baseline. Delivery agent must audit existing PPR tests for hardcoded score values. |
| SR-07: chain/current must not use find_terminal_active | R-01, R-20 | ADR-001 prohibits in-memory path for chain/current. R-01 scenario 2 tests cold-start CTE correctness. R-20 adds orphaned-deprecated-entry as a separate Critical risk — the `AND e.status = 'Active'` CTE filter is the only guard; R-20 scenario 1 is the only test that catches its accidental omission. |
| SR-08: vnc-017 branch dependency | R-19 | Gate-0 check: delivery branch must be cut from post-vnc-017 merged main. Smoke test confirms 16 RelationType variants accessible. |
| SR-09: tools.rs wiring pattern #4436 | R-13 | ARCHITECTURE.md mandates fully-qualified module path. AC-20 integration test is runtime proof of correct dispatch. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 8 (R-01, R-02, R-03, R-04, R-05, R-06, R-19, R-20) | 28 scenarios |
| High | 8 (R-08, R-09, R-10, R-11, R-12, R-13, R-14, R-21) | 18 scenarios |
| Medium | 5 (R-15, R-16, R-17, R-18 + edge cases) | 10 scenarios |
| Low | 1 (R-07 — resolved by ADR-008) | 2 scenarios |

**Non-negotiable tests** (Gate 3b/3c failures documented in lessons #4437, #2758):
- AC-16: P-03 asserts 14 tools — mandatory, no exceptions
- AC-19: Four indexes present after migration
- AC-03b: `truncated.forward` / `truncated.backward` independently asserted as per-direction struct
- R-03 staleness test: write + immediate depth=2 query asserts edge ABSENT (not a bug)
- R-20 orphaned-deprecated test: deprecated entry with no successor returns "no active terminal found" error — the only test that catches an omitted `AND e.status = 'Active'` filter
- AC-05a / R-21 asymmetry pair: `current` on non-existent ID returns error; `chain` on same ID returns empty — both must be present as a matched pair

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned failures gate rejection — found #4473 (warn+continue masks failure-path tests, vnc-017), #4437 (missing tool count assertion, vnc-015), #2758 (non-negotiable test names at Gate 3c), #4177 (tautological assertions). All four informed R-14, R-06, and failure mode assertions.
- Queried: `/uni-knowledge-search` for SQL CTE migration pattern — found #4373 (schema version cascade checklist, directly informs R-05), #4153 (three-path schema bump, informs R-05), #4468 (SQL CTE for supersession — confirms ADR-001 correctness).
- Queried: `/uni-knowledge-search` for TypedRelationGraph BFS staleness — found #3650 (edges_of_type filter boundary pattern, confirms R-07 node_index concern), #4479 (ADR-005 vnc-018 neighbors split, confirms R-03 design).
- Queried: `/uni-knowledge-search` for PPR regression — found #3896 (PPR regression test trap: both edges required for test correctness, informs R-09), #3992 (PPR expander BFS architecture, confirms Advances/Motivates direction reasoning).
- Stored: nothing novel to store at this amendment pass — R-07 resolved by ADR-008 (pub fn node_index_for accessor pattern); will store as reusable pattern post-delivery once the implementation confirms the accessor approach is correct. R-20 (CTE status filter omission risk) and R-21 (chain/current behavioral asymmetry) are feature-specific risks; no cross-feature pattern established yet.
