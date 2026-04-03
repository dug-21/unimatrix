# Test Plan Overview: crt-045
# Eval Harness — Wire TypedGraphState Rebuild into EvalServiceLayer

## Summary

crt-045 is a single-file fix in `eval/profile/layer.rs` with two supporting changes
(`ppr-expander-enabled.toml` and `layer_tests.rs`). The fix calls
`TypedGraphState::rebuild()` inside `EvalServiceLayer::from_profile()` and writes the
result into the existing shared `Arc<RwLock<TypedGraphState>>` handle. The primary test
risk is the **wired-but-unused anti-pattern** (ADR-003): the handle can hold a rebuilt
state that SearchService never observes if the Arc clone chain is broken or the
`if !use_fallback` guard is bypassed.

---

## Test Layers

| Layer | Scope | Command |
|-------|-------|---------|
| Unit | `TypedGraphState` handle mechanics (already exist in `typed_graph.rs`) | `cargo test --workspace 2>&1 \| tail -30` |
| Integration (in-process) | `EvalServiceLayer::from_profile()` against a real seeded SqlxStore | `cargo test -p unimatrix-server eval::profile 2>&1 \| tail -30` |
| Integration harness (infra-001) | MCP-level smoke — no crt-045-specific behavior visible at MCP layer | `pytest -m smoke` |

---

## Risk-to-Test Mapping

| Risk ID | Severity | Priority | Test Component | Test Name(s) | Notes |
|---------|----------|----------|---------------|-------------|-------|
| R-01 | High | Med | layer_tests.rs | `test_from_profile_typed_graph_rebuilt_after_construction` (layer 1+3) | Write propagation confirmed by live search returning Ok(_) |
| R-02 | High | Med | layer_tests.rs | `test_from_profile_typed_graph_rebuilt_after_construction` (layer 2+3) | Three-layer assertion: handle state + `find_terminal_active` + `search()` |
| R-03 | High | **High** | layer_tests.rs | `test_from_profile_typed_graph_rebuilt_after_construction` (seeding) | Fixture must use Active entries + S1/S2/S8 edge (C-09) |
| R-04 | High | Med | layer_tests.rs | `test_from_profile_returns_ok_on_cycle_error` (new) | Cycle-producing Supersedes set → `Ok(layer)` + `use_fallback==true` |
| R-05 | Med | Med | ppr-expander-enabled-toml.md | unit test in `eval/profile/tests.rs` | `parse_profile_toml()` returns `Ok(profile)` with `distribution_change==false` |
| R-06 | Med | Low | layer_tests.rs | All existing tests pass unchanged | Regression: no behavioral change for non-graph profiles |
| R-07 | Med | Low | (residual) | None — accepted risk; sqlx timeout is implicit guard | |
| R-08 | Med | Low | EvalServiceLayer.md | Compiler check: accessor is `pub(crate)` | Enforced by Rust visibility rules; PR review gate |
| R-09 | Med | Med | (manual) | Pre-merge baseline run: `unimatrix eval run --profile baseline.toml` | Delivery agent must confirm MRR >= 0.2651 |
| R-10 | Low | Low | (residual) | Covered incidentally by AC-06 integration test | from_profile() is sequential async; no concurrent access |

---

## Non-Negotiable Test Scenarios (gate-blocking)

Per RISK-TEST-STRATEGY.md Coverage Summary — all four must be present before PASS:

1. `use_fallback == false` AND `typed_graph` non-empty after `from_profile()` with Active-entry + edge snapshot (R-03, AC-06)
2. Live `search()` call returns `Ok(_)` on graph-enabled layer (R-02, SR-05, ADR-003)
3. `Ok(layer)` returned on cycle-detected rebuild error with `use_fallback == true` (R-04, AC-05)
4. All existing `layer_tests.rs` and `eval/profile/tests.rs` tests pass unchanged (R-06, AC-08)

---

## Acceptance Criteria Coverage

