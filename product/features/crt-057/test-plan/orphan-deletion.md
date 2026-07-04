# Test Plan — Orphan Deletion + Exhaustive-Match Re-Home

**Files:** `unimatrix-server/src/server.rs:661` (`purge_cycle_transcripts`), `session.rs`
(`clear_transcripts_for_feature`), `infra/transcript_hold.rs:331` (`purge_held_for_feature`)
**Risks:** R-06 (High), R-11 · **ACs:** AC-13 (+ AC-12 source-assertion)

> ANTI-STUB (CLAUDE.md rule 2). Once the four review-site purge calls are removed, these three functions lose
> all non-test callers. They MUST be **deleted** (dead-code / clippy) — NOT `#[allow]`ed. The exhaustive
> `TranscriptRetention` match lived INSIDE the deleted purge; its C-5 obligation **relocates** to the
> backstops (tested here + in `backstop-reclaim.md`), it does not disappear.

---

## R-06 — dead-code removal (AC-13)
- `test_purge_cycle_transcripts_deleted` / `test_clear_transcripts_for_feature_deleted` /
  `test_purge_held_for_feature_deleted` — assert each symbol no longer exists as a defined item (source
  absence, or rely on the `#![deny(dead_code)]`/clippy gate to fail the build if left). Removal, not
  `#[allow]`. (AC-13, R-06 sc.1.)
- `test_no_non_test_caller_of_removed_purge` — reachability check (#5383): the only prior callers were the
  four review-site calls; assert none remain in production code.
- `test_no_new_cycle_close_or_review_purge_trigger` — assert no purge fires at review or at
  `context_cycle(stop)`; reclamation only via TTL/cap/session-close. Keyed on synchronous buffer state
  (R-10). (Ties R-08; R-06 sc.4.)

## R-06 — exhaustive `TranscriptRetention` re-home (AC-13; C-5)
The match that governed reclamation moves onto the surviving backstop reclaim path.
- `test_transcript_retention_match_exhaustive_no_wildcard` — the re-homed match covers every
  `TranscriptRetention` variant explicitly; `RetainDays` stays a **no-op**; **no `_` catch-all arm** (#4831).
  Supply each variant to the reclaim path and assert no arm was dropped and retention behavior is
  **byte-unchanged** vs the pre-deletion behavior. (R-06 sc.3.)

## R-11 — no orphaned dependency on removed strings
- `test_no_test_references_removed_purge_count` — grep every test file + doc comment: nothing still
  references the removed `purge_cycle_transcripts` count/ordering assertions (#4044). Shared with
  `distill-before-purge.md` §R-11.

## Notes
- The backstops actually reclaiming (TTL / cap / session-close, each with a content-free audit) is tested in
  `backstop-reclaim.md` — this file proves the DELETION and the match RE-HOME; that file proves the surviving
  path still reclaims correctly.
