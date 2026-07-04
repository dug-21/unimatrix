# Test Plan — `{NEW_NAME}` (renamed from `distill_before_purge`)

**File:** `unimatrix-server/src/mcp/distill_handler.rs:48`
**Risks:** R-01 (Critical, raison d'être), R-05, R-11, R-16 · **ACs:** AC-06, AC-07, AC-08, AC-02, AC-12
**New signature:** `{NEW_NAME}(registry, feature_cycle, observations, cfg, scope: Option<&TranscriptScope>, reviewer_session_id: Option<&str>) -> Option<TranscriptCandidatesSection>`

> **RENAME (OQ-4, human directive).** `distill_before_purge` → `{NEW_NAME}`. The name is no longer vestigial
> because there is no purge; the pseudocode OVERVIEW fixes the concrete name. Every source-assertion string
> in this file uses `{NEW_NAME}` — substitute at Stage 3b/3c. Extend the existing `distill_handler.rs`
> `#[cfg(test)]` module and its fixtures (C-7 — no isolated scaffolding).

---

## Unit test expectations

### Scope-absent short-circuit (FR-6)
- `test_helper_returns_none_when_scope_none` — `scope == None` ⇒ returns `None`, **no buffer read**. Assert
  via a `snapshot`/read spy captured at return that the reader was never invoked (synchronous, R-10).
  (Extends the existing `test_helper_returns_none_on_retaindays` shape.)

### R-01 — per-loss-condition `search_complete` matrix (THE central coverage; AC-06)
Build one fixture session per loss signal, each carrying **exactly one** signal, and run a `match` no-match:

| Row | Session loss signal | `matched` | `search_complete` | Surfaced field asserted |
|-----|---------------------|-----------|-------------------|-------------------------|
| a | `elided_bytes > 0` (only) | false | **false** | `elided_bytes` present |
| b | `has_holes == true` (only) | false | **false** | `has_holes`/loss present |
| c | `provenance == Reconstructed` (only) | false | **false** | `provenance == Reconstructed` |
| d | `dropped_candidates > 0` (only) | false | **false** | `dropped_candidates` present |
| e | **OR-combination** (two signals at once) | false | **false** (OR, not AND) | both surfaced |
| f | clean `Primary`, zero loss | false | **true** (trustworthy negative) | session MAY be omitted |

- `test_search_complete_false_per_single_loss_condition` — rows a–d, table-driven.
- `test_search_complete_false_on_combined_loss_or_not_and` — row e (guards an inverted/partial predicate).
- `test_clean_primary_nomatch_is_trustworthy_negative` — row f (`search_complete == true`; omittable).
- `test_match_never_collapses_to_bare_boolean` — inspect the RESPONSE SHAPE: every returned session in a
  `match` result carries a `SessionLossInfo`; no code path yields a bare `matched` without it. (AC-06;
  R-01 sc.3 — the silent-false-negative guard.)
- `test_loss_row_present_on_match_hit_too` — a positive `match` over a lossy session STILL surfaces loss
  (the hit may be incomplete). (R-01 sc.5.)

**Coverage requirement (R-01):** the matrix above is the richest in the suite. Any no-match path that can
return without a `SessionLossInfo` fails the gate.

### R-05 — clock normalization + windowed join (AC-07, AC-08; feeds off `window.md`)
Use **explicit fixed offsets**, never `now_ts()`; the join is **windowed, never exact** (#4195/#4236).
- `test_skewed_plane_b_ts_resolved_via_window` — anchor evidence at Plane-A `ts=T`; a candidate whose
  Plane-B JSONL `ts` is offset by a realistic skew inside ±120 000 ms is **selected**. Assert an exact-match
  join would miss it and the windowed join finds it. (AC-08.)
- `test_ts_none_included_via_byte_offset_fallback` — a `ts:None` candidate inside the ±3-block byte-proximity
  window is **included AND flagged** as ts-less-included (so a consumer sees the fallback fired). Never a
  silent drop. (AC-07.)
- `test_epoch_boundary_triple` — for a ts-bearing candidate at the window edge: just-inside → included, on
  the boundary → included, just-outside → excluded. (R-05 sc.5, #4236.)
- `test_caller_never_supplies_plane_b_clock` — no test path passes a Plane-B storage timestamp; queries are
  expressed only in anchor/phase id, regex, event/time window. (AC-08 interface assertion.)
- `test_canonical_epoch_parse_at_attach` — candidate `ts` strings parse to a canonical epoch through the
  named boundary-conversion helper (#3385/#3372); a malformed `ts` degrades to `byte_offset`, not a panic.

### R-16 — degraded / second retrieval (AC-03 support; shared with `snapshot-reuse.md`)
- `test_partial_buffer_yields_reconstructed_with_loss` — `elided_bytes`/holes ⇒ `Reconstructed` provenance +
  per-session loss; assert NO verbatim text in a hole region.

### R-11 — source-assertion migration (AC-12; see also `orphan-deletion.md`, `activity-fold.md`)
Update `test_exhaustiveness_fifth_return_fails` (`distill_handler.rs:651`):
- `{NEW_NAME}(` counted **×4** (was `distill_before_purge(`) — the count STANDS; the string is renamed.
- `attach_to_response_assembly(` counted **×4** — PRESERVED.
- The `purge_cycle_transcripts(&feature_cycle)` ×4 assertion is **REMOVED with an in-source rationale
  comment** (purge gone, NG-6). `test_distill_strictly_before_purge_at_each_return` (:695, attach-before-purge
  ordering) is **DELETED with rationale**.
- `test_no_orphaned_dependency_on_removed_purge_strings` — grep the crate test files + doc comments: no other
  assertion references `purge_cycle_transcripts` count/ordering (#4044, R-11 sc.3).

## Edge cases
- `scope == Some({})` (all-None) → returns the full candidate set (delegates to `transcript-scope.md` AC-05).
- Fully-corrupt snapshot → normal response, loss surfaced (extends `test_handler_fully_corrupt_snapshot_normal_response`, `test_poison_recovery_surfaces_loss`).
