# Agent Report: dsn-001-agent-1-architect

## Status: COMPLETE

## Artifacts Produced

- `/workspaces/unimatrix/product/features/dsn-001/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/dsn-001/architecture/ADR-001-confidence-params-struct.md` — Unimatrix #2284
- `/workspaces/unimatrix/product/features/dsn-001/architecture/ADR-002-config-type-placement.md` — Unimatrix #2285
- `/workspaces/unimatrix/product/features/dsn-001/architecture/ADR-003-two-level-config-merge.md` — Unimatrix #2286
- `/workspaces/unimatrix/product/features/dsn-001/architecture/ADR-004-forward-compat-stubs.md` — Unimatrix #2287

## Key Decisions

1. **ADR-001**: `ConfidenceParams` struct in `unimatrix-engine` — absorbs `alpha0`,
   `beta0`, and `freshness_half_life_hours` into a single context struct.
   `compute_confidence(entry, now, &ConfidenceParams)` replaces four positional args.
   W3-1 extends the struct without further API churn.

2. **ADR-002**: `UnimatrixConfig` lives in `unimatrix-server/src/infra/config.rs`.
   No `Arc<UnimatrixConfig>` crosses any crate boundary. Values extracted as plain
   primitives: `bool`, `Vec<String>`, `Vec<Capability>`, `ConfidenceParams`.
   `CategoryAllowlist::new()` delegates to `from_categories(INITIAL_CATEGORIES)` —
   all existing tests remain valid unchanged.

3. **ADR-003**: Two-level merge uses replace semantics. Per-project field absent =
   falls through to global. Per-project field present = fully replaces. List fields
   replace not append. Merge runs in `load_config()` after path resolution.
   `dirs::home_dir()=None` degrades gracefully.

4. **ADR-004**: Empty `ConfidenceConfig` and `CycleConfig` stubs reserved in
   `UnimatrixConfig` for W3-1. Zero fields, zero behavior, TOML namespace reserved.

## ADR File Paths (for synthesizer)

- `product/features/dsn-001/architecture/ADR-001-confidence-params-struct.md`
- `product/features/dsn-001/architecture/ADR-002-config-type-placement.md`
- `product/features/dsn-001/architecture/ADR-003-two-level-config-merge.md`
- `product/features/dsn-001/architecture/ADR-004-forward-compat-stubs.md`

## Constraints for Delivery Team

1. `toml = "0.8"` added to `unimatrix-server/Cargo.toml` only. Run `cargo tree`
   after adding — pin as `toml = "0.8"` (not `^`) per SR-01.
2. `ConfidenceParams` migration: all `compute_confidence` call sites change from
   `(entry, now, alpha0, beta0)` to `(entry, now, &ConfidenceParams::default())`.
   Tests that override one field use struct update syntax.
3. `agent_resolve_or_enroll` gains third param `session_caps: Option<&[Capability]>`.
   All existing call sites pass `None`.
4. `context_retrospective` → `context_cycle_review` blast radius: Rust source,
   protocol files, skill files, research docs, CLAUDE.md. Build passing is
   necessary but not sufficient — audit all non-Rust files.
5. `ContentScanner::global()` must be called once at the top of `load_config` to
   force singleton initialization before `scan_title()` is invoked (SR-03).
6. File permission check is `#[cfg(unix)]` only.
7. `dirs::home_dir()=None` — warn and use defaults, do not abort.
