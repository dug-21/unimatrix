# crt-037 Test Plan Overview

## Test Strategy

crt-037 is a three-crate change: a pure enum extension in `unimatrix-engine`, three new
config fields in `unimatrix-server`, and one new store query in `unimatrix-store`. The
detection logic in `nli_detection_tick.rs` carries the highest risk — it adds Phase 4b and
Phase 8b to an existing multi-phase tick pipeline and introduces `NliCandidatePair` as a
tagged union routing discriminator.

### Test Layers

| Layer | Scope | Tooling |
|-------|-------|---------|
| Unit tests | Pure functions, guard predicates, config parsing, PPR math | `cargo test --workspace` |
| Store integration | `query_existing_informs_pairs` against real SQLite | `cargo test --workspace` (sqlx test pool) |
| Tick integration | Phase 4b/8b detection path end-to-end | `cargo test --workspace` (existing tick test fixtures) |
| CI grep gates | Rayon async contamination, domain string leakage | Shell in CI (AC-21, AC-22) |
| infra-001 smoke | MCP protocol + tool dispatch regression | `pytest -m smoke` |
| infra-001 tools suite | All 12 tool parameter paths | `pytest suites/test_tools.py` |
| infra-001 lifecycle suite | Multi-tick flows, restart persistence | `pytest suites/test_lifecycle.py` |

### Priority Order

1. R-20 (critical, process): All 11 tick integration tests (AC-13–AC-23) must ship with
   Phase 4b/8b code in the same wave. Gate hard-stop if missing.
2. R-02 (critical): PPR direction — AC-05 must assert `scores[lesson_node_id]`, not
   aggregate non-zero.
3. R-03 (critical): Five independent guard predicate negative tests — one per predicate,
   not bundled.
4. R-01 (critical): DDL inspection pre-gate + runtime write confirmation.
5. R-04 (high): Cross-route contamination — both directions tested.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Primary Tests | Component |
|---------|----------|---------------|-----------|
| R-01 | Critical | DDL inspection (pre-gate); store write + readback integration test | read.rs |
| R-02 | Critical | AC-05 unit (specific lesson node score); `positive_out_degree_weight` unit; Direction::Incoming negative | graph_ppr.md |
| R-03 | Critical | AC-14 temporal (equal + reversed); AC-15 same-cycle; AC-16 wrong-category; AC-17 cosine floor; neutral=0.5 boundary | nli_detection_tick.md |
| R-04 | High | Phase 8 only: SupportsContradict written, no Informs; Phase 8b only: Informs written, no Supports | nli_detection_tick.md |
| R-05 | High | AC-13 weight value check (`cosine * ppr_weight`); feature-cycle propagation data-flow test | nli_detection_tick.md |
| R-06 | High | Cap sequencing: full cap → zero Informs accepted; partial cap → Informs fills remainder | nli_detection_tick.md |
| R-07 | High | neutral=0.5000001 written; neutral=0.5 rejected; FR-11 entailment exclusion | nli_detection_tick.md |
| R-08 | High | AC-22 CI grep gate; category non-match unit test; empty pairs config test | nli_detection_tick.md / config.md |
| R-09 | High | Directional dedup: `(100,200)` present, `(200,100)` absent; bootstrap_only=1 excluded | read.md |
| R-10 | High | AC-24: Informs-only graph, graph_penalty = FALLBACK_PENALTY; find_terminal_active empty | graph.md |
| R-11 | Medium | Cap invariant: `merged.len() <= max_cap` across input size combinations; cap=0 no panic | nli_detection_tick.md |
| R-12 | Medium | Log field assertions: accepted, total, dropped in all three configurations | nli_detection_tick.md |
| R-13 | Medium | OQ-S3: in-memory category map vs DB read; latency regression with 500 entries | nli_detection_tick.md |
| R-14 | Medium | AC-21 CI grep gate (Handle::current, .await in rayon closure) | nli_detection_tick.md |
| R-15 | Low | Finite weight assertion: `cosine * ppr_weight` is finite before write | nli_detection_tick.md |
| R-16 | High | Existing Supports/Contradicts test suite passes unchanged; Supports-only tick count matches baseline | nli_detection_tick.md |
| R-17 | Medium | AC-23: two-tick run, exactly one Informs row; pre-filter loaded on second tick | read.md / nli_detection_tick.md |
| R-18 | Low | AC-10 cosine floor bounds (0.0 → Err, 1.0 → Err, 0.45 → Ok); AC-11 ppr_weight bounds | config.md |
| R-19 | Medium | FR-11 Gap-2: entailment > threshold AND neutral > 0.5 → only Supports edge written | nli_detection_tick.md |
| R-20 | Critical | Delivery process gate: all AC-13–AC-23 present and passing in same wave as code | nli_detection_tick.md |

