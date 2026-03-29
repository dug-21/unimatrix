# crt-030 Test Plan Overview — Personalized PageRank

## Test Strategy

Three tiers of testing apply to this feature:

1. **Unit tests** — inline in `graph_ppr.rs` (or `graph_ppr_tests.rs` if >500 lines). Cover the
   pure PPR function in isolation: all edge type cases, direction semantics, boundary values,
   degenerate inputs, and the determinism constraint. No async, no store, no config.

2. **Component tests** — inline in `search.rs` test module (`search_tests.rs`). Cover the Step 6d
   integration block within the search pipeline: personalization vector construction, blending,
   pool expansion, quarantine enforcement, fallback bypass, and config field wiring. Use
   mock/stub `TypedRelationGraph` and mock `entry_store` where needed.

3. **Integration tests** — infra-001 harness via the MCP JSON-RPC protocol. Verify end-to-end
   behavior: a quarantined PPR neighbor does not appear in results (R-08), a graph-reachable
   entry surfaces after PPR expansion (AC-17), and PPR-surfaced entries participate in co-access
   boost (AC-11 step ordering).

---

## Risk-to-Test Mapping

| Risk ID | Priority | Component | Test File(s) | Scenarios |
|---------|----------|-----------|--------------|-----------|
| R-01 | Deferred | N/A | N/A | Rayon offload branch not in scope; zero scenarios |
| R-02 | High | search.rs | search_step_6d.md | use_fallback bit-for-bit identity; zero allocation |
| R-03 | High | search.rs | search_step_6d.md | ppr_blend_weight=0.0 → PPR-only sim=0.0; blend leaves HNSW unchanged |
| R-04 | High | graph_ppr.rs | graph_ppr.md | Sort-outside-loop code review gate; sort length == node_index.len(); timing at 10K |
| R-05 | High | search.rs | search_step_6d.md | All fetches fail → pool unchanged; quarantine fetch → silently skipped |
| R-06 | High | search.rs, config | search_step_6d.md, config_ppr_fields.md | Threshold == boundary → not included; threshold+epsilon → included |
| R-07 | Med | graph_ppr.rs | graph_ppr.md | Zero out-degree node receives no propagation; single seed normalization; all scores finite |
| R-08 | Critical | search.rs + integration | search_step_6d.md, infra-001/test_lifecycle.py | Quarantined entry from PPR fetch → not appended; active entry → appended; E2E integration |
| R-09 | Med | graph_ppr.rs | graph_ppr.md | grep gate: no edges_directed in graph_ppr.rs; Supersedes/Contradicts edges excluded |
| R-10 | Med | search.rs | search_step_6d.md | Code review: no phase_affinity_score() call in Step 6d; non-uniform snapshot changes seeds |
| R-11 | Med | search.rs | search_step_6d.md | ppr_blend_weight=1.0 overwrites HNSW sim; PPR-only entry ranks above lower HNSW candidate |
| R-12 | Med | graph_ppr.rs | graph_ppr.md | Prerequisite edge A→B: seed B finds A; seed A does NOT propagate to B via Prerequisite |
| R-13 | Med | graph_ppr.rs | graph_ppr.md | Dense 50-node full-mesh CoAccess graph completes < 1 ms |

---

## AC-to-Test Mapping

| AC-ID | Component | Test Plan | Test Reference(s) |
|-------|-----------|-----------|-------------------|
| AC-01 | graph_ppr.rs | graph_ppr.md | grep verification + module re-export test |
| AC-02 | graph_ppr.rs | graph_ppr.md | grep gate (no edges_directed); R-09 behavioral test |
| AC-03 | graph_ppr.rs | graph_ppr.md | T-PPR-08: Supersedes/Contradicts yield zero mass |
| AC-04 | graph_ppr.rs | graph_ppr.md | Code review of doc-comment |
| AC-05 | graph_ppr.rs | graph_ppr.md | T-PPR-09: identical inputs → identical output; sort placement code review |
| AC-06 | search.rs | search_step_6d.md | Non-uniform snapshot test; None snapshot cold-start; all-zero guard |
| AC-07 | graph_ppr.rs | graph_ppr.md | T-PPR-05: zero positive out-degree → zero forward propagation |
| AC-08 | graph_ppr.rs | graph_ppr.md | T-PPR-03 (Supports), T-PPR-06 (CoAccess), Prerequisite direction test |
| AC-09 | config.rs | config_ppr_fields.md | TOML round-trip; Default values; SearchService field wiring |
| AC-10 | config.rs | config_ppr_fields.md | All 10 out-of-range rejection cases |
| AC-11 | search.rs | search_step_6d.md | Step comment order code review; T-PPR-IT-01 co-access boost only if PPR before 6c |
| AC-12 | search.rs | search_step_6d.md | use_fallback=true: pool identical before/after Step 6d |
| AC-13 | search.rs | search_step_6d.md | Threshold boundary (==, +epsilon); quarantine skip; error skip; sort+cap |
| AC-14 | search.rs | search_step_6d.md | PPR-only initial_sim == ppr_blend_weight * ppr_score |
| AC-15 | search.rs | search_step_6d.md | Blend formula: sim=0.8, ppr=0.4, w=0.15 → 0.74 |
| AC-16 | search.rs | search_step_6d.md | Non-uniform phase_snapshot produces different seed_scores from uniform baseline |
| AC-17 | search.rs | search_step_6d.md | Inline unit test: A→Supports→B, B is HNSW seed, A surfaces after Step 6d |
| AC-18 | graph_ppr.rs | graph_ppr.md | CoAccess N→S with S as seed: result[N] > 0.0 |

---

## Cross-Component Test Dependencies

