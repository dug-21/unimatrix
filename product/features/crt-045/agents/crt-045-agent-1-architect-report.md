# crt-045 Architect Report

**Agent:** crt-045-agent-1-architect

## Artifacts Produced

- `/workspaces/unimatrix/product/features/crt-045/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/crt-045/architecture/ADR-001-post-construction-write-vs-parameter.md`
- `/workspaces/unimatrix/product/features/crt-045/architecture/ADR-002-rebuild-error-handling.md`
- `/workspaces/unimatrix/product/features/crt-045/architecture/ADR-003-test-live-search-not-just-handle-state.md`
- `/workspaces/unimatrix/product/features/crt-045/architecture/ADR-004-typed-graph-handle-accessor-visibility.md`
- `/workspaces/unimatrix/product/features/crt-045/architecture/ADR-005-toml-distribution-change-false.md`

## ADRs Stored in Unimatrix

| File | Unimatrix ID | Title |
|------|-------------|-------|
| ADR-001 | #4098 | Post-Construction Write-Back Rather Than Pre-Populated Handle Parameter |
| ADR-002 | #4099 | Degraded Mode on Rebuild Failure, Not Abort |
| ADR-003 | #4100 | Integration Test Must Invoke Live Search, Not Only Assert Handle State |
| ADR-004 | #4101 | typed_graph_handle() Accessor on EvalServiceLayer is pub(crate) |
| ADR-005 | #4102 | ppr-expander-enabled.toml Sets distribution_change=false |

## SR Risk Resolution

**SR-01 (High) — RESOLVED:** `services/mod.rs:399` creates `TypedGraphState::new_handle()`.
Line 419 passes `Arc::clone(&typed_graph_state)` to `SearchService::new()`. `ServiceLayer`
retains the original in `self.typed_graph_state`. `ServiceLayer::typed_graph_handle()` returns
`Arc::clone(&self.typed_graph_state)`. All three Arcs share the same backing allocation.
Post-construction write propagates. Option B (write-after-construction) is safe. No parameter
signature change required.

**SR-05 (High) — ADDRESSED:** ADR-003 mandates a three-layer test assertion: handle state,
graph connectivity via `find_terminal_active`, and a live `search()` call returning `Ok(_)`.
This guards against the wired-but-unused anti-pattern (Unimatrix #1495).

**SR-06 (Med) — ADDRESSED:** ADR-003 specifies that the test snapshot must contain at least
two Active entries with one graph edge between them, inserted via raw SQL into `graph_edges`
following the pattern in `test_reverse_coaccess_high_id_to_low_id_ppr_regression`.

## Key Design Decisions

1. **Post-construction write is the approach** (ADR-001). Rebuild happens between Step 5 and
   Step 13; write-back is Step 13b immediately after `with_rate_config()` returns. Zero
   signature changes elsewhere.

2. **Degraded mode, not abort** (ADR-002). On any rebuild error: `tracing::warn!`, leave
   `use_fallback = true`, return `Ok(layer)`. On success: `tracing::info!` with entry count.

3. **New `pub(crate)` accessor on `EvalServiceLayer`** (ADR-004):
   `fn typed_graph_handle(&self) -> TypedGraphStateHandle { self.inner.typed_graph_handle() }`
   No new field; pure delegation.

4. **Three-layer integration test** (ADR-003): handle state + graph connectivity +
   live `search()` call. Seeding pattern from existing `typed_graph.rs` tests.

5. **TOML fix: `distribution_change = false`** (ADR-005) with `mrr_floor = 0.2651` and
   `p_at_5_min = 0.1083`. Comment in TOML explains the intentional choice.

## Open Questions

None. All SCOPE.md open questions resolved (OQ-01, OQ-02, OQ-03/OQ-04).
