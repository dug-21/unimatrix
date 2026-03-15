# Test Plan: search.rs — crt-014

## Component

`crates/unimatrix-server/src/services/search.rs` — MODIFIED.

Changes under test:
1. Import line 18: `DEPRECATED_PENALTY, SUPERSEDED_PENALTY` removed; `graph_penalty`, `find_terminal_active`, `FALLBACK_PENALTY`, `GraphError` imported from `unimatrix_engine::graph`.
2. Step 6a (penalty marking): `penalty_map.insert(entry.id, graph_penalty(...))` replaces two constant insertions.
3. Step 6b (successor injection): `find_terminal_active(...)` replaces `entry.superseded_by` single-hop lookup.
4. T-SP test migration: 8 existing tests that reference `DEPRECATED_PENALTY` or `SUPERSEDED_PENALTY` must be updated.

All unit tests are in the inline `#[cfg(test)]` block at the bottom of `search.rs`. Integration tests go in `infra-001/suites/test_lifecycle.py`.

---

## AC-12: penalty_map Populated via graph_penalty (R-01)

### Grep Verification (AC-14 / AC-18)

After all changes, the following shell commands must return zero non-test hits:

```bash
grep -rn "DEPRECATED_PENALTY\|SUPERSEDED_PENALTY" crates/unimatrix-server/src/ --include="*.rs" \
  | grep -v "#\[cfg(test)\]" | grep -v "// "
```

Expected: no output.

```bash
grep -rn "DEPRECATED_PENALTY\|SUPERSEDED_PENALTY" crates/unimatrix-engine/src/ --include="*.rs" \
  | grep -v "#\[cfg(test)\]" | grep -v "// "
```

Expected: no output (constants removed from confidence.rs).

### Unit Test: Penalty Uses Graph Topology (AC-12)

A unit test within `search.rs` cannot invoke the live search pipeline (requires async runtime and store), but can verify the graph-derived penalty helpers used by Step 6a. Tests for the `graph_penalty` function itself are in `graph.rs`. The `search.rs` test block should assert the integration at the helper call site:

```rust
#[test]
fn penalty_map_uses_graph_penalty_not_constant() {
    // Verify that for a superseded entry, graph_penalty returns a topology-derived value.
    // With a clean-replacement chain (depth 1), the penalty is CLEAN_REPLACEMENT_PENALTY (0.40),
    // which differs from both old SUPERSEDED_PENALTY (0.5) and DEPRECATED_PENALTY (0.7).
    use unimatrix_engine::graph::{build_supersession_graph, graph_penalty, CLEAN_REPLACEMENT_PENALTY};
    let entries = vec![
        make_test_entry(1, Status::Active, None, 0.65, "decision"),
        make_test_entry(2, Status::Active, Some(1), 0.65, "decision"),
    ];
    // Note: make_test_entry second arg is superseded_by; adjust to match actual signature.
    let graph = build_supersession_graph(&entries).expect("valid DAG");
    let penalty = graph_penalty(1, &graph, &entries);
    // Entry 1 is superseded by 2 (depth-1 clean replacement)
    assert!(
        (penalty - CLEAN_REPLACEMENT_PENALTY).abs() < 1e-10,
        "depth-1 superseded entry must receive CLEAN_REPLACEMENT_PENALTY (0.40), got {penalty}"
    );
    // Confirm it differs from old constant values
    assert!(
        (penalty - 0.5_f64).abs() > 0.05,
        "penalty must not equal old SUPERSEDED_PENALTY (0.5)"
    );
    assert!(
        (penalty - 0.7_f64).abs() > 0.05,
        "penalty must not equal old DEPRECATED_PENALTY (0.7)"
    );
}
```

---

## AC-13: Multi-Hop Injection (R-06)

**Location**: `infra-001/suites/test_lifecycle.py`

### `test_search_multihop_injects_terminal_active`

This integration test verifies that Step 6b follows the full chain A→B→C and injects C (the terminal active node), not B (the single-hop successor).

Setup:
1. Store entry A with content "alpha knowledge v1".
2. Correct A → creates B (B supersedes A, A gains `superseded_by=B.id`).
3. Correct B → creates C (C supersedes B, B gains `superseded_by=C.id`; C is Active, `superseded_by=None`).
4. Search for "alpha knowledge".

Expected behavior:
- A appears in candidates (semantic match).
- Injected successor is C (not B).
- B may or may not appear, but if it does it is penalized.
- C.id is in the returned result IDs.

