# Test Plan Overview: crt-029 — Background Graph Inference

GH Issue: #412

---

## Overall Test Strategy

crt-029 adds a recurring background tick (`run_graph_inference_tick`) that infers `Supports`
graph edges across the full active entry population. The implementation spans four components:
config validation, store helpers, the tick function + helpers, and the background call site.

Testing layers:
1. **Unit tests** — pure logic: threshold validation, candidate selection, cap enforcement,
   priority ordering, store query correctness. No live NLI model required (mock `NliScores`).
2. **Integration tests (infra-001)** — system-level: graph edges visible after tick runs, `nli_enabled`
   gate, EDGE_SOURCE_NLI constant on written edges, no `Contradicts` edges from tick path.
3. **Static grep gates** — compile-invisible risks that cannot be caught by tests alone (R-01,
   R-09, R-10, R-11, R-07).

Because the tick's runtime risks (R-09 rayon/tokio boundary, R-10 W1-2 violation) are
compile-invisible, the test strategy pairs unit tests with mandatory pre-merge grep checks and
an independent code review requirement. Unit tests alone are insufficient for R-09.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Component | Test Type | Test Location | AC Coverage |
|---------|----------|-----------|-----------|---------------|-------------|
| R-01 (by design) | High (residual verify) | nli_detection_tick.rs | grep gate | Pre-merge shell | AC-10a, AC-19† |
| R-02 | Critical | nli-detection-tick | Unit | nli_detection_tick.rs `#[cfg(test)]` | AC-06c |
| R-03 | High | inference-config | Unit | config.rs `#[cfg(test)]` | AC-02, AC-03 |
| R-04 | Medium | store-query-helpers | Unit | read.rs `#[cfg(test)]` | AC-15 |
| R-05 | Medium | nli-detection-tick | Integration | infra-001 lifecycle | AC-08 |
| R-06 | High | store-query-helpers | grep gate + integration | Pre-merge shell | (pre-existing conflict) |
| R-07 | High | inference-config | grep gate + compile | Pre-merge shell | AC-18† |
| R-08 | High | nli-detection-tick | Unit | nli_detection_tick.rs `#[cfg(test)]` | AC-11 |
| R-09 | Critical | nli-detection-tick | grep gate + code review | Pre-merge shell + independent reviewer | AC-R09 |
| R-10 | High | nli-detection-tick | grep gate | Pre-merge shell | AC-08 |
| R-11 | High | background-call-site | Compile + grep gate | Pre-merge shell | (compile gate) |
| R-12 | Medium | nli-detection-tick | Unit | nli_detection_tick.rs `#[cfg(test)]` | AC-07 |
| R-13 | Low | nli-detection-tick | Unit | nli_detection_tick.rs `#[cfg(test)]` | AC-16 |

### Critical Risk Notes

**R-09 is compile-invisible and test-invisible.** Unit tests run on the Tokio runtime and will
not reproduce the panic. The only detection methods are:
- `grep -n 'Handle::current\|\.await' crates/unimatrix-server/src/services/nli_detection_tick.rs`
  (any match inside the rayon closure body is gate-blocking)
- Independent code review of the `rayon_pool.spawn()` closure — the reviewer MUST NOT be the
  same agent or person who wrote the closure (C-14 requirement)

**R-01 is eliminated by design** (C-13). The tick has no `Contradicts` write path. Residual
verification is a grep gate, not a behavioral test.

---

## Component Test Plan Files

| File | Component | Risks Addressed |
|------|-----------|----------------|
| `inference-config.md` | `InferenceConfig` additions | R-03, R-07 |
| `store-query-helpers.md` | `query_entries_without_edges`, `query_existing_supports_pairs` | R-04, R-06 |
| `nli-detection-tick.md` | `run_graph_inference_tick`, `select_source_candidates`, `write_inferred_edges_with_cap` | R-01(residual), R-02, R-05, R-08, R-09(partial), R-12, R-13 |
| `background-call-site.md` | `background.rs` call site | R-11, AC-14 |

---

## Cross-Component Test Dependencies

1. **Config validation before tick**: AC-01 through AC-04b (config tests) are prerequisites
   for any tick test that passes `InferenceConfig` values. Use `InferenceConfig::default()`
   in tick unit tests to avoid dependency on validation logic.

2. **Store helpers before tick**: `query_entries_without_edges` and
   `query_existing_supports_pairs` are called in the tick's Phase 2. Their unit tests must
   pass independently. The tick tests mock these returns rather than calling live DB helpers.

3. **`pub(crate)` promotions before tick compilation**: `write_nli_edge`,
   `format_nli_metadata`, `current_timestamp_secs` must be promoted in `nli_detection.rs`
   before `nli_detection_tick.rs` can compile. The compile gate catches this (R-11).

4. **`pub mod nli_detection_tick` declaration in `mod.rs`**: missing declaration causes
   a build failure immediately. Delivery agent adds this in wave-1.

---

## Integration Harness Plan

### Suite Selection

crt-029 touches background tick logic (store + graph edge writes) but does NOT add or modify
any MCP tool. The feature is not directly observable through a single MCP call — its effects
accumulate over background tick cycles and are visible via `context_status` graph metrics and
GRAPH_EDGES content.

