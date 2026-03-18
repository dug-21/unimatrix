# Agent Report: dsn-001-agent-10-startup-wiring

## Task
Wire config loading into the startup sequence in `crates/unimatrix-server/src/main.rs` and update `background.rs` to accept `Arc<ConfidenceParams>`.

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/main.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/background.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/main_tests.rs`

## Changes Made

### main.rs

Added imports: `std::collections::HashSet`, `unimatrix_engine::confidence::ConfidenceParams`, `unimatrix_server::infra::config::{load_config, resolve_confidence_params, UnimatrixConfig}`, `unimatrix_server::infra::registry::Capability`.

In both `tokio_main_daemon` and `tokio_main_stdio`, immediately after the tracing log of project paths (before `handle_stale_pid_file`):

1. `dirs::home_dir()` → `load_config()` block with non-fatal fallback to `UnimatrixConfig::default()` on both `None` and `Err(e)`.
2. `Arc::new(resolve_confidence_params(&config))` with `unwrap_or_else` fallback to `ConfidenceParams::default()`.
3. Extraction of `knowledge_categories`, `boosted_categories`, `server_instructions`, `permissive`, `session_caps`.

Updated:
- `AgentRegistry::new(Arc::clone(&store), permissive, session_caps)` — replaces hardcoded `true, vec![]`
- `CategoryAllowlist::from_categories(knowledge_categories)` — replaces `CategoryAllowlist::new()`
- `UnimatrixServer::new(..., server_instructions)` — replaces `None`
- `ServiceLayer::new(..., boosted_categories)` — replaces stub comment + default HashSet
- `spawn_background_tick(..., Arc::clone(&confidence_params))` — new final arg in both call sites

### background.rs

Added `use unimatrix_engine::confidence::ConfidenceParams`.

Added `confidence_params: Arc<ConfidenceParams>` as last parameter to:
- `spawn_background_tick` (public) — threads into inner spawn
- `background_tick_loop` (private async fn) — prefixed `_confidence_params` (no call sites yet; threaded for future use per pseudocode)

### main_tests.rs

Added 6 new tests:
- `test_main_startup_handles_no_home_dir` — R-15: default config produces `ConfidenceParams::default()`
- `test_default_config_categories_match_initial_categories` — AC-01: default config matches `CategoryAllowlist::new()`
- `test_default_config_boosted_categories_is_lesson_learned` — AC-01: default boosted is `["lesson-learned"]`
- `test_default_config_agents_permissive_is_true` — AC-01: default trust is "permissive"
- `test_arc_confidence_params_from_empirical_preset` — IR-04: empirical preset produces `w_fresh=0.34`, `half_life=24.0h`
- `test_run_stop_is_synchronous` — pre-existing structural test (unchanged)

## Build

Pass — zero errors. Warnings are pre-existing.

## Tests

Pass — 29/29 binary tests. 10 pre-existing failures in workspace (pool timeout + uds concurrency races, same as baseline before these changes).

## Deviations from Pseudocode

None. One note: pseudocode says "Inside `background_tick_loop`, all calls to `compute_confidence(entry, now)` become `compute_confidence(entry, now, &confidence_params)`" — there are currently zero such calls in `background_tick_loop`. The parameter is threaded and prefixed `_confidence_params` per Rust convention for intentionally unused parameters.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` startup/config injection — no prior patterns found.
- Stored: entry #2328 "dsn-001 Startup Config Injection: Non-Fatal Fallback with Arc<ConfidenceParams> Threading" via `/uni-store-pattern`.
  - Key gotcha: `INITIAL_CATEGORIES` is private — use `CategoryAllowlist::new().list_categories()` in tests instead of direct import.
  - Key gotcha: config failures are non-fatal (fallback to defaults), not abort-on-error.
  - Key gotcha: `_confidence_params` prefix needed in `background_tick_loop` — no current call sites.
