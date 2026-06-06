# Agent Report: vnc-025-agent-5-config-knob

Component: config-knob (ADR-006, FR-10, R-11). Commit: `f1c14876`.

## Files Modified

- `crates/unimatrix-server/src/infra/config.rs` — feature change (all five sites per pattern #4070):
  1. `RetentionConfig.transcript_buffer_max_bytes: usize` beside `transcript_retention`, `#[serde(default = "default_transcript_buffer_max_bytes")]`, full ADR-006 doc comment (accumulated-buffer semantics, no-global-cap posture, evidence triggers).
  2. `default_transcript_buffer_max_bytes() -> usize { 4_194_304 }`.
  3. `impl Default for RetentionConfig` entry.
  4. `validate()` floor: `< 65_536` → `ConfigError::RetentionFieldOutOfRange` naming the field and the 64 KiB floor (same pattern as siblings). Unconditional check.
  5. `merge_configs` retention literal: project-wins arm mirroring the `transcript_retention` arm.
- `crates/unimatrix-server/src/services/status.rs` — compile collateral only: 4 exhaustive `RetentionConfig { ... }` test literals in `crt_036_gc_block_tests` converted to `..RetentionConfig::default()` spread (PurgeOnCycleClose IS the default — semantics unchanged); dropped the now-unused `TranscriptRetention` import. Same conversion applied to 8 exhaustive literals inside config.rs's own test module.

## Tests Added (config.rs test module)

- `test_transcript_buffer_max_bytes_default_when_absent` — absent block, present-block-field-absent, Default/defaulter-fn agreement (all 4_194_304).
- `test_transcript_buffer_max_bytes_explicit_value_respected` — explicit TOML round-trip, siblings keep defaults.
- `test_validate_rejects_below_floor` — 65_535 rejected (variant + field name + floor in message); 65_536 accepted (boundary).
- `test_validate_accepts_default` — default passes validate.
- `test_transcript_buffer_max_bytes_project_overrides_global` — both non-default, project wins.
- `test_transcript_buffer_max_bytes_global_used_when_project_absent` — project at default, global wins.

## Results

- `cargo build --workspace`: pass.
- `cargo test --workspace`: 5514 passed, 0 failed (includes the 6 new tests).
- `cargo fmt`: applied. Clippy: no diagnostics in any edited region; pre-existing workspace clippy failures (`unimatrix-observe` lib, `unimatrix-store`/`unimatrix-vector` test targets) confirmed present on a clean stash — not introduced here.

## Deferred (per spawn prompt — not mine)

- §3 cap-chain integration test (`test_config_cap_reaches_session_buffer`) and the construction-site grep gate: require `SessionRegistry::with_transcript_cap` (Wave 2 registry-wiring). Per Gate 3a W1, the gate must assert only `main.rs:645/:1068` — `server.rs:335` is the test ctor (overwritten at `main.rs:752/:1174`), keeps `SessionRegistry::new()`.
- Cross-module default pin: `DEFAULT_TRANSCRIPT_BUFFER_MAX_BYTES` in the (parallel, uncommitted-at-edit-time) `infra/session_transcript.rs` verified equal to 4_194_304; config keeps its own literal with a cross-ref comment. A single cross-module pin test can land once both modules are wired.

## Issues

None blocking. Two ts-rs fixture JSONs were regenerated (key-order only) by the test run; restored via checkout — out of scope.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — surfaced ADR-006 (#4744, followed verbatim), pattern #4070 (five-site config extension incl. hidden merge_configs literal — applied: grepped all `RetentionConfig {` literals workspace-wide, fixed the 12 exhaustive ones), lesson #3905 (post-merge revalidation context).
- Stored: nothing novel to store — pattern #4070 already captures the five-site + grep-all-literals procedure; this task was a direct application of it to RetentionConfig with no new gotchas discovered.
