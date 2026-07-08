# vnc-046 Wave 4 — Boot Assertion + Field Census (agent w4)

Stage 3b, final wave. ADR-003 (#5636). Converts the "constructor-default field
never overwritten on the per-slug path" bug class from silent-read-zero into
loud-at-boot.

## Delivered

**Guard 1 — runtime per-slug boot assertion (`main.rs`)**
- `IsolationProbe` struct (cheap Arc clones + `has_hold` + `declares_signals`),
  captured in the EXISTING pre-move boot loop (before `from_servers` consumes the
  inputs), while each slug's resolved config is still in scope.
- `assert_per_slug_isolation(probe, resolver, config) -> Result<(), ServerError>`,
  generalizing `assert_wave_b_precondition`. Per built slug asserts: P1
  registry/pending `Arc::ptr_eq` convergence (write instance == read instance),
  F1/SR-03 transcript-hold pairing, P2 `services_for` reachability, P3
  hollow-counts sentinel (declared `[transcript_signals]` but empty class names).
  A REAL `Result` (`?`-propagated at the call site to abort boot), NOT a
  `debug_assert` (SR-06/R-03: debug_assert = zero coverage on the release cloud
  binary).
- Wired: `assert_per_slug_isolation` runs once per built slug at boot, right after
  `MultiProjectRouter::from_servers`, before `Arc::new(router)`. `Err` aborts boot.
- Did NOT disturb Wave 2's scanner/config param-threading at the
  `build_project_server` call site, nor Wave 3's ObserveContext construction —
  only ADDED the probe capture + assertion loop.

**Guard 2 — compile-time exhaustive field census (`server_field_census.rs`, lib crate)**
- Destructures `UnimatrixServer` with NO `..`, so a FUTURE field is a compile
  error until classified PER-SLUG / CORRECTLY-GLOBAL / CORRECTLY-PER-INSTANCE.
- Placed in the LIB crate (wired as `#[path] mod field_census;` inside server.rs)
  because module-private fields (`tool_router`, `server_info`) and `pub(crate)`
  fields are invisible to the binary crate — a main.rs census literally cannot
  name them. NON-`#[cfg(test)]` so the guard holds in the RELEASE build too.
- `categories` classified PER-SLUG to match shipped code (`slug_categories`,
  main.rs:1183); NFR-5's "global operator allowlist" prose is stale (ADR-003
  correction, documented in the census comment).

## Tests (all green)
- `main_boot_assertion_tests.rs` (bin crate), 5 tests:
  - `test_assert_per_slug_isolation_unwired_registry_returns_err` (R-03/AC-08)
  - `test_assert_per_slug_isolation_unpaired_hold_returns_err` (R-05)
  - `test_assert_per_slug_isolation_unset_config_sentinels_return_err` (R-04 P3)
  - `test_assert_per_slug_isolation_fully_wired_returns_ok`
  - `test_registry_for_ptr_eq_slug_server_registry` — PRODUCTION resolver wiring
    pin (registry + pending `Arc::ptr_eq`) + `store_config`/`inference_config`
    AC-06 white-box value-pins.
- Census compile-fail property verified by add-field/build/revert (E0027 "pattern
  does not mention field" fires at the census).
- Full suite: `cargo test -p unimatrix-server --lib` = 4513 passed, 0 failed;
  `--bin unimatrix` = 132 passed, 0 failed. No new failures.

## Files
- `crates/unimatrix-server/src/main.rs` (modified)
- `crates/unimatrix-server/src/server.rs` (modified — census `mod` wiring only)
- `crates/unimatrix-server/src/server_field_census.rs` (new)
- `crates/unimatrix-server/src/main_boot_assertion_tests.rs` (new)

## Build / lint
- `cargo build -p unimatrix-server`: green.
- `cargo clippy -p unimatrix-server --tests`: no warnings from my files. Two
  pre-existing `repeat().take()` warnings in `mcp/response/verbosity.rs`
  (unrelated newer lint on existing test code) — flagged, not touched.
- `cargo fmt` reformatted an unrelated file (`mcp/edge_write_delete_agent_tests.rs`)
  — reverted (pure fmt churn, out of scope).

## Flags for Stage 3c
- Two pre-existing clippy `repeat().take()` warnings in `mcp/response/verbosity.rs`
  (not introduced by this wave). Decide whether to fix under `-D warnings` gate.
- `store_config` / `inference_config` have no runtime sentinel and no `*_for`
  resolver method (AC-06 documented exception): they are covered by the census +
  the wiring-pin value-pins here, and must ALSO be covered behaviorally by the
  isolation-suite (INV-C / persistence-level checks) — not substitutable by these
  white-box guards.
- Guard 2 is a source-assertion (#5427): it proves a field is classified/wired,
  blind to whether the per-slug handle is actually USED on the write path. The
  behavioral suite (INV-T1/T3 via `route_observe` -> McpAdapter) is the real
  enforcement — keep it in the coverage-enumeration.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (decision/vnc-046) + context_get #5637
  (multi_thread test-flavor pattern) — applied both (ADR-003 shape; all server-
  building tests use `#[tokio::test(flavor = "multi_thread")]`).
- Stored: entry #5640 "Exhaustive field-census guard must live in the lib crate,
  not the binary" via context_store (pattern, topic unimatrix-server) — captures
  the private-field visibility trap, non-cfg-test release closure, probe-capture-
  before-move sequencing, and the config-sourced `declares_signals` gotcha.
