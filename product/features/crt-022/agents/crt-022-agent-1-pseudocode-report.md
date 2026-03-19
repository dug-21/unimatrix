# Agent Report: crt-022-agent-1-pseudocode

**Agent**: crt-022-agent-1-pseudocode (Pseudocode Specialist)
**Feature**: crt-022 — Rayon Thread Pool + Embedding Migration (W1-2)
**Session**: 2 Stage 3a

---

## Output Summary

All six pseudocode files created in `product/features/crt-022/pseudocode/`:

| File | Lines | Component |
|------|-------|-----------|
| `OVERVIEW.md` | ~120 | Component interactions, data flow, shared types, wave rationale |
| `rayon_pool.md` | ~170 | `RayonPool` + `RayonError` bridge algorithm |
| `inference_config.md` | ~180 | `InferenceConfig` + `validate()` + `ConfigError` variant |
| `call_site_migration.md` | ~230 | Migration pattern for all 7 sites, pool distribution wiring |
| `async_embed_removal.md` | ~130 | What to delete, what to keep, verification steps |
| `ci_enforcement.md` | ~180 | Grep step logic, allow-list strategy, CI integration |

---

## Components Covered

1. `RayonPool` + `RayonError` — new `crates/unimatrix-server/src/infra/rayon_pool.rs`
2. `InferenceConfig` — additions to `crates/unimatrix-server/src/infra/config.rs`
3. Call-site migration (7 sites) — `services/search.rs`, `services/store_ops.rs`,
   `services/store_correct.rs`, `services/status.rs`, `background.rs` (×2), `uds/listener.rs`
4. `AsyncEmbedService` removal — `crates/unimatrix-core/src/async_wrappers.rs`
5. CI grep enforcement — new CI step

---

## Architecture Compliance

- All interface names traced to `ARCHITECTURE.md` integration surface table or source files
- `RayonPool::spawn`, `spawn_with_timeout`, `pool_size`, `name` — from ARCHITECTURE.md
- `RayonError::Cancelled`, `RayonError::TimedOut(Duration)` — from ARCHITECTURE.md + ADR-002
- `InferenceConfig`, `UnimatrixConfig::inference` — from ARCHITECTURE.md + IMPLEMENTATION-BRIEF.md
- `ServiceLayer::new` signature extension documented from reading actual source in `services/mod.rs`
- `spawn_background_tick` and `start_uds_listener` parameter additions noted as required by
  ADR-004 (pool distribution); exact current signatures verified from `main.rs` source
- `MCP_HANDLER_TIMEOUT` confirmed as `Duration::from_secs(30)` from `infra/timeout.rs`

---

## Open Questions

### OQ-A: `ServerStartupError::InferencePoolInit` variant name

The IMPLEMENTATION-BRIEF.md and SPECIFICATION.md reference
`ServerStartupError::InferencePoolInit(rayon::ThreadPoolBuildError)` (NFR-03), but
the actual `ServerError` enum in `crates/unimatrix-server/src/error.rs` was not read
during this session. The implementer must:
1. Check whether `ServerError` already has a variant suitable for startup pool failures
2. If not, add `InferencePoolInit(rayon::ThreadPoolBuildError)` or an equivalent variant
3. The error message must include the configured `rayon_pool_size` value

If `ServerError` does not derive the right traits to wrap `rayon::ThreadPoolBuildError`
directly, an intermediate `String`-based wrapping is acceptable.

### OQ-B: `num_cpus` crate presence in `unimatrix-server`

The `Default` impl for `InferenceConfig` uses `num_cpus::get()`. The IMPLEMENTATION-BRIEF.md
lists `num_cpus` as "verify or add". The implementer must run `cargo tree -p unimatrix-server`
to confirm whether it is already a transitive dependency. If not, add `num_cpus = "1"` to
`unimatrix-server/Cargo.toml`.

### OQ-C: Exact location for `ServiceLayer` field and constructor parameter

The `ServiceLayer` struct in `services/mod.rs` currently does NOT have an
`ml_inference_pool` field. The pseudocode documents adding it, but the exact
placement in the struct (before or after `contradiction_cache`) should follow the
field declaration order that matches the instantiation order in `with_rate_config`.
No architectural constraint on ordering — implementer's discretion.

### OQ-D: CI workflow file location

Only `release.yml` was found in `.github/workflows/`. There is no separate PR-triggered
CI workflow. The implementer must decide:
- Add a new `ci.yml` workflow for PR checks, or
- Add the grep step to an existing workflow file

The script logic in `ci_enforcement.md` is correct regardless of the wrapper format.

---

## Deviation from Established Patterns

None detected. The `RayonPool` bridge follows the pattern documented in Unimatrix entry
#2491 (Rayon-Tokio bridge pattern for CPU-bound ML inference, tagged crt-022). The
pool distribution via `AppState` follows entry #316 (ServiceLayer extraction pattern).
The pool floor reasoning follows entry #2535 (shared rayon pool monopolisation by
long-running background scans).

The `InferenceConfig::validate` pattern exactly mirrors the other `validate_config`
subsection calls in `config.rs` (check category count, check half_life, check agents,
then check inference). No deviation.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "rayon thread pool async bridge tokio patterns" →
  found entry #2491 (Rayon-Tokio bridge pattern, crt-022 tagged) and #2535 (pool
  monopolisation, crt-022 tagged). Both directly informed rayon_pool.md algorithm and
  call_site_migration.md background task pattern.
- Queried: `/uni-query-patterns` for "crt-022 architectural decisions" (category: decision,
  topic: crt-022) → found ADR-001 through ADR-004 entries (#2536–#2539). All four ADRs
  confirmed in pseudocode.
- Deviations from established patterns: none. All design choices trace to ADRs or existing
  entries #2491, #2535.
- Stored: nothing — patterns are feature-specific and already stored by researcher/architect
  in entries #2491, #2535.
