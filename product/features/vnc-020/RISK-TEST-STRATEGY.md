# Risk-Based Test Strategy: vnc-020

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `path` mode staleness disclosure absent or incorrect in tool description — agents apply freshness model of `inverse`/`filter` to `path` results | High | Med | Critical |
| R-02 | `max_edge_count=0` boundary in `filter` mode returns wrong results — `COUNT(*) = 0` vs `COUNT(*) >= N` are structurally different SQL shapes | High | Med | Critical |
| R-03 | BFS visited set keyed on raw neighbor ID, not resolved ID — double-enqueue when multiple deprecated nodes share one terminal successor | High | Med | Critical |
| R-04 | `validate_no_unsupported_params` incomplete rejection matrix — a missed rejection on any of 8 new fields × 7 modes lets wrong-mode params silently reach the wrong handler | High | Med | Critical |
| R-05 | `inverse` mode AND semantics — callers expecting OR get unexpected narrow results; no test validates the case where an entry has SOME but not ALL specified types | Med | High | High |
| R-06 | `path` mode `resolve_supersessions=true` response reflects original ID instead of resolved ID in `from_id`/`to_id` fields — caller cannot reconstruct the actual path taken | Med | Med | High |
| R-07 | `depth` rejection behavior change breaks existing callers who pass `depth` to `chain`/`current`/`subgraph`/`inverse`/`filter` silently — now returns validation error | Med | Med | High |
| R-08 | `filter` mode with both `min_edge_count` and `max_edge_count` present generates two independent correlated subqueries — an implementation using a single subquery with AND bounds produces wrong results | Med | Med | High |
| R-09 | `path` mode no-path vs. not-in-snapshot both return `found: false` — test gap if distinct error surface is missing (AC-14 vs AC-15 are same wire shape but different internal paths) | Med | Med | High |
| R-10 | SQL antijoin in `inverse` mode includes deprecated entries (status != 0) if `AND e.status = 0` clause is omitted during dynamic SQL construction | Med | Med | High |
| R-11 | `filter` mode category-only query (no property filters, no edge-count filters) is valid per spec but may be untested — returns all active entries in a category up to limit | Low | Med | Medium |
| R-12 | Path response `length` field diverges from `hops.len()` — off-by-one during path reconstruction if `from_id` is accidentally included in the hops array | Low | Med | Medium |
| R-13 | `inverse` mode `limit` out-of-range boundary (0 and 501) produces wrong default behavior or panic instead of a validation error | Low | Low | Low |
| R-14 | `RelationType::from_str` wildcard arm fires before a legitimate type name for any newly added variant — causes incorrect "unrecognized edge type" error for valid inputs | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: path Mode Staleness Disclosure Absent or Incorrect

**Severity**: High
**Likelihood**: Med
**Impact**: AI agents treating `path` results as live-DB fresh will make stale-data decisions (e.g., assert a path exists based on an edge written within the last tick interval). Agents may also fail to handle `{ found: false }` as a non-error when a node is absent from the snapshot.

**Test Scenarios**:
1. Code review inspection: verify `tools.rs` tool description for `context_graph` contains the exact staleness disclosure text specified in ARCHITECTURE.md §Staleness Disclosure — including the phrase "The cache is rebuilt each tick (typically 30–60 seconds)".
2. Unit test: `handle_path` called with a `from_id` that exists in the DB but NOT in the current `TypedRelationGraph` snapshot returns `{ found: false, hops: [], length: 0 }` and NOT an error code.
3. Integration test: `inverse` and `filter` mode tool descriptions do NOT contain staleness language — verify no "tick" or "cache" language appears in those mode descriptions.

**Coverage Requirement**: AC-15 (not-in-snapshot is `found: false`, not error), AC-19 (exact disclosure text present). Manual inspection gate for tools.rs description string.

---

### R-02: `max_edge_count=0` Boundary Returns Wrong Results

**Severity**: High
**Likelihood**: Med
**Impact**: Q10 stale Goal detection — the primary `filter` mode use case — silently returns incorrect results if `<= 0` is structurally implemented as `>= 0` or `= 0` only works via the general path but not the boundary value.

