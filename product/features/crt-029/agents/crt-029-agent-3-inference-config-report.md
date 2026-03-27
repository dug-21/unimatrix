# Agent Report: crt-029-agent-3-inference-config

## Task
Implement InferenceConfig additions component for crt-029 — four new fields, serde defaults, validate() guards, pub(crate) promotions, and mod declaration.

## Files Modified

1. `crates/unimatrix-server/src/infra/config.rs` — four new InferenceConfig fields, four default fns, new ConfigError::GraphInferenceThresholdInvariantViolated variant + Display arm, four validate() guards, merge function updated, 16 new unit tests
2. `crates/unimatrix-server/src/services/nli_detection.rs` — three pub(crate) promotions: write_nli_edge, format_nli_metadata, current_timestamp_secs
3. `crates/unimatrix-server/src/services/mod.rs` — added `pub(crate) mod nli_detection_tick;`
4. `crates/unimatrix-server/src/services/nli_detection_tick.rs` — Wave 2 stub (minimal, unblocks compilation)

## Tests: 189 passed, 0 failed

All 16 new tests pass (AC-01, AC-17, AC-02, AC-03, AC-04, AC-04b). Full workspace: zero failures.

`cargo test -p unimatrix-server -- config` result: 189 passed, 0 failed.

## Pre-Merge Gates Verified

- Gate 1 (C-11): All 52 `InferenceConfig {` occurrences compile cleanly — compiler backstop confirms no bare literals missing new fields. The 50 test-site occurrences in config.rs already used `..InferenceConfig::default()` tails; the 2 in nli_detection.rs do too. The merge function bare literal was updated explicitly.
- Gate 4 (R-11): All three pub(crate) promotions confirmed present.
- `cargo build --workspace`: zero errors, three pre-existing warnings (unrelated).

## Design Decisions

- Used Option B for the cross-field error variant: added `ConfigError::GraphInferenceThresholdInvariantViolated { path, candidate, edge }` with clear field names rather than repurposing `NliThresholdInvariantViolated`. Clearer diagnostics for operators; small enum addition with no crate dependency cost.
- validate() guard order: range checks for `supports_candidate_threshold` and `supports_edge_threshold` come BEFORE the cross-field check — this ensures a precise error on boundary violations (e.g. candidate=0.0) rather than the cross-field message which would fire first at 0.0 < 0.7.
- nli_detection_tick.rs stub: minimal comment-only file to unblock Wave 1 compilation and test running. Wave 2 agent replaces it with the full implementation.

## Issues

None blocking. One non-issue encountered: initial TOML test had `[inference]` section header when deserializing directly into `InferenceConfig` — silently produced defaults instead of the test values. Fixed by removing the header (flat top-level fields). Pattern stored in Unimatrix (#3662).

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — surfaced entries #2730 (InferenceConfig struct literal trap), #3603 (StatusReport struct literal locations), #646 (serde(default) backward compat). Entry #2730 directly confirmed the ..default() tail requirement.
- Stored: entry #3662 "InferenceConfig TOML tests must use flat top-level fields — no [section] header" via /uni-store-pattern — the toml::from_str section-header silent-default trap is not captured anywhere in the existing knowledge base and will prevent the same mistake in future config extension tests.
