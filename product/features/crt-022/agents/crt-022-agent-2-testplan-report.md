# Agent Report: crt-022-agent-2-testplan (Stage 3a — Test Plan Design)

## Output

All test plan files created under `product/features/crt-022/test-plan/`:

| File | Purpose |
|------|---------|
| `OVERVIEW.md` | Test strategy, risk-to-test mapping, integration harness plan |
| `rayon_pool.md` | Unit test specs for RayonPool + RayonError |
| `inference_config.md` | Unit test specs for InferenceConfig |
| `call_site_migration.md` | Grep/static verification for all 7 call sites |
| `async_embed_removal.md` | Removal verification for AsyncEmbedService |
| `ci_enforcement.md` | CI grep step verification |

## Risk Coverage

| Risk ID | Priority | Coverage Type | Plan File |
|---------|----------|---------------|-----------|
| R-01 | High | Unit: panic containment tests | rayon_pool.md §panic-containment |
| R-02 | Critical | Unit: timeout semantics, pool remains functional | rayon_pool.md §timeout-semantics |
| R-03 | High | Unit: mutex-poisoned closure → Cancelled at bridge | rayon_pool.md §panic-containment |
| R-04 | Critical | Grep: method audit + CI step verification | call_site_migration.md, ci_enforcement.md |
| R-05 | Med | Grep + cargo check | async_embed_removal.md |
| R-06 | High | CI grep step | ci_enforcement.md §spawn-blocking-grep |
| R-07 | Med | Unit: 6 boundary value tests + integration startup | inference_config.md |
| R-08 | High | Unit: pool exhaustion, queue behaviour | rayon_pool.md §concurrency |
| R-09 | Med | Grep: single construction site | call_site_migration.md §single-instantiation |
| R-10 | Low | Grep: embed_handle.rs guard | call_site_migration.md §embed-handle-guard |
| R-11 | Low | Cargo.toml inspection | ci_enforcement.md §cargo-toml-check |

## Integration Harness Plan

- Mandatory gate: `pytest -m smoke`
- Required suites: `tools`, `lifecycle`
- Suites not required: `protocol`, `volume`, `security`, `confidence`, `contradiction`, `edge_cases`, `adaptation`
- New integration test identified: `test_server_rejects_invalid_rayon_pool_size` — tests startup
  rejection when `rayon_pool_size = 0` via a custom fixture. Flagged as conditional: implement
  if the harness fixture cost is low; file GH Issue if it requires significant infrastructure changes.

## Key Observations

1. **InferenceConfig is not yet in UnimatrixConfig** — the existing `config.rs` has no
   `inference` field. The test plan for `inference_config.md` specifies tests for the field
   that the implementer must add. Stage 3b should follow the `#[serde(default)]` pattern
   used by all other config sections.

2. **ConfigError needs a new variant** — `InferenceConfig::validate()` needs a structured
   error variant. The test plan expects `ConfigError::InferencePoolSizeOutOfRange` (or similar)
   to be added to the `ConfigError` enum in `config.rs`. Stage 3b must add this variant.

3. **Background site error handling is a gap** — The contradiction scan and quality-gate loop
   using `spawn(...)` must emit `error!` tracing events on `Cancelled`. This is a
   correctness requirement from RISK-TEST-STRATEGY.md §Integration Risks. The test plan
   covers it via grep (look for `error!` near the spawn sites in background.rs).

4. **`spawn_with_timeout` in background.rs** — One of the 7 sites is the warmup embedding in
   `uds/listener.rs`, not `background.rs`. The two background sites (contradiction scan,
   quality-gate) use `spawn`. This distinction must be enforced and is the source of R-04 risk.

## Open Questions

1. **CI step location**: Is the enforcement step going into `.github/workflows/` (YAML `run:`
   step) or as an xtask? The test plan covers both paths but the implementer must confirm.
   Recommendation: `.github/workflows/` is simpler and doesn't require xtask infrastructure.

2. **New integration test fixture**: The `test_server_rejects_invalid_rayon_pool_size` test
   requires launching the binary with a bad config and checking exit code without connecting.
   The existing `server` fixture does not support this. Recommend implementing as a simple
   subprocess launch with `subprocess.run()` rather than modifying `conftest.py`.

3. **`ConfigError` variant naming**: The plan uses `InferencePoolSizeOutOfRange` as a
   placeholder name. The implementer should align with the existing `ConfigError` naming
   convention in `config.rs` (e.g., `InvalidRayonPoolSize { value: usize }` to match the
   `Invalid*` naming used by other variants).

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `crt-022 architectural decisions` (category: decision,
  topic: crt-022) — found ADR-001 through ADR-004 (entries #2536–#2539). All four ADRs confirmed
  and incorporated into test plan design.
- Queried: `/uni-knowledge-search` for `rayon thread pool testing patterns async bridge edge cases`
  — found entries #2535 (rayon monopolisation pattern), #2491 (Rayon-Tokio bridge pattern),
  #2539 (ADR-004), #2540 (session 1 outcome), #748 (TestHarness Server Integration Pattern).
  Entry #748 confirmed the established fixture pattern for integration tests.
- Stored: nothing novel to store — all relevant patterns (#2491 bridge pattern, #2535
  monopolisation) were already stored by prior agents in crt-022 design phase. The test plan
  techniques used here (boundary value tests for config structs, grep-based static analysis
  plans) follow established project conventions and do not constitute novel patterns.