---

## Cross-Component Dependencies

| Dependency | Boundary | Test Coverage |
|------------|----------|---------------|
| `RelationType::Informs.as_str()` == `"Informs"` | engine → server | AC-03/AC-04 (graph.md) |
| `query_existing_informs_pairs` returns directional `(source, target)` tuples | store → server Phase 2 | R-09 unit tests (read.md) |
| `NliCandidatePair::Informs { candidate, nli_scores }` fields survive Phase 4b→8b | server internal | AC-20 weight assertion (nli_detection_tick.md) |
| `informs_category_pairs` passed as runtime value into detection | config → server | AC-22 grep gate + R-08 unit test |
| Fourth `edges_of_type(_, RelationType::Informs, Direction::Outgoing)` in PPR | engine internal | AC-05/AC-06 (graph_ppr.md) |

---

## Integration Harness Plan

### Which Existing Suites Apply

crt-037 does not add a new MCP tool and does not change any tool's parameter schema or
response format. The feature's observable MCP-visible effects are:

- PPR traversal now includes `Informs` edges in score computation
- Background tick writes `Informs` rows to `GRAPH_EDGES`
- `context_status` may reflect new edge counts if it queries `GRAPH_EDGES`

The following suites provide regression coverage:

| Suite | Rationale |
|-------|-----------|
| `smoke` | Mandatory minimum gate — ensures protocol and dispatch survived the refactor |
| `tools` | Exercises all 12 tool call paths; catches any graph_ppr or tick dispatch regression via `context_search` and `context_briefing` |
| `lifecycle` | Multi-tick flows that exercise graph edge accumulation and `context_briefing` PPR output |
| `confidence` | PPR is part of the re-ranking pipeline; adding an edge type could shift scores |

Suites `security`, `contradiction`, `volume`, `edge_cases`, `adaptation` are not
directly relevant to this change — run smoke only for regression safety.

### What Existing Suites Do NOT Cover

The infra-001 harness tests the compiled binary through MCP. It cannot:

- Trigger `run_graph_inference_tick` deterministically (background, time-driven)
- Inject controlled `NliScores` values (model is live)
- Assert specific edge counts in `GRAPH_EDGES` (no DB-introspection tool exposed via MCP)
- Verify the tagged union routing in `NliCandidatePair`

All Phase 4b/8b correctness (AC-13–AC-23) must be covered by internal integration tests in
`crates/unimatrix-server/src/services/nli_detection_tick.rs` (or a dedicated test module),
not by infra-001.

### New Integration Tests Needed

No new tests need to be added to infra-001 suites. The detection path is not observable
through the MCP JSON-RPC interface at the level required by AC-13–AC-23. Those tests are
Rust integration tests that call `run_graph_inference_tick` directly with a controlled store,
mock NLI scorer, and assertable `GRAPH_EDGES` state.

If a future feature exposes an `Informs` edge query via MCP (e.g., `context_search` returning
graph neighbor type), that would warrant infra-001 additions. Not in scope for crt-037.

### infra-001 Run Command (Stage 3c)

```bash
# Smoke gate (mandatory)
cd /workspaces/unimatrix/product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60

# Tools + lifecycle + confidence suites
python -m pytest suites/test_tools.py suites/test_lifecycle.py suites/test_confidence.py -v --timeout=60
```

---

## CI Grep Gates (Non-Test Enforcement)

These shell checks must run in CI and be verified during Stage 3c:

```bash
# AC-21: no Tokio handle inside rayon closure
grep -n 'Handle::current' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty output

# AC-22: no domain strings in detection logic
grep -n '"lesson-learned"\|"decision"\|"pattern"\|"convention"' \
  crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty output

# R-02 guard: no Direction::Incoming in graph_ppr.rs
grep -n 'Direction::Incoming' crates/unimatrix-engine/src/graph_ppr.rs
# Expected: empty output
```

---

## Test File Locations

| Component | Test Location |
|-----------|---------------|
| graph.rs | `crates/unimatrix-engine/src/graph.rs` (inline `#[cfg(test)]`) |
| graph_ppr.rs | `crates/unimatrix-engine/src/graph_ppr.rs` (inline `#[cfg(test)]`) |
| config.rs | `crates/unimatrix-server/src/infra/config.rs` (inline `#[cfg(test)]`) |
| read.rs | `crates/unimatrix-store/src/read.rs` (inline `#[cfg(test)]` or `tests/` integration dir) |
| nli_detection_tick.rs | `crates/unimatrix-server/src/services/nli_detection_tick.rs` or `tests/nli_detection_tick_tests.rs` |

Extend existing test modules — do not create isolated scaffolding.
