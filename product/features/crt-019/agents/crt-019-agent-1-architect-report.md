# Agent Report: crt-019-agent-1-architect

## Task
Design the architecture for crt-019: Confidence Signal Activation.

## Status
COMPLETE

## Output Files

- `/workspaces/unimatrix/product/features/crt-019/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/crt-019/architecture/ADR-001-adaptive-blend-state-management.md`
- `/workspaces/unimatrix/product/features/crt-019/architecture/ADR-002-bayesian-prior-state-cold-start.md`
- `/workspaces/unimatrix/product/features/crt-019/architecture/ADR-003-base-score-trust-source-scope.md`
- `/workspaces/unimatrix/product/features/crt-019/architecture/ADR-004-context-lookup-doubled-access.md`

## Key Decisions Made

### ADR-001: Adaptive Blend State (SR-03)
Chose parameter-passing with server-side `Arc<RwLock<ConfidenceState>>`. Engine remains
stateless; `rerank_score` gains `confidence_weight: f64` parameter. 4 call sites in search.rs
require mechanical updates. Rejected `AtomicU64` (cannot atomically update 4 values) and
embedding state in the engine crate (breaks pure-function invariant).

### ADR-002: Bayesian Prior State and Cold-Start (SR-01, SR-02)
Threshold of ≥10 voted entries for empirical prior; cold-start `α₀=3.0, β₀=3.0` below that.
All four values `{α₀, β₀, observed_spread, confidence_weight}` updated atomically under one
`RwLock` write per tick. `compute_confidence` gains `alpha0, beta0` parameters. The
`record_usage_with_confidence` closure in usage.rs must become a capturing closure (not bare
function pointer) to carry the current α₀/β₀.

### ADR-003: base_score Scope (SR-04)
Differentiation applies to `Status::Active` only — `auto` trust_source active entries → 0.35,
all other active → 0.5. `Proposed` always returns 0.5, preserving T-REG-01 ordering
(`auto_extracted_new()` uses Proposed). Clean two-parameter signature change: 5 call sites.

### ADR-004: context_lookup Doubled Access (SR-05)
`UsageContext` gains `access_weight: u32` field. Dedup fires first; weight multiplies the
resulting allowed IDs before passing to `record_usage_with_confidence`. Dedup-blocked entries
get 0 increments (not 2). All existing construction sites add `access_weight: 1`.

### SR-07: Implicit helpful vote for context_get
Confirmed: implemented by setting `helpful: params.helpful.or(Some(true))` in the existing
`UsageContext` at the `context_get` handler call site. No second spawn_blocking task.

### SR-08: lock_conn in async context
Confirmed from usage.rs: all `record_usage_with_confidence` calls are inside `spawn_blocking`.
No direct `lock_conn()` in async context on the usage path. 500-entry batch is safe.

## Implementation Ordering Requirement (SR-06)

The implementation agent MUST update T-REG-02 weight constant assertions FIRST, before
changing the weight constants in confidence.rs. This avoids a confusing state where code
compiles but calibration tests fail in non-obvious ways.

Recommended order:
1. Update T-REG-02 to new constant values (causes test to fail until Step 2)
2. Update weight constants in confidence.rs (T-REG-02 now passes)
3. Add `ConfidenceState` + `ConfidenceStateHandle` in services/confidence.rs
4. Thread `ConfidenceStateHandle` through ServiceLayer to SearchService and StatusService
5. Update `rerank_score` signature and all 4 call sites in search.rs
6. Update `base_score` signature and all 5 call sites
7. Replace `helpfulness_score` (Bayesian) and update `compute_confidence` signature
8. Update confidence refresh loop in status.rs (duration guard + prior computation)
9. Update usage.rs `compute_confidence` closure to capture α₀/β₀
10. Inject implicit helpful in context_get; add access_weight to context_lookup
11. Update all tests; add new calibration scenario; verify T-REG-01 ordering holds
12. Update skill files (documentation only)

## Critical Implementation Note: compute_confidence Closure

`services/usage.rs` line 158 currently passes:
```rust
Some(&crate::confidence::compute_confidence)
```
as a bare function pointer. With the new signature
`compute_confidence(entry, now, alpha0, beta0)`, this cannot remain a bare function pointer.

The implementation agent must either:
- Change `record_usage_with_confidence` to accept `Box<dyn Fn(&EntryRecord, u64) -> f64 + Send>`
  and build a closure at the call site that captures the snapshotted α₀/β₀, or
- Add a separate step after `record_usage_with_confidence` that calls `update_confidence` with
  the pre-computed values.

This affects the store's API surface in `crates/unimatrix-store/` — verify the signature of
`record_usage_with_confidence` before implementing.

## Unimatrix Storage

MCP server was not accessible in this session. ADRs must be stored via `/uni-store-adr` skill
in a subsequent session before marking this feature complete in Unimatrix.