| AC-ID | Test Method | Covered By |
|-------|-------------|-----------|
| AC-01 | Automated integration test | `test_from_profile_typed_graph_rebuilt_after_construction` — layer 1 assertion |
| AC-02 | Manual harness run | Pre-merge: `eval run --profile ppr-expander-enabled.toml` vs `baseline.toml` |
| AC-03 | Automated unit test | `parse_profile_toml()` unit test in `eval/profile/tests.rs` |
| AC-04 | Manual + automated regression | Existing tests pass; manual baseline run pre-merge |
| AC-05 | Automated integration test | `test_from_profile_returns_ok_on_cycle_error` |
| AC-06 | Automated integration test | `test_from_profile_typed_graph_rebuilt_after_construction` — all three layers |
| AC-07 | CI gate | `cargo test --workspace` exit 0 |
| AC-08 | Automated regression | All pre-existing `layer_tests.rs` and `eval/profile/tests.rs` tests pass |

---

## Cross-Component Test Dependencies

| Interaction | Tested By |
|-------------|-----------|
| `EvalServiceLayer::from_profile()` → `TypedGraphState::rebuild()` | `test_from_profile_typed_graph_rebuilt_after_construction` (AC-06 layer 1) |
| `ServiceLayer` Arc clone chain → post-construction write propagates to `SearchService` | `test_from_profile_typed_graph_rebuilt_after_construction` (AC-06 layer 3: live search) |
| `TypedGraphState::rebuild()` → `build_typed_relation_graph()` filters Quarantined | Pre-existing: `test_rebuild_excludes_quarantined_entries` in `typed_graph.rs` |
| `ppr-expander-enabled.toml` → `parse_profile_toml()` → `EvalProfile.config` | Unit test: `parse_profile_toml()` returns `Ok` with correct field values |
| `typed_graph_handle()` accessor → delegates to `self.inner.typed_graph_handle()` | `test_from_profile_typed_graph_rebuilt_after_construction` (uses accessor) |

---

## Integration Harness Plan (infra-001)

### Suite Selection

crt-045 modifies only the eval path (`eval/profile/layer.rs`) — no MCP tools, no server
protocol, no store schema, no confidence scoring, no security boundary changes. The fix is
entirely internal to `EvalServiceLayer::from_profile()`.

| Feature touches... | Applicable? | Suites |
|--------------------|-------------|--------|
| Any server tool logic | No | `tools`, `protocol` — not relevant |
| Store/retrieval behavior | No | `lifecycle`, `edge_cases` — not relevant |
| Confidence system | No | `confidence` — not relevant |
| Schema or storage changes | No | `volume` — not relevant |
| Any change at all | **Yes** | **`smoke` — mandatory minimum gate** |

**Conclusion:** Run `pytest -m smoke` as the minimum gate. No additional infra-001 suites
are relevant — crt-045's behavior is not observable through the MCP interface (eval path is
CLI-only, not MCP-tool exposed).

### Why No New Integration Tests Are Needed in infra-001

The critical behavioral assertions for crt-045 require:

1. Direct access to `EvalServiceLayer::typed_graph_handle()` (a `pub(crate)` accessor)
2. Raw SQL insertion into `graph_edges` via `store.write_pool_server()`
3. In-process `SearchService.search()` invocation on the constructed layer

None of these are accessible through the MCP JSON-RPC protocol. The correct test location
is `crates/unimatrix-server/src/eval/profile/layer_tests.rs` (in-process integration
test), not infra-001.

### Gap Analysis

| Gap | Assessment |
|-----|-----------|
| New MCP tool behavior | None — no new MCP tool added |
| New lifecycle flow through MCP | None — eval is CLI-only |
| New security boundary | None |
| New confidence behavior | None |
| Behavior only visible via MCP | None |

**No new infra-001 integration tests are needed for crt-045.**

### Smoke Gate Command

```bash
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60
```

Expected: all smoke tests pass. crt-045 changes are orthogonal to every smoke-tested
capability.

---

## Test Execution Order (Stage 3c)

1. `cargo test --workspace 2>&1 | tail -30` — unit test baseline
2. `cargo test -p unimatrix-server eval::profile 2>&1 | tail -30` — targeted eval profile integration tests
3. `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60` — infra-001 smoke gate
4. Manual pre-merge: `unimatrix eval run --profile baseline.toml` baseline confirmation (R-09)