| Feature behaviour | Applicable suite | Rationale |
|-------------------|-----------------|-----------|
| Background tick fires, writes edges | `lifecycle` | Multi-step flow: store entries, wait for tick, verify graph |
| `nli_enabled` gate | `tools` | context_status reports, tools behaviour with NLI on/off |
| No `Contradicts` from tick | `lifecycle`, `tools` | Verify graph edge types after tick |
| EDGE_SOURCE_NLI on written edges | `lifecycle` | DB-level assertion after tick |
| Smoke baseline | `smoke` | Mandatory minimum gate |

**Suites to run in Stage 3c:**
- `smoke` — mandatory minimum gate (always)
- `lifecycle` — tick execution flows, edge accumulation
- `tools` — context_status graph metrics, nli_enabled flag behavior

**Suites NOT required:**
- `protocol` — no new MCP tools or protocol changes
- `security` — no new input surface
- `confidence` — no confidence system changes
- `contradiction` — tick does not write Contradicts; contradiction suite tests the dedicated path
- `volume` — tick scale behaviour is bounded by `max_graph_inference_per_tick`; not a new scale risk
- `edge_cases` — tick edge cases are covered by unit tests (empty graph, single entry, etc.)

### New Integration Tests Required

The existing lifecycle suite tests multi-step store→search→correct flows but does not exercise
the background tick's graph inference pass specifically. The following new integration tests
must be added to `suites/test_lifecycle.py`:

#### 1. `test_graph_inference_tick_writes_supports_edges`
**Fixture**: `server` (NLI enabled, fresh DB)
**Scenario**: Store two semantically related entries. Trigger a tick cycle. Query GRAPH_EDGES
(via context_status graph metrics or a lifecycle-level assertion). Assert at least one
`Supports` edge exists with `source = 'nli'` and `bootstrap_only = 0`.
**Traces**: AC-13, FR-07

#### 2. `test_graph_inference_tick_no_contradicts_edges`
**Fixture**: `server` (NLI enabled)
**Scenario**: After tick runs, assert zero rows in GRAPH_EDGES with `relation_type = 'Contradicts'`
that have `source = 'nli'` inserted by the tick path (verify via context_status contradiction
count remains unchanged from pre-tick baseline).
**Traces**: AC-10a, AC-19†, R-01

#### 3. `test_graph_inference_tick_nli_disabled`
**Fixture**: `server` (NLI disabled via config)
**Scenario**: With `nli_enabled = false`, run multiple tick cycles. Assert graph metrics
(`inferred_edge_count`) remain 0.
**Traces**: AC-14, FR-06

### Integration Tests NOT Required

The following behaviors are fully covered by existing suites or are not MCP-visible:
- Priority ordering of source candidates — pure logic, covered by unit tests
- Cap enforcement at `max_graph_inference_per_tick` — pure logic, unit tests
- `supports_candidate_threshold` / `supports_edge_threshold` config validation — unit tests
  cover validation; no MCP-level test needed (config is set at server startup)
- Rayon closure sync-only constraint — not testable through MCP interface (R-09 grep gate)
- `query_existing_supports_pairs` — unit tests; no additional MCP surface

---

## Mandatory Pre-Merge Grep Gates

These are not unit tests. They are shell checks performed before the PR is opened.

```bash
# R-01 / AC-10a / AC-19† — No Contradicts writes in tick module
grep -n 'Contradicts' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty

# R-09 / C-14 / AC-R09 — No tokio handle access inside rayon closure
grep -n 'Handle::current' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty
# ALSO: independent manual inspection of rayon_pool.spawn() closure for .await expressions

# R-10 / AC-08 — No spawn_blocking in tick module
grep -n 'spawn_blocking' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty

# R-07 / AC-18† — InferenceConfig struct literal coverage
grep -rn 'InferenceConfig {' crates/unimatrix-server/src/
# Expected: 52+ occurrences, each with new fields or ..InferenceConfig::default()

# R-11 — pub(crate) promotions present
grep -n 'pub(crate) fn write_nli_edge\|pub(crate) fn format_nli_metadata\|pub(crate) fn current_timestamp_secs' \
  crates/unimatrix-server/src/services/nli_detection.rs
# Expected: all three present

# NFR-05 / C-08 — File size limit
wc -l crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: <= 800

# C-12 / R-06 — compute_graph_cohesion_metrics pool choice
grep -n 'read_pool\|write_pool' crates/unimatrix-store/src/read.rs
# Expected: compute_graph_cohesion_metrics uses read_pool() (not write_pool_server())
```

---

## Test Count Targets

| Module | Unit Tests | Notes |
|--------|-----------|-------|
| `infra/config.rs` `#[cfg(test)]` | 10–14 | Defaults, TOML deser, validation: each boundary, cross-field |
| `services/nli_detection_tick.rs` `#[cfg(test)]` | 12–18 | No-op guard, cap, priority, idempotency, pre-filter, threshold |
| `unimatrix-store/src/read.rs` `#[cfg(test)]` | 8–10 | query_entries_without_edges (4), query_existing_supports_pairs (4) |
| `services/background.rs` `#[cfg(test)]` | 2–4 | Call site ordering, nli_enabled gate |
| infra-001 new tests | 3 | lifecycle suite additions (see above) |