```python
def test_search_multihop_injects_terminal_active(server):
    # Step 1: Store A
    resp_a = server.call("context_store", {
        "content": "alpha knowledge v1",
        "category": "decision",
        "agent_id": "test-agent",
    })
    id_a = resp_a["id"]

    # Step 2: Correct A → B
    resp_b = server.call("context_correct", {
        "original_id": id_a,
        "content": "alpha knowledge v2",
        "reason": "Updated",
        "agent_id": "test-agent",
    })
    id_b = resp_b["id"]

    # Step 3: Correct B → C
    resp_c = server.call("context_correct", {
        "original_id": id_b,
        "content": "alpha knowledge v3 (current)",
        "reason": "Further updated",
        "agent_id": "test-agent",
    })
    id_c = resp_c["id"]

    # Step 4: Search
    resp = server.call("context_search", {
        "query": "alpha knowledge",
        "k": 10,
        "agent_id": "test-agent",
    })
    result_ids = [r["id"] for r in resp["results"]]

    # C (the terminal active node) must be injected
    assert id_c in result_ids, f"Terminal active C (id={id_c}) must appear in results"
    # B must NOT be injected as the successor (multi-hop must follow to C)
    # B may appear as a penalized candidate — the key assertion is C's presence
    # Optional: verify C appears before A and B in ranking
```

Fixture: `server` (fresh DB — no state leakage).

### Regression: Single-Hop Still Works

```python
def test_search_single_hop_injection_regression(server):
    # Store A, correct A → B only (no further chain)
    resp_a = server.call("context_store", {...})
    id_a = resp_a["id"]
    resp_b = server.call("context_correct", {"original_id": id_a, ...})
    id_b = resp_b["id"]

    resp = server.call("context_search", {"query": "...", "k": 10, ...})
    result_ids = [r["id"] for r in resp["results"]]

    # B must be injected (single-hop chain → terminal is B)
    assert id_b in result_ids, "Single-hop terminal B must be injected"
```

---

## AC-14: Constant Absence Verification (R-11)

Shell commands (run in Stage 3c before reporting):

```bash
# No references to removed constants in production code
grep -rn "DEPRECATED_PENALTY\|SUPERSEDED_PENALTY" \
    /workspaces/unimatrix-crt-014/crates/ --include="*.rs"
# Expected: zero non-test references (only test lines allowed if any)
```

This is a grep-based verification, not a test function. Run during Stage 3c and record count in RISK-COVERAGE-REPORT.md.

---

## AC-16: Cycle Fallback (R-08)

**Location**: `search.rs` unit test block (not infra-001 — cycle data cannot be injected through MCP).

### `fn cycle_fallback_uses_fallback_penalty`

Constructs a cycle via raw `EntryRecord` values and verifies that the fallback penalty path is invoked. This test cannot run the live search pipeline; it tests the graph construction + fallback detection helper in isolation.

```rust
#[test]
fn cycle_fallback_uses_fallback_penalty() {
    use unimatrix_engine::graph::{build_supersession_graph, FALLBACK_PENALTY, GraphError};

    // Two entries creating a cycle
    let entries = vec![
        make_test_entry(1, Status::Active, /* superseded_by= */ Some(2), 0.65, "decision"),
        make_test_entry(2, Status::Active, /* superseded_by= */ Some(1), 0.65, "decision"),
    ];
    // Note: must set supersedes field, not superseded_by — check make_test_entry signature
    // The cycle is: entry 1 has supersedes=Some(2), entry 2 has supersedes=Some(1)
    let result = build_supersession_graph(&entries);
    assert!(
        matches!(result, Err(GraphError::CycleDetected)),
        "cycle must be detected"
    );

    // When CycleDetected, use_fallback=true → FALLBACK_PENALTY applied to deprecated/superseded entries
    // We verify the constant value is what search.rs will apply:
    assert!(
        (FALLBACK_PENALTY - 0.70_f64).abs() < f64::EPSILON,
        "FALLBACK_PENALTY must be 0.70"
    );
    // And that it is strictly in (0.0, 1.0) — valid penalty range
    assert!(FALLBACK_PENALTY > 0.0 && FALLBACK_PENALTY < 1.0);
}
```

### `fn cycle_fallback_does_not_penalize_active_entries` (R-08)