**Test Scenarios**:
1. Integration test (AC-29): write 4 goal entries — 0, 1, 2, 3 outgoing `Advances` edges respectively. Call `filter(category="goal", max_edge_count=0, edge_types=["Advances"])`. Assert exactly the entry with 0 edges is returned; entries with 1, 2, 3 edges are absent.
2. Integration test: call with `max_edge_count=1` on the same data. Assert entries with 0 and 1 edges are returned; 2 and 3 are absent.
3. Unit test: SQL string constructed for `max_edge_count=0` uses `<= ?` binding with value 0 — not a special-cased `= 0` or `IS NULL` form.

**Coverage Requirement**: The `= 0` boundary must be tested as a distinct case from `>= 1` (AC-29). Both the SQL form and the result set must be verified.

---

### R-03: BFS Visited Set Keyed on Raw Neighbor ID (Double-Enqueue)

**Severity**: High
**Likelihood**: Med
**Impact**: When `resolve_supersessions=true` and multiple deprecated nodes share the same terminal successor, the terminal node is enqueued multiple times. This produces a path that revisits the same node — violating shortest-path invariant and possibly causing infinite BFS loops in degenerate graphs. Pattern #4494 documents this exact failure mode from vnc-019.

**Test Scenarios**:
1. Unit test: construct a graph with two deprecated nodes A_dep and B_dep both superseded by C_active, with edges from_id→A_dep and from_id→B_dep. Call `path(from_id, to_id=C_active, resolve_supersessions=true)`. Assert path is returned correctly (not a loop) and C_active appears exactly once in hops.
2. Unit test: verify the visited set is keyed on the resolved ID — when `follow_to_current(A_dep) = C_active` and `follow_to_current(B_dep) = C_active`, C_active's visited entry is checked before enqueueing the second reference.
3. Integration test: three-node deprecated chain (A_dep→B_dep→C_active) used as intermediate path node with resolve_supersessions=true — assert only C_active appears in hops, not the deprecated intermediaries.

**Coverage Requirement**: At minimum one unit test with a forked deprecated-supersession graph to validate the visited-set invariant. References pattern #4494.

---

### R-04: `validate_no_unsupported_params` Incomplete Rejection Matrix

**Severity**: High
**Likelihood**: Med
**Impact**: A missed rejection lets a wrong-mode handler receive a param it did not design for. For SQL modes this risks a WHERE clause that ignores the param (silent data corruption — wrong result set). For path mode it risks unvalidated `category` reaching the BFS handler where it has no effect — misleading caller.

**Test Scenarios**:
1. Unit test matrix (minimum coverage): for each of the 8 new fields, test at least one wrong-mode rejection. Priority ordering:
   - `category` passed to `path` mode → error naming "inverse/filter"
   - `missing_edge_types` passed to `filter` mode → error naming "inverse"
   - `limit` passed to `chain` mode → error naming "inverse/filter"
   - `min_edge_count` passed to `inverse` mode → error naming "filter"
   - `max_edge_count` passed to `neighbors` mode → error naming "filter"
   - `min_age_days` passed to `path` mode → error naming "filter"
   - `min_confidence` passed to `subgraph` mode → error naming "filter"
   - `max_confidence` passed to `current` mode → error naming "filter"
2. Unit test: `from_id` passed to `filter` mode → error naming "path" (pre-existing stub now actively rejected per spec).
3. Unit test: unrecognized mode name → error lists all seven mode names: "chain, current, neighbors, subgraph, inverse, filter, path" (AC-26).

**Coverage Requirement**: SR-08 requires at least one wrong-mode rejection test per new field (8 minimum). Rejection matrix in ARCHITECTURE.md is the authoritative reference.

---

### R-05: `inverse` Mode AND Semantics — Unexpected Narrow Results

**Severity**: Med
**Likelihood**: High
**Impact**: An agent calling `inverse(missing_edge_types=["Cites","Supports"])` expecting entries missing EITHER type gets back only entries missing BOTH. The result set is correct per spec but the mismatch between caller intent and semantics produces spurious "no uncited sources" conclusions. ADR-003 records that this is the intended design, but the tool description must make it unmissable.

**Test Scenarios**:
1. Integration test (AC-28): write 4 source entries — (a) no Cites, no Supports; (b) has Cites, no Supports; (c) no Cites, has Supports; (d) has both. Call `inverse(category="source", missing_edge_types=["Cites","Supports"])`. Assert ONLY entry (a) is returned. Entries (b) and (c) — which are missing one type — must NOT be returned.
2. Integration test: call same query with `missing_edge_types=["Cites"]` only. Assert entries (a) AND (c) are returned (both lack Cites). Validates that single-type inverse works correctly and OR behavior is achievable via two separate calls.
3. Code review: tool description contains AND semantics example ("entries missing ALL listed types") per ADR-003.

