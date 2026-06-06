# Test Plan: cycle-review-purge (`mcp/tools.rs:1918` handler gate + clear)

Covers R-10; AC-09, FR-15/16. Integration tests through the tool handler (existing `tools.rs`
test patterns); clear-method semantics partly shared with registry-wiring.md.

## §1 Attribution matrix — R-10.1

- `test_cycle_review_clears_only_matching_feature_sessions` — mixed registry:
  `feature == Some(reviewed)`, `Some(other)`, `None`. Only the first group's buffers cleared
  (empty afterward); other/None buffers untouched; ALL sessions stay registered; returned
  `Vec<TranscriptPurgeRecord>` matches the cleared set exactly (ids + byte counts).
- `test_cycle_review_zero_attributed_sessions_noop` — review for a feature with no attributed
  sessions: no-op, no audit row, no error, review output normal (R-10.5).

## §2 Retention gate — FR-16

- Compile-level: exhaustive `match` on `TranscriptRetention` — no `_` arm, no `if let`
  (review gate; a new enum variant must force a compile error).
- `test_cycle_review_purges_under_purge_on_cycle_close` — default OSS config: clear runs.
- Non-`PurgeOnCycleClose` arm: OSS `validate()` rejects `RetainDays`, so behavior is
  enterprise-seam only — assert via direct registry/handler unit call with the enum value
  injected (bypassing validate) that NO clear occurs, if constructible; otherwise the
  exhaustive-match review gate carries it. Do not weaken validate() to test this.

## §3 Post-clear semantics — R-10.2/.3 (crt-052 inherits)

- `test_post_clear_resumed_stream_serves_tail` — after clear, deltas continue at high file
  offsets (the client doesn't know the server cleared): the implementation's chosen behavior
  (large hole → ring-tail/collapse) is asserted explicitly — final `contiguous_tail` serves
  the newest contiguous bytes, no panic, no gap filler. Behavior must be pinned by this test,
  not emergent.
- `test_clear_return_and_metadata_semantics_pinned` — `clear()` returns bytes purged;
  post-clear `high_water`/`elided_bytes` values asserted (whatever Stage 3b defines, pin it
  here — crt-052 builds on these).

## §4 Review output unchanged — AC-09

- `test_cycle_review_output_unchanged_by_purge` — snapshot the full `context_cycle_review`
  tool response for a fixed corpus BEFORE Stage 3b changes; assert post-change response
  identical for the same corpus (purge is a side effect, never visible in output; no new
  fields, no count changes to the 23 detection rules).
- Audit emission from this purge point: purge-audit.md §1/§2 (`trigger=cycle_review`,
  `log_event_async` from the async handler context).

## §5 MCP-harness regression (Stage 3c)

Existing infra-001 cycle_review tests in `test_tools.py`, `test_lifecycle.py`,
`test_protocol.py` pass unmodified — the MCP-level half of "output otherwise unchanged"
(see OVERVIEW.md Integration Harness Plan).