Documents the expected runtime behavior: only entries where `superseded_by.is_some() || status == Deprecated` receive `FALLBACK_PENALTY`. Active non-superseded entries get no entry in `penalty_map`.

This is a code-review check (R-08). The condition guard in search.rs Step 6a must read:

```rust
if entry.superseded_by.is_some() || entry.status == Status::Deprecated {
    let penalty = if use_fallback { FALLBACK_PENALTY } else { ... };
    penalty_map.insert(entry.id, penalty);
}
```

Verify during Stage 3c code review: no unconditional `penalty_map.insert()` call.

---

## T-SP Test Migration

The 8 existing T-SP tests (lines 540–678 and 814–869) reference `DEPRECATED_PENALTY` or `SUPERSEDED_PENALTY`. After constant removal from `confidence.rs`, these tests will fail to compile unless updated. Migration strategy for each:

### T-SP-01: `deprecated_below_active_flexible` (line 541)

Current: calls `penalized_score(..., DEPRECATED_PENALTY)`.

Migration: Replace the constant with the `graph_penalty` value for an orphan deprecated entry (ORPHAN_PENALTY = 0.75) or a topology-appropriate value. Since the test proves behavioral ordering (active ranks above deprecated), keep the behavioral assertion but supply a topology-derived penalty:

```rust
// After migration: use ORPHAN_PENALTY for a deprecated entry with no successors
let deprecated_penalty = ORPHAN_PENALTY; // 0.75 — softest topology penalty
let deprecated_score = penalized_score(deprecated_sim, deprecated.confidence, deprecated_penalty);
assert!(active_score > deprecated_score, "...");
```

The assertion still proves active > deprecated regardless of the exact constant value.

### T-SP-02: `superseded_below_active_flexible` (line 562)

Current: calls `penalized_score(..., SUPERSEDED_PENALTY)`.

Migration: Replace with `CLEAN_REPLACEMENT_PENALTY` (0.40) — the penalty for a depth-1 clean replacement supersession (the canonical single-hop case). The ordering assertion (active > superseded) still holds since 0.40 is harsher than 0.70.

```rust
let superseded_penalty = CLEAN_REPLACEMENT_PENALTY; // 0.40
```

### T-SP-03: `strict_mode_excludes_non_active` (line 580)

No constant references. No migration required. Keep as-is.

### T-SP-04: `superseded_penalty_harsher` (line 607)

Current: `assert!(SUPERSEDED_PENALTY < DEPRECATED_PENALTY, ...)`.

Migration: This test validates ordering between penalty values. After migration, the equivalent behavioral ordering is that a clean-replacement-superseded entry is penalized more harshly than an orphan deprecated entry:

```rust
#[test]
fn superseded_harsher_than_orphan_deprecated() {
    assert!(
        CLEAN_REPLACEMENT_PENALTY < ORPHAN_PENALTY,
        "clean replacement ({CLEAN_REPLACEMENT_PENALTY}) must be harsher than orphan deprecated ({ORPHAN_PENALTY})"
    );
}
```

This maps the intent of T-SP-04 to the new penalty ordering semantics.

### T-SP-05: `deprecated_only_results_visible_flexible` (line 615)

Current: calls `penalized_score(..., DEPRECATED_PENALTY)`.

Migration: Replace with `ORPHAN_PENALTY`. The test intent is "deprecated entry still visible in flexible mode" — any penalty < 1.0 satisfies this:

```rust
let score = penalized_score(deprecated_sim, deprecated.confidence, ORPHAN_PENALTY);
assert!(score > 0.0, "deprecated entry must have positive score");
```

### T-SP-06: `successor_ranks_above_superseded` (line 631)

Current: calls `penalized_score(..., SUPERSEDED_PENALTY)`.

Migration: Replace with `CLEAN_REPLACEMENT_PENALTY`. The ordering assertion (successor > superseded) holds with any penalty < 1.0:

```rust
let superseded_score = penalized_score(superseded_sim, superseded.confidence, CLEAN_REPLACEMENT_PENALTY);
```

### T-SP-07: `penalty_independent_of_confidence_formula` (line 651)

Current: `base * DEPRECATED_PENALTY` and `assert!((penalized - base * DEPRECATED_PENALTY).abs() < f64::EPSILON)`.

Migration: Replace with any topology-derived constant. The test intent is "penalty is applied multiplicatively to the rerank score, not to confidence." Use `ORPHAN_PENALTY`:

```rust
let penalized = base * ORPHAN_PENALTY;
assert_eq!(base, rerank_score(sim, conf, 0.18375));
assert!(penalized < base);
assert!((penalized - base * ORPHAN_PENALTY).abs() < f64::EPSILON);
```

### T-SP-08: `equal_similarity_penalty_determines_rank` (line 667)

Current: asserts `active_score > deprecated_score > superseded_score` using old constants.

Migration: Update to use topology-derived ordering. The three-tier ordering now is: active (1.0) > orphan deprecated (0.75) > clean-replacement superseded (0.40):

```rust
let active_score    = penalized_score(sim, conf, 1.0);
let deprecated_score = penalized_score(sim, conf, ORPHAN_PENALTY);         // 0.75
let superseded_score = penalized_score(sim, conf, CLEAN_REPLACEMENT_PENALTY); // 0.40

assert!(active_score > deprecated_score,
    "active must rank above orphan deprecated");
assert!(deprecated_score > superseded_score,
    "orphan deprecated must rank above clean-replacement superseded");
```

Note: The ordering now reflects topology semantics: an orphan deprecated entry (0.75) is actually softer than a clean-replacement superseded entry (0.40). Wait — this is inverted from the old ordering where superseded (0.5) was harsher than deprecated (0.7). The new behavior is intentionally different: an orphan deprecated entry with no known successor is softer (less penalized) than a superseded entry with a known better replacement, and a superseded entry at depth 1 is harshest among common cases. This behavioral change is intentional per ADR-004 and should be documented in the test.

### crt-018b Interaction Tests (lines 814–869)

Tests `test_utility_delta_inside_deprecated_penalty` and `test_utility_delta_inside_superseded_penalty` use `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` as representative penalty values to validate the ADR-003 formula placement for crt-018b. After constant removal, these tests must be updated to use topology-derived constants from `graph.rs`:

- `test_utility_delta_inside_deprecated_penalty` (line 817): Replace `DEPRECATED_PENALTY` with `ORPHAN_PENALTY` (0.75) — still a deprecated-entry scenario.
- `test_utility_delta_inside_superseded_penalty` (line 848): Replace `SUPERSEDED_PENALTY` with `CLEAN_REPLACEMENT_PENALTY` (0.40) — still a superseded-entry scenario.

The formula verification (correct vs wrong placement) remains identical; only the constant values change. The numerical comments in the test body must be updated to reflect the new values.

---

## IR-01: QueryFilter::default() Includes All Statuses

### Code Review Check

Verify in `search.rs` Step 6 pre-processing that `Store::query(QueryFilter::default())` is called (not `QueryFilter { status: Some(Active), .. }`). The full-store read must include Deprecated entries for graph construction.

### Unit Check

In the graph unit tests (graph.md), the test `fn all_statuses_included_in_graph` verifies:

```rust
// Build graph with both Active and Deprecated entries
entries = [make_entry(id=1, status=Active, ...), make_entry(id=2, status=Deprecated, ...)]
graph = build_supersession_graph(&entries).unwrap()
assert!(graph.node_index.contains_key(&1))
assert!(graph.node_index.contains_key(&2), "Deprecated entry must be included in graph")
```

---

## IR-02: Unified Guard Condition

### `fn unified_penalty_guard_covers_superseded_active_entry` (IR-02)

Entry with `superseded_by.is_some()` but `status == Active` (superseded-but-marked-active) must receive a graph penalty.

```rust
#[test]
fn unified_penalty_guard_covers_superseded_active_entry() {
    // This entry is Active status but has superseded_by set (unusual but valid)
    let entry = make_test_entry(1, Status::Active, Some(99), 0.65, "decision");
    // Verify the guard condition: superseded_by.is_some() || status == Deprecated
    let should_penalize = entry.superseded_by.is_some() || entry.status == Status::Deprecated;
    assert!(should_penalize, "entry with superseded_by set must be penalized regardless of status field");
}
```

---

## IR-03: No Unnecessary graph_penalty Calls for Active Entries

### Code Review Check

Confirm the Step 6a guard in search.rs:

```rust
if entry.superseded_by.is_some() || entry.status == Status::Deprecated {
    // penalty logic here
}
// No else branch that also calls graph_penalty
```

Active entries with `superseded_by.is_none()` must have no entry in `penalty_map` after Step 6a. Verified by code review.
