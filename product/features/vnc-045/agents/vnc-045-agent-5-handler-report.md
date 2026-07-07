# Agent Report — vnc-045-agent-5-handler (Wave 3: `context_tag` handler)

## Summary
Implemented the `context_tag` MCP `#[tool]` handler + `TagParams` struct in
`mcp/tools.rs`, the two extracted `pub(crate)` seam fns (`derive_namespace`,
`check_tag_lifecycle` + `LifecycleRejection`), and the `format_tag_success` helper.
Removed the forward-wired `#![allow(dead_code)]` on `services/store_tag.rs` — the
whole `StoreTagService` surface is now consumed by the handler (no residual allow needed).

Handler flow (matches pseudocode context-tag-handler.md):
build_context_with_external_identity → require_cap(Write) [RETROFIT SEAM #2, comment only]
→ TagAction::parse (unknown → invalid_params) → non-empty tag check → derive_namespace
→ entry_store.get → check_tag_lifecycle (Quarantined → invalid_params) → value-opacity
pre-write seam (comment only, NO validator/evaluate/validate_outcome_tags) → delegate to
StoreTagService::tag → format_tag_success.

Value-opacity honored: any tag written uninterpreted; no ProtectedTagsConfig, no
min_trust_level, no Capability::Tag, no add/remove/replace capability split, no cadence guard.
`agent_id` is audit-only. Per #5468 no attempt to unit-construct the `#[tool]` handler —
route/format proofs are deferred to Stage-3c.

## Files modified
- `crates/unimatrix-server/src/mcp/tools.rs` — TagParams, context_tag `#[tool]` handler,
  `derive_namespace`, `check_tag_lifecycle`/`LifecycleRejection`, `format_tag_success`,
  10 unit tests in `mod tests`.
- `crates/unimatrix-server/src/services/store_tag.rs` — removed module-wide `allow(dead_code)`
  + its rationale comment (surface now consumed).

## Tests
- New unit tests: 10 passed / 0 failed
  - R-06 `derive_namespace` boundary table (7): standard, colon-terminated, colon-less,
    multi-colon, mid-string-colon, leading-colon, empty.
  - R-05 `check_tag_lifecycle` (3): quarantined refused, deprecated allowed, active allowed.
- Regression: `store_tag` seam suite 17 passed / 0 failed after removing the allow.
- `cargo build -p unimatrix-server`: clean.
- `cargo clippy -p unimatrix-server -- -D warnings`: clean.
- `cargo fmt`: applied; reverted one out-of-scope churn hunk
  (`mcp/edge_write_delete_agent_tests.rs`) per swarm fmt-churn rule.

## Issues / blockers
None. Did not touch integration tests (Stage 3c), `context_correct`,
`validate_outcome_tags`, config, or server state.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` x2 —
  pattern search surfaced #4707 (RequestContext as named tool-handler param) and #3814
  (MCP param deserialization needs transport-level validation, unit tests insufficient),
  reinforcing the #5468 no-unit-handler constraint; decision search surfaced #5609 (ADR-008
  Write-gate trust seam / agent_id audit-only / value-opacity hygiene seam) and #5610
  (ADR-009 audit shape). Applied: gate location as marked seam #2, value-opacity pre-write
  point as marked seam #1, both comment-only.
- Stored: nothing novel to store — the extracted-seam-fn pattern for non-unit-constructible
  `#[tool]` handlers is already captured (#5389/#5468); the value-opacity marked-seam posture
  is documented in ADR-008 (#5609). No new runtime-invisible gotcha discovered.