**Coverage Requirement**: The 4-state data fixture (entry × 4 combinations of Cites/Supports presence) is mandatory — a 2-state fixture cannot distinguish AND from OR semantics.

---

### R-06: `resolve_supersessions=true` Response Reflects Original Instead of Resolved ID

**Severity**: Med
**Likelihood**: Med
**Impact**: If `PathResponse.from_id` echoes the deprecated input ID rather than the resolved successor ID, callers using the response's `from_id` to reconstruct the path start from the wrong (now-deprecated) node. ADR-006 requires the resolved ID to be reflected in the response.

**Test Scenarios**:
1. Integration test (AC-20): write deprecated entry D (superseded by active entry A). Call `path(from_id=D, to_id=B, resolve_supersessions=true)`. Assert `response.from_id == A` (resolved ID), NOT D (original). Assert the path BFS started from A (hops reflect edges from A, not D).
2. Integration test (AC-21): same setup, `resolve_supersessions=false`. Assert `response.from_id == D` (original). Verify BFS proceeds from D — if D has no edges in the graph, result is `found: false`.
3. Unit test: `to_id` resolution — write deprecated destination D2 superseded by T. Call `path(from_id=X, to_id=D2, resolve_supersessions=true)`. Assert `response.to_id == T`.
4. Edge case: when `follow_to_current` returns `None` (orphaned deprecated terminal — 50-hop cap), verify fallback uses original ID and does not panic.

**Coverage Requirement**: Both `from_id` and `to_id` resolution must be tested independently (AC-20 covers from_id; add explicit to_id test). The `None`-fallback path must be exercised.

---

### R-07: `depth` Rejection Behavior Change Breaks Existing Callers

**Severity**: Med
**Likelihood**: Med
**Impact**: Any existing caller that passes `depth` to `chain`, `current`, `subgraph`, `inverse`, or `filter` modes currently receives silently-ignored behavior (no error). After vnc-020, those calls return a validation error. This is intentional (ADR-004) but is a breaking change for anyone relying on the silent-ignore path.

**Test Scenarios**:
1. Unit test (AC-25): for each of the 5 affected modes — `chain`, `current`, `subgraph`, `inverse`, `filter` — call `context_graph(mode=X, depth=3)` and assert a validation error is returned with message "depth is not supported in X mode — use neighbors or path mode".
2. Regression test: `neighbors` mode still accepts `depth` (no change — assert no error when `depth=3` is passed to neighbors).
3. Regression test: `path` mode accepts `depth=3` with no error.
4. Unit test: `depth=0` passed to `path` mode → validation error (range [1,10]).
5. Unit test: `depth=11` passed to `path` mode → validation error.

**Coverage Requirement**: All 5 newly-rejecting modes must each have an explicit rejection test. This corrects a pre-existing silent-ignore behavior and must be enumerated (AC-25).

---

### R-08: `filter` Mode with Both `min_edge_count` and `max_edge_count` — Two Correlated Subqueries Required

**Severity**: Med
**Likelihood**: Med
**Impact**: ARCHITECTURE.md §filter specifies two independent correlated subqueries when both bounds are present (one for `>=`, one for `<=`). An implementation using a single subquery with `BETWEEN`-style AND-bounds is functionally equivalent for most cases but risks subtle miscounting if `edge_types` filtering differs between the two evaluations. More critically, a single subquery with `BETWEEN` syntax is more fragile to extend.

**Test Scenarios**:
1. Integration test: write entries with 0, 1, 2, 3, 4 outgoing edges. Call `filter(min_edge_count=2, max_edge_count=3, edge_types=["Advances"])`. Assert exactly entries with 2 and 3 edges are returned; 0, 1, and 4 are absent.
2. Unit test: SQL generated when both `min_edge_count` and `max_edge_count` are present contains TWO separate `(SELECT COUNT(*) ...)` subquery expressions, not one.
3. Unit test: `min_edge_count=2`, no `max_edge_count` → single `>= 2` subquery present; no `<= ?` clause present.
4. Unit test: `max_edge_count=0`, no `min_edge_count` → single `<= 0` subquery present.

**Coverage Requirement**: The combined-bounds case (both present) must be tested as a data correctness test, not just SQL shape inspection. Boundary values (exactly at min, exactly at max, one outside each) required.

