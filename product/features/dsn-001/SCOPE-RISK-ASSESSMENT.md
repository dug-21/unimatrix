# Scope Risk Assessment: dsn-001

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `toml = "0.8"` adds first TOML dependency; no precedent in workspace. Transitive deps (indexmap, toml_edit) may introduce version conflicts with existing crates. | Med | Low | Pin `toml = "0.8"` (not `^`) at workspace level from the start; run `cargo tree` post-add to surface conflicts before implementation begins. |
| SR-02 | `freshness_score()` and `compute_confidence()` in `unimatrix-engine` are pure functions with a stable signature — every call site in the engine's own tests passes the half-life implicitly via the compiled constant. Adding a `freshness_half_life_hours: f64` parameter is a cross-crate API break that touches 2 test files and ~15 call sites. | High | High | Architect should decide parameter threading strategy before touching engine code: function parameter vs. a `ConfidenceParams` context struct that absorbs future additions without further API churn. A struct is safer if W3-1 will eventually replace these values. |
| SR-03 | `ContentScanner::global()` is a startup singleton. If config load (which calls `scan_title()`) is inserted before the singleton is initialized, the call panics. Startup ordering in `tokio_main_daemon`/`tokio_main_stdio` is untested for this sequencing. | Med | Med | Architect must verify `ContentScanner::global()` is initialized before config load runs, or delay instructions validation to after scanner initialization. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | The PRODUCT-VISION W0-3 section includes `[confidence]` lambda weights and `[cycle]` label parameters that SCOPE.md explicitly excludes (marked non-goal). W3-1 depends on `[confidence] weights` as cold-start config (PRODUCT-VISION line 709). If the architect designs `UnimatrixConfig` without a `[confidence]` section stub, adding it later requires a config format break. | High | Med | Architect should forward-design the `UnimatrixConfig` struct with placeholder `[confidence]` and `[cycle]` sections (empty, unused by W0-3) so W3-1 can add fields without format breaks. The sections need not be parsed — just reserved. |
| SR-05 | `context_retrospective` → `context_cycle_review` rename spans Rust source, `unimatrix-observe` types, 2 protocol files, 1 skill file, research docs, and CLAUDE.md. A partial rename compiles but leaves protocol/skill callers broken at runtime. The rename is a completeness test, not a logic test. | High | High | Spec writer must produce an explicit exhaustive checklist of all files requiring update (not just crates). The architect should not gate this rename on build success alone — all non-Rust files must be audited. |
| SR-06 | The two-level config merge (global → per-project) requires a merge strategy for structured fields: does a per-project `categories` list *replace* or *extend* the global list? SCOPE.md says "per-project values shadow global values" but the semantics for list fields are ambiguous. | Med | Med | Spec writer must define merge semantics explicitly for every field type: scalar fields shadow (last wins), list fields replace (not append). Ambiguity here produces hard-to-debug operator behavior. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | `CategoryAllowlist::new()` is called in 15+ test sites in `categories.rs`. Introducing `new_from_config()` as the production path while keeping `new()` for tests creates a persistent two-constructor split. Tests that use `new()` will never exercise config-driven category logic. | Med | High | Architect should make `new()` call `new_from_config(&Default::default())` so a single code path is always exercised. Prevents divergence between test and production behavior. |
| SR-08 | `PERMISSIVE_AUTO_ENROLL` is in `unimatrix-server/infra/registry.rs` but `session_capabilities` is in `unimatrix-store/registry.rs`. Config plumbing must cross the crate boundary; `Arc<UnimatrixConfig>` must be threaded into `unimatrix-store`. The store crate currently has no dependency on server-layer config types. | Med | Med | Architect should define config types in a thin `unimatrix-config` crate or in `unimatrix-core` to avoid a circular dependency. Alternatively, pass capabilities as plain values, not as `Arc<UnimatrixConfig>`. |

## Assumptions

- **SCOPE.md §Constraints**: Assumes `dirs::home_dir()` is available and correct. `dirs::home_dir()` returns `None` in container environments with `HOME` unset. If `None`, config load must degrade gracefully (not panic) — this is not stated explicitly.
- **SCOPE.md §Config Security Model**: Assumes `ContentScanner::global()` is initialized prior to config load. This ordering is asserted in prose but not enforced by the type system — a refactor could silently break it.
- **SCOPE.md §Non-Goals**: Assumes lambda weight externalisation can be deferred to W3-1 without affecting W1 accuracy. PRODUCT-VISION §W3-1 explicitly states GNN cold-start uses config-defined weights; if W3-1 begins before dsn-001's `[confidence]` section exists, W3-1 must define its own config format — causing a potential conflict.

## Design Recommendations

- **SR-02 (Critical)**: Decide on `ConfidenceParams` struct vs. bare parameter before starting. Bare parameter is simpler today but guarantees another API churn at W3-1. A struct costs nothing now.
- **SR-04 (High)**: Reserve `[confidence]` and `[cycle]` TOML sections in `UnimatrixConfig` as empty structs with `Default` impls, even if not populated by W0-3. This is a 10-line hedge against a later format break.
- **SR-05 (High)**: Rename checklist must be comprehensive across all file types before the PR opens. Build passing is a necessary but insufficient gate.
