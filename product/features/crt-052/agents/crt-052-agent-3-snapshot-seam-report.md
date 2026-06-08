# Agent Report — crt-052 C1 Snapshot Seam (`take_transcripts_for_feature`)

Agent: crt-052-agent-3-snapshot-seam | Component: C1 (Wave A) | Issue: #689

## Summary
Implemented the snapshot-and-release seam `take_transcripts_for_feature` in
`infra/session.rs` per ADR-001/ADR-002, sibling to `clear_transcripts_for_feature`.
Two-phase lock discipline; the held-buffer scan (Wave B) is a severable branch
reached through a locally-owned `HeldBufferScan` trait + optional handle, so
session.rs has zero compile-time reference to `transcript_hold.rs` (R-11).

## Files Modified
- `crates/unimatrix-server/src/infra/session.rs`
  - `take_transcripts_for_feature(&self, feature_cycle: &str) -> Vec<(String, TranscriptSnapshot)>`
  - `pub trait HeldBufferScan` (Wave-A-owned severable seam) + `transcript_hold: Option<Arc<dyn HeldBufferScan>>` field + `with_transcript_hold(...)` builder
  - import of `TranscriptSnapshot`
  - 11 unit tests appended to the existing transcript test suite

## Lock Discipline (Constraint 1 / AC-01 / R-08)
- Phase 1 (registry lock): linear scan `feature.as_deref() == Some(feature_cycle)`, `Arc::clone` matching buffers, release.
- Phase 2 (per-buffer lock): `lock_buffer(&arc)` (poison-recovers + clear_poison per #4764), `buf.snapshot()` byte copy + metadata, release.
- No parse/marker/I/O under any lock; consumers read the owned Vec (#3753). Buffers NOT cleared (purge is separate, ADR-005).

## Tests
11 new, all pass: `test_seam_returns_owned_snapshots_with_metadata`,
`test_seam_none_feature_never_matches`, `test_seam_empty_registry_returns_empty`,
`test_seam_no_match_returns_empty`, `test_seam_does_not_clear_buffers`,
`test_seam_no_parse_under_lock` (AC-01a merge gate, comment-stripped source assertion),
`test_seam_poisoned_buffer_recovers_treat_as_empty` (R-16),
`test_concurrent_deltas_during_seam_consistent` (AC-01b merge gate, 4-writer stress, no torn frame),
`test_seam_scans_registered_and_held` (R-13), `test_seam_no_double_snapshot_arc_identity` (R-13 Arc dedup),
`test_seam_wave_a_only_registered_scan` (R-11).

- `cargo test -p unimatrix-server --lib seam` → 11 passed / 0 failed.
- `cargo test -p unimatrix-server --lib` → 3696 passed, 1 ignored, 1 flaky unrelated failure (`http::token::tests::test_concurrent_creation_no_corruption` — passes in isolation; pre-existing concurrency flake, not in my scope).
- `cargo build --workspace` clean. `cargo clippy -p unimatrix-server --lib` → no warnings in new code (pre-existing crate warnings unchanged). `cargo fmt` applied.

## Held-scan seam (how Wave B / C8 injects without C1 importing transcript_hold.rs)
C1 owns `pub trait HeldBufferScan { fn held_arcs_for_feature(&self, feature_cycle: &str) -> Vec<(String, Arc<Mutex<TranscriptBuffer>>)>; }` in `session.rs` (Wave A). The registry holds `transcript_hold: Option<Arc<dyn HeldBufferScan>>`, `None` by default. In phase 1, after the registered scan, `if let Some(hold) = self.transcript_hold.as_ref()` calls `hold.held_arcs_for_feature(feature_cycle)` and pushes each Arc not already present by `Arc::ptr_eq` (R-13 dedup). Wave B's `transcript_hold.rs` will `impl HeldBufferScan for HeldStore` and the constructor will wire it via `SessionRegistry::with_transcript_hold(Arc::new(store))`. Dependency direction is one-way: C8 depends on session.rs's trait; session.rs never `use`s transcript_hold. Reverting Wave B drops the impl + the `with_transcript_hold` call site → field stays `None` → branch inert → C1 scans registered buffers only, compiles, tests pass, ships degraded (ADR-009 safe revert target). Verified: grep finds only the field NAME, never `use`/`::transcript_hold`/`mod transcript_hold`.

## Issues / Blockers
- None for C1. Note: sibling crate `unimatrix-observe` (C3/C4) is mid-flight in the shared working tree; an interim build hit a stale `distill` module error that resolved once their modules + the `regex` dep landed. Did not touch their files; committed only `session.rs`.

## Cited, not reworked
vnc-030 ADR-007 §2 close/sweep precedence (#4819, PR #702) — `None` never matches; declared-attribution chain untouched (Constraint 13). vnc-025 `clear_transcripts_for_feature` left unchanged (counts-only) — seam is additive.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search — surfaced ADR-001 (#4847), ADR-002 (#4848), #3753 (use snapshot never relock), #4799 (per-turn drain), #4764 (poison recovery via lock_buffer). Applied all.
- Stored: entry #4861 "Severable revert-boundary seam via locally-owned trait + Option<Arc<dyn>> handle (satisfies no-cross-module-use merge gate)" via /uni-store-pattern — covers the dependency-inversion severable-seam technique AND the comment-stripping gotcha for source-assertion lock-scope tests.