---

### R-09: `path` No-Path vs. Not-In-Snapshot — Same Wire Shape, Different Internal Paths

**Severity**: Med
**Likelihood**: Med
**Impact**: Both "no path found within depth" and "from_id not in current snapshot" return `{ found: false, hops: [], length: 0 }`. If only one code path is tested (typically the no-path case), a bug in the snapshot-absence path (e.g., wrong error converted to empty result via the warn+continue anti-pattern — lesson #4473) goes undetected at gate.

**Test Scenarios**:
1. Unit test (AC-15): call `handle_path` with a `from_id` not present in the TypedRelationGraph (bypassing the tick via injection helper per pattern #4501). Assert return is `PathResponse { found: false }` — NOT an `ErrorData` return. Inspect handler signature to confirm it returns `Result<PathResponse, ErrorData>` not an infallible `PathResponse` (lesson #4497).
2. Integration test (AC-14): write two entries with NO edge between them. Call path mode. Assert `{ found: false, hops: [], length: 0 }`.
3. Unit test: `to_id` absent from snapshot (from_id present). Assert `found: false` not an error.
4. Verify that both AC-14 and AC-15 scenarios are covered by distinct test fixtures — not a single test that happens to also hit the snapshot-absence path incidentally.

**Coverage Requirement**: AC-14 and AC-15 require separate test fixtures. Handler must use `Result<PathResponse, ErrorData>` signature (not infallible) per pattern #4497.

---

### R-10: `inverse` Mode Includes Deprecated Entries Without `status = 0` Guard

**Severity**: Med
**Likelihood**: Med
**Impact**: If the `AND e.status = 0` WHERE clause is omitted during dynamic antijoin SQL construction (particularly easy to miss in the dynamic N-JOIN builder), deprecated entries with no incoming edges of the specified type appear in the result set. Agents acting on "orphaned sources" would act on already-deprecated entries.

**Test Scenarios**:
1. Integration test (AC-27, extended): write one active source with no Cites edge AND one deprecated source with no Cites edge. Call `inverse(category="source", missing_edge_types=["Cites"])`. Assert only the active entry is returned; the deprecated entry is absent.
2. Unit test: SQL string generated by `handle_inverse` always contains `AND e.status = 0` regardless of the number of LEFT JOINs (test with 1, 2, and 3 missing_edge_types).
3. Integration test: same scenario for `filter` mode — deprecated entries are excluded (`status = 0` in outer WHERE clause).

**Coverage Requirement**: The `status = 0` guard must be verified explicitly; it must not be treated as covered by AC-27's base scenario which may only use active entries.

---

### R-11: `filter` Mode Category-Only Query (No Filters) Is Untested

**Severity**: Low
**Likelihood**: Med
**Impact**: A `filter` call with `category` only and no property or edge-count filters is valid (FR-07: "a valid category-only filter"). If untested, an implementation that requires at least one optional filter to avoid returning `errors` or an empty query is silently broken for this use case.

**Test Scenarios**:
1. Unit test: call `handle_filter(category="goal")` with no other params. Assert no validation error, SQL executes, response contains all active goal entries up to limit=100.
2. Integration test: write 3 goal entries. Call `filter(category="goal")` with no other params. Assert all 3 are returned with `total_returned: 3`.

**Coverage Requirement**: One integration test with the minimal valid `filter` invocation.

---

### R-12: Path Response `length` Diverges from `hops.len()`

**Severity**: Low
**Likelihood**: Med
**Impact**: If `from_id` is accidentally included in the hops array during BFS path reconstruction, `length` matches `hops.len()` but the full node sequence count is inflated by 1. Agents reconstructing the path as `[from_id] + hops.map(h => h.entry_id)` would have a duplicate start node.

**Test Scenarios**:
1. Integration test (AC-13, AC-31): for a known 2-hop path A→B→C, assert `hops.len() == 2`, `hops[0].entry_id == B`, `hops[1].entry_id == C`, `from_id == A` (NOT in hops), `length == 2`.
2. Unit test: 1-hop path (A→B direct edge): assert `hops.len() == 1`, `from_id == A`, `hops[0].entry_id == B`, `length == 1`.
3. Unit test: `from_id == to_id` (same node): assert `found: false` and NOT `{ found: true, hops: [], length: 0 }` — a self-path is not a valid traversal result per spec (path mode finds paths between two distinct nodes).

**Coverage Requirement**: Path response shape must be verified for 1-hop, 2-hop, and zero-hop (no-path) cases.

---

### R-13: `limit` Out-of-Range Boundary Handling

**Severity**: Low
**Likelihood**: Low
**Impact**: `limit=0` or `limit=501` could cause a panic (division by zero, integer overflow in LIMIT clause), silent truncation, or a missing validation error. The range [1, 500] must be enforced for both `inverse` and `filter` modes identically.

**Test Scenarios**:
1. Unit test (AC-05): `inverse` mode with `limit=0` → validation error stating allowed range.
2. Unit test (AC-05): `inverse` mode with `limit=501` → validation error.
3. Unit test (AC-11): `filter` mode with `limit=0` → validation error.
4. Unit test: omit `limit` in both modes → default 100 applied (assert SQL contains `LIMIT 100`).

**Coverage Requirement**: Boundary tests for both owning modes. Default behavior verified by SQL inspection or result count.

---

### R-14: `RelationType::from_str` Wildcard Arm Ordering

**Severity**: Low
**Likelihood**: Low
**Impact**: The SCOPE.md and vnc-018 ADR-007 both require the wildcard arm to remain LAST in `from_str`. If a future vnc-020 change or merge conflict moves the wildcard arm earlier, valid type names like `"Advances"` fail with "unrecognized edge type" errors in `inverse`, `filter`, and `path` modes.

**Test Scenarios**:
1. Unit test: call `RelationType::from_str` with each of the 16 known variant names. Assert all 16 parse successfully (no false-negative "unrecognized" errors).
2. Unit test: call with `"NotAType"` → assert error returned by the wildcard arm.

**Coverage Requirement**: A smoke test covering all 16 variants prevents silent breakage from future struct changes. This is a cheap regression guard.

---

## Integration Risks

### IR-01: SQL Dynamic Builder Correctness Under N LEFT JOINs (inverse mode)

`handle_inverse` constructs SQL dynamically based on `missing_edge_types.len()`. The alias progression (`g1`, `g2`, ... or `?` placeholder alignment) must be verified to produce valid SQL for N=1, N=2, and N=3+ types. A parameter index offset error (e.g., binding `category` to the wrong `?` position) silently returns wrong results without a SQL error.

**Scenario**: Integration tests for AC-27 (single type) and AC-28 (two types) cover N=1 and N=2. Add a unit test for N=3 missing_edge_types to verify SQL validity — even if N=3 is not a documented use case, it exercises the dynamic builder loop's termination condition.

### IR-02: `follow_to_current` Async Calls Inside BFS Loop (path mode)

Path BFS is async due to per-hop `follow_to_current` calls (ADR-006). The graph is cloned before BFS begins (lock released), but `follow_to_current` hits the Store for each deprecated neighbor. Under a graph with many deprecated nodes and `resolve_supersessions=true`, the number of Store reads is bounded by `avg_degree × depth × deprecated_fraction`. At worst-case `depth=10` and 100% deprecated fraction, this is ~50 Store reads per path query. This is acceptable but must not block the BFS frontier processing.

**Scenario**: Unit test with a graph containing 5 consecutive deprecated intermediaries (all superseded by one active terminal) — verify BFS completes within the depth budget and the Store read count does not exceed `2 + N_deprecated_hops`.

### IR-03: `graph_read.rs` 500-Line Budget

`graph_read.rs` is projected to reach approximately 500 lines after adding 8 new `GraphParams` fields, 3 new response envelopes, 3 dispatch arms, and validation expansion. If the 500-line limit is exceeded, the delivery gate will reject the file and require an immediate refactor. This causes rework per lesson #1203 (Gate Validators Must Check All Files in One Pass).

**Scenario**: Code review gate must count lines in `graph_read.rs` and confirm it is ≤ 500. Handler logic must be in sibling modules only. Validation expansion in `validate_no_unsupported_params` is the most likely cause of overflow.

### IR-04: `filter` Mode SQL Correlated Subquery — `edge_types` IN Clause Binding

When `edge_types` contains multiple values (e.g., `["Advances", "Supports"]`), the correlated subquery uses `relation_type IN (?, ?)`. The number of `?` placeholders must match `edge_types.len()`. sqlx `push_bind` (pattern #4058) is the correct approach — a string-interpolated IN clause is an injection surface and produces wrong parameter counts.

**Scenario**: Integration test: call `filter(category="goal", max_edge_count=0, edge_types=["Advances","Supports"])` with entries having mixed outgoing edge types. Verify entries with 0 outgoing edges of EITHER type are excluded correctly, and entries with only `RelatedTo` edges (not in edge_types) are treated as having 0 edges of the specified types.

---

## Edge Cases

| Edge Case | Mode | Risk |
|-----------|------|------|
| `from_id == to_id` in path mode | path | Whether spec returns `found: false` or `found: true` with `hops: []` must be explicit — a zero-hop "path to self" is an ambiguous result |
| `missing_edge_types` with duplicate entries (e.g., `["Cites","Cites"]`) | inverse | Generates two LEFT JOINs on the same type — both null checks always produce the same result. Should deduplicate or validate uniqueness. |
| `min_confidence > max_confidence` in filter mode | filter | Silently returns 0 results (empty set) — not a validation error per spec, but callers should be warned in tool description |
| BFS depth=1 in path mode where from_id and to_id are 2 hops apart | path | Returns `found: false` — not an error. Tests for AC-14 should include this boundary |
| `inverse` mode with 10+ `missing_edge_types` | inverse | Dynamic SQL builder generates 10+ LEFT JOINs — verify SQLite handles this without hitting compile-time join limits |
| `filter` mode with `min_age_days=0` | filter | Logically means "any age" — should return all entries of the category, not zero entries. |
| `path` mode with `depth=10` in a densely connected graph | path | Worst-case BFS: 5^10 = ~10M frontier operations. Test with a graph of 100 nodes, high connectivity at depth=10 to confirm sub-second return |

---

## Security Risks

### SR-A: Filter Mode SQL Construction — Parameterization Completeness

`handle_filter` dynamically constructs SQL WHERE clauses from caller-supplied typed fields. Each non-null field adds one clause fragment and one bound parameter. The risk is that a refactoring or copy-paste error introduces a string interpolation instead of a `?` binding for any field — converting a safe typed value into an injection surface.

**Untrusted inputs**: All `GraphParams` fields deserialized from MCP JSON. No field should appear as a string fragment in SQL; all must be bound via sqlx parameters.

**Blast radius**: If any filter field is interpolated, a crafted `min_confidence` or `min_age_days` value could alter the WHERE clause or extract data from arbitrary tables via UNION SELECT.

**Mitigation scenarios**:
1. Code review gate: grep `graph_read_filter.rs` for any format string or string concatenation that includes a caller-supplied field value. Zero matches required.
2. Fuzz-style unit test: pass `min_age_days = u32::MAX` and `min_confidence = f64::INFINITY` — assert no SQL error and no panic. These are extreme typed values that must bind cleanly.

### SR-B: `inverse` Mode Dynamic SQL Alias Construction

`handle_inverse` generates JOIN aliases (`g1`, `g2`, ...) dynamically. If alias names are constructed from caller input rather than from the loop counter, an adversarial `missing_edge_types` string could inject SQL via the alias position.

**Untrusted inputs**: `missing_edge_types` values are validated via `RelationType::from_str` before SQL construction. The type names themselves are never interpolated — only `?` placeholders are used for the relation type values. Aliases are loop-counter-derived (`g1`, `g2`...), not user-supplied.

**Blast radius**: Low if `RelationType::from_str` validates correctly. The injection surface is effectively closed by the enum validation step.

**Mitigation scenario**: Unit test: pass `missing_edge_types=["Cites'; DROP TABLE entries; --"]` — assert validation error is returned, no SQL is executed.

### SR-C: `path` Mode — Cycle Handling in BFS

The in-memory `TypedRelationGraph` can contain cycles (non-tree graphs). The BFS visited set must prevent infinite loops. `is_cyclic_directed` is already used in vnc-018 for cycle detection in other contexts, but path mode BFS must independently protect against re-visiting nodes.

**Blast radius**: Without a visited set, a cyclic graph causes unbounded BFS until `depth` is exhausted — correct behavior, but a graph with many short cycles could cause the frontier to balloon.

**Mitigation scenario**: Unit test: construct a cyclic graph (A→B→C→A). Call `path(from_id=A, to_id=D)` where D is unreachable. Assert BFS terminates at `depth` hops with `found: false` and does not loop.

---

## Failure Modes

| Failure | Mode | Expected Behavior |
|---------|------|------------------|
| `from_id` not in graph snapshot | path | `{ found: false, hops: [], length: 0 }` — not an error (AC-15) |
| `to_id` not in graph snapshot | path | Same as above — not an error |
| No path within `depth` hops | path | `{ found: false, ... }` — not an error (AC-14) |
| `follow_to_current` returns `None` (orphaned deprecated chain) | path | Fall back to original ID — not an error (ADR-006 fallback) |
| `category` not found in DB (no matching entries) | inverse, filter | Empty `entries: []`, `total_returned: 0` — not an error |
| `limit` out of range | inverse, filter | Validation error with exact range statement |
| Unrecognized `edge_types` value | all three modes | Validation error naming the unrecognized value and listing all 16 types |
| TypedRelationGraph RwLock poisoned | path | `unwrap_or_else(|e| e.into_inner())` fallback — consistent with subgraph/neighbors pattern; must not panic |
| DB read pool unavailable | inverse, filter | sqlx error propagated as `ErrorData` — not a panic |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (two freshness contracts on one tool) | R-01 (staleness disclosure correctness) | Exact tool description text mandated in ARCHITECTURE.md §Staleness Disclosure; `inverse`/`filter` descriptions explicitly prohibit staleness language. |
| SR-02 (filter dynamic SQL double-count risk) | R-08 (two independent subqueries required) | Architecture specifies two independent correlated subqueries for combined bounds. AC-29/AC-30 integration tests validate count correctness. |
| SR-03 (module split boundary decisions) | IR-03 (graph_read.rs 500-line budget) | ADR-001 mandates three sibling modules; `validate_no_unsupported_params` stays in `graph_read.rs` as single cross-mode rejection point. |
| SR-04 (depth rejection is a behavior change) | R-07 (existing callers broken) | ADR-004 documents intentional behavior change; AC-25 requires per-mode rejection tests for all 5 newly-rejecting modes. |
| SR-05 (resolve_supersessions intermediate resolution — new vs. reused?) | R-03 (double-enqueue via raw visited key) | SR-05 is RESOLVED: `follow_to_current` is reused infrastructure. R-03 covers the correct implementation requirement (pattern #4494: key visited set on resolved ID). |
| SR-06 (AND semantics non-obvious default) | R-05 (unexpected narrow results) | ADR-003 mandates AND semantics example in tool description; AC-28 requires 4-state data fixture to distinguish AND from OR. |
| SR-07 (vnc-019 delivery sequencing) | — | Accepted — no architecture risk. Delivery blocked on PR #597; no test coverage needed. |
| SR-08 (combinatorial rejection surface) | R-04 (incomplete rejection matrix) | ARCHITECTURE.md rejection matrix is authoritative; tester must cover at least one wrong-mode rejection per new field (8 minimum). |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-04) | Staleness disclosure inspection; max_edge_count=0 boundary integration; visited-set double-enqueue unit; rejection matrix 8-field coverage |
| High | 6 (R-05 through R-10) | AND semantics 4-state fixture; endpoint resolution reflected in response; depth rejection 5-mode sweep; combined edge-count bounds; no-path vs. not-in-snapshot distinct fixtures; deprecated-entry exclusion |
| Medium | 2 (R-11, R-12) | Category-only filter; path response shape for 1-hop and 2-hop |
| Low | 2 (R-13, R-14) | Limit boundary validation; RelationType::from_str 16-variant smoke |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found #4473 (warn+continue masks failure-path tests), #2758 (grep non-negotiable test names before accepting PASS claims)
- Queried: `/uni-knowledge-search` for "risk pattern SQL dynamic query injection" — found #4058 (push_bind pattern for dynamic SQL), #3346 (sole-write-gate allowlist exhaustive match)
- Queried: `/uni-knowledge-search` for "BFS graph traversal staleness tick" — found #4494 (visited set keyed on resolved ID — directly informs R-03), #4493 (staleness disclosure ADR), #4501 (TypedGraphState injection for BFS unit tests)
- Queried: `/uni-knowledge-search` for "validate_no_unsupported_params cross-mode rejection" — found #4497 (infallible handler signatures mask validation — directly informs R-09)
- Stored: nothing novel to store — R-03 (visited-set keyed on resolved ID) is already entry #4494; R-09 (infallible handler signatures) is already entry #4497. Both patterns are now confirmed as cross-feature recurring risks, but their entries already exist and are correctly scoped.