1. **graph_ppr.rs → search.rs Step 6d**: The step 6d tests pass a real `TypedRelationGraph`
   (not mocked) to exercise the full PPR call path. These tests depend on correct PPR function
   behavior established by the graph_ppr unit tests.

2. **config.rs → search.rs Step 6d**: Step 6d reads config values. Config tests verify defaults
   and validation; step 6d tests assume valid config values and use explicit constants rather than
   re-testing config validation.

3. **search.rs Step 6d → infra-001 T-PPR-IT-01**: The integration test depends on the full
   server binary with all three components compiled and wired together.

---

## Integration Harness Plan (infra-001)

### Which Existing Suites Apply

| Suite | Rationale |
|-------|-----------|
| `smoke` | Mandatory minimum gate — verify existing search, quarantine, and store operations still work |
| `lifecycle` | PPR expansion is a new retrieval flow; `store→search` lifecycle tests cover the path PPR entries travel through |
| `tools` | `context_search` tool has new behavior; tool parameter tests exercise query paths that trigger Step 6d |
| `security` | R-08 (quarantine bypass) is a security-relevant risk; quarantine enforcement tests cover the infra-level control |

These suites have no tests targeting PPR-specific behavior (PPR is new), so they serve as
regression gates confirming the feature does not break existing behavior.

### Gap Analysis

PPR introduces new MCP-visible behavior that no existing suite tests:

1. A graph-reachable entry that is NOT in the HNSW results can appear in `context_search` output
   after crt-030. No existing test validates this.
2. A quarantined entry that is a PPR neighbor of a seed must not appear in results. The existing
   security suite tests HNSW quarantine filtering; it does not test PPR-path quarantine filtering.
3. PPR-surfaced entries participate in co-access boost (AC-11). No test validates that co-access
   boost uses PPR-expanded pool as its anchor set.

### New Integration Tests to Add

These tests belong in `product/test/infra-001/suites/test_lifecycle.py` unless otherwise noted.

#### T-PPR-IT-01 — PPR surfaces graph-reachable entry not in HNSW results (AC-17)

```python
@pytest.mark.smoke
def test_ppr_surfaces_graph_neighbor(server):
    """
    Store two entries: B (decision, query-similar content) and A (lesson-learned,
    content unrelated to query). Add a Supports edge A→B. Query for B's content.
    Assert A appears in context_search results (PPR expanded pool) even though A
    is not query-similar to the search text.
    """
```

Fixture: `server` (fresh DB, function scope).

Note: This test requires that graph edges can be written in the test harness. If the infra-001
harness does not support writing GRAPH_EDGES directly (no `context_store_edge` tool), this test
must be deferred or written as a unit test in `search_tests.rs` instead. See open questions
below.

#### T-PPR-IT-02 — Quarantined PPR neighbor excluded from results (R-08 integration scenario)

```python
def test_ppr_quarantined_neighbor_excluded(admin_server):
    """
    Store two entries: B (seed, query-similar) and A (quarantined, Supports A→B).
    Query for B's content. Assert A does NOT appear in results.
    """
```

Fixture: `admin_server` (needs quarantine capability).

Same caveat as T-PPR-IT-01 regarding GRAPH_EDGES write access.

#### T-PPR-IT-03 — use_fallback=true leaves results identical (R-02 integration regression)

This scenario can be validated through the existing `lifecycle` and `tools` suites if a test
calls `context_search` after forcing `use_fallback = true` on the server. However, `use_fallback`
is an internal state (set by the tick on Supersedes cycle detection), not directly controllable
via MCP. This scenario is therefore best covered by the unit test in `search_step_6d.md`
rather than a new integration test.

### Harness Infrastructure Note

The key gap for T-PPR-IT-01 and T-PPR-IT-02 is the absence of a harness-level mechanism to
write `GRAPH_EDGES` rows during tests. If the existing suites use a pre-populated DB with graph
edges, T-PPR-IT-01 can use that fixture. Otherwise:

- **If graph edges are NOT writable via MCP tools**: Add T-PPR-IT-01 and T-PPR-IT-02 as inline
  `#[tokio::test]` tests in `search_tests.rs`, building `TypedRelationGraph` directly. Do not
  file a GH Issue for the harness gap unless directed.
- **If graph edges ARE writable** (e.g., `context_store` with edge type): implement as full
  infra-001 tests as described above.

The Stage 3c tester must check whether `populated_server` fixture (50 pre-loaded entries) includes
any `GRAPH_EDGES` rows before deciding which path to take.

---

## Test Conventions

- All unit tests in `graph_ppr.rs`: `#[cfg(test)]` module, `#[test]` (sync, no async needed — pure function)
- All step 6d tests: `#[tokio::test]` where async store access is needed
- Naming pattern: `test_{function_or_concept}_{scenario}_{expected}`
- Determinism tests: use `assert_eq!` on full `HashMap` contents, not just length
- Floating-point comparisons: use `(a - b).abs() < 1e-9` tolerance for blend formula assertions;
  use exact `==` for boundary assertions where integer math applies
- Timing assertions: use `std::time::Instant::now()` + `elapsed().as_millis()` with a 2× safety
  margin over NFR budget

---

## Open Questions for Stage 3c Tester

1. Does the `populated_server` infra-001 fixture include any `GRAPH_EDGES` rows? If yes,
   T-PPR-IT-01 and T-PPR-IT-02 can be implemented as infra-001 tests. If no, implement as
   inline `search_tests.rs` unit tests.
2. Is there a `context_store_edge` MCP tool or equivalent that the harness can call to write
   graph edges at test time? Check `suites/test_tools.py` for edge-writing calls.
3. The R-04 timing test requires the release build (`cargo test --release` or a dedicated
   bench). Confirm whether the CI/CD pipeline runs timing tests in release mode.
