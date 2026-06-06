# Agent Report: vnc-025-agent-baselines

> Phase: Stage 3b Wave 0 — pre-change baseline capture (Gate 3a W3 / OQ-5)
> Date: 2026-06-06
> Result: COMPLETE — all three baselines captured from current HEAD; zero production code modified

## Baselines Captured

All fixtures live in `crates/unimatrix-server/tests/fixtures/vnc-025/`. Each is produced and gated by a capture test that emits the fixture on first run and asserts **byte identity** against the committed file on every subsequent run — the same tests are the post-change hard gates.

| # | Baseline | Fixture file(s) | Capture test | Plan ref |
|---|----------|-----------------|--------------|----------|
| 1 | Empty-buffer `CompactPayload` → `BriefingContent` (R-09.4/FR-18 hard gate) | `compact_payload_empty_buffer.unknown_session.json`, `.registered_no_state.json`, `.registered_with_histogram.json` | `uds::listener::tests::test_compact_payload_empty_buffer_byte_identical` | dispatch-wiring §3 |
| 2 | `context_cycle_review` rendered output (AC-09) | `cycle_review_render.markdown.json`, `cycle_review_render.format_json.json` | `mcp::tools::tests::test_cycle_review_render_baseline_byte_identical` | cycle-review-purge §4 |
| 3 | `SignalOutput` + persisted-queue serialization (ADR-004 drain-signature guard) | `signal_output_drain.txt`, `signal_record_wire.json` | `infra::session::tests::test_signal_output_shape_unchanged` | registry-wiring §2 |

## How Each Was Produced

1. **CompactPayload**: full `dispatch_request(HookRequest::CompactPayload{..})` through the existing listener test harness (`make_store`/`make_services`/direct-dispatch pattern). Three scenarios: unknown session; registered session with no state; registered session with role/feature/category-histogram (distinct counts to avoid sort-tie nondeterminism) and one warm-up compaction so content is non-empty (`Compaction: #2` line). Serialized via `serde_json::to_string(&HookResponse)` — the wire form. No timestamps appear in any payload.
2. **Cycle review**: replays the handler's full-pipeline render path with all clock/store inputs pinned — `default_rules(None, vec![])` (asserts rule count == 23) → `detect_hotspots` → `compute_metric_vector(.., PINNED_NOW=2_000_000_000)` → `build_report` → `recommendations_for_hotspots` → `synthesize_narratives` → `dispatch_review_with_advisory` in both `markdown` and `json` formats. Fixed 9-record, 2-session corpus trips `SearchViaBashRule` (33% > 5%) and `orphaned_calls`, so hotspots/recommendations/narratives are all non-empty in the snapshot. Snapshot = pretty-printed `CallToolResult`.
3. **SignalOutput**: real `drain_and_signal_session` for success / rework (3 edit-fail-edit cycles) / abandoned scenarios → Debug doc (entry ids are sorted by `build_signal_output_from_state`, deterministic). Plus serde-JSON of the `SignalRecord`s produced by the exact `write_signals_to_queue` field mapping with `created_at` pinned — the shape that feeds the persisted SIGNAL_QUEUE.

## Infrastructure Extended (cumulative, no isolated scaffolding)

- `crates/unimatrix-server/src/test_support.rs`: new `vnc025_fixture_dir()` + `assert_matches_committed_baseline()` — follows the unimatrix-engine `bindings/fixtures` precedent (CARGO_MANIFEST_DIR-anchored, committed file is contract authority).
- Capture tests added inside the existing `#[cfg(test)]` modules of `listener.rs`, `tools.rs`, `session.rs` — exactly where the test plans designate.

## Verification

- All three tests pass and were re-run 3× against the committed fixtures (assert path, not emit path) — byte-stable.
- Full `cargo test -p unimatrix-server --lib`: 3506 passed, 0 failed.
- `cargo fmt` clean; clippy reports no warnings in the new code (pre-existing crate-wide warnings untouched; pre-existing `unimatrix-observe` `-D warnings` failures are unrelated toolchain-lint drift).
- Diff: 4 files, +434 lines, ALL inside test modules / test_support. `listener.rs:1009` and all production paths untouched.

## Issues / Required Next Step (SM)

1. **COMMIT REQUIRED BEFORE WAVE 1**: the 7 fixture files + 4 source files must be committed before any production edit. The helper is emit-if-absent — if the fixtures are lost and re-emitted from post-change code, the baseline is silently destroyed (#2984 anti-pattern, the exact failure OQ-5 warns about).
2. AC-09 scope note: the cycle-review baseline pins the deterministic render path (detection → report → format dispatch), not the rmcp tool envelope (`RequestContext` is not constructible in unit tests). MCP-envelope-level regression remains covered by the infra-001 harness suites per the OVERVIEW.md plan. Stage 3b's `test_cycle_review_output_unchanged_by_purge` should consume these same committed fixtures.
3. Baseline 3 pins the persisted serialization as serde-JSON of `SignalRecord` (matches the actual `insert_signal` SQL+JSON path); bincode `serialize_signal` is not re-exported from unimatrix-store and is migration-only — intentionally not snapshotted.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — surfaced vnc-025 ADRs #4739/#4740/#4741/#4742/#4744 (lock discipline, audit shape, tee point) and #4725 (transport-convergence test pattern); applied ADR context when choosing capture surfaces. context_search (pattern, k=3) — no existing baseline-fixture pattern found.
- Stored: entry #4747 "Pre-change baseline gate: emit-if-absent, byte-assert-if-present fixture helper in test_support.rs" via /uni-store-pattern.
