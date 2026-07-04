# crt-057 Wave 2 — Server-crate agent report (crt-057-agent-4-server)

**Status:** COMPLETE. `cargo build --workspace` clean, `cargo clippy --workspace --all-targets -D warnings` clean, `cargo test --workspace` rc=0, `cargo test -p unimatrix-server --lib` = 4351 passed / 0 failed / 1 ignored (pre-existing). Committed as `8781efe3` on `feature/crt-057`.

## Files modified
- `crates/unimatrix-observe/src/lib.rs` — re-export the Wave-1 types (`TranscriptScope`, `Window`, `SessionSearchStatus`, `ResolvedBounds`, `BoundsKind`, `DEFAULT_WINDOW_*`) from the crate root (they were defined in types.rs but not exported).
- `crates/unimatrix-server/src/mcp/distill_scope.rs` — **NEW** (352 lines): bounded-regex compile/validate, dependency-free ISO-8601→epoch parser + `candidate_epoch_ms`, windowed-never-exact join (`ts_within_window`/`phase_contains_ts`), `block_within` byte-offset fallback, `ScopeCtx`, `scope_predicate`, `derive_search_status`.
- `crates/unimatrix-server/src/mcp/distill_scope_tests.rs` — **NEW** (323 lines): split out per the crate's `_tests.rs` convention to keep the module under 500 lines.
- `crates/unimatrix-server/src/mcp/distill_handler.rs` — rename `distill_before_purge`→`retrieve_scoped_candidates` (+`scope`,`reviewer_session_id`), scope filter + `build_scope_ctx`/`apply_scope_filter`, `attach_search_status`, header rewrite, source-assertion test migration, new R-01/R-09 tests.
- `crates/unimatrix-server/src/mcp/tools.rs` — `RetrospectiveParams.transcript` field + axis docs, tool-description rewrite (3 axes + no purge verb), up-front `validate_scope_regex`, four-site threading, **four `purge_cycle_transcripts` calls deleted**, `"summary"` arm dropped ×4, two purge tests deleted/reworked.
- `crates/unimatrix-server/src/mcp/mod.rs` — register `distill_scope`.
- `crates/unimatrix-server/src/server.rs` — **delete `purge_cycle_transcripts`**; add `reclaim_permitted_by_retention` (re-homed exhaustive match, no `_` arm); repoint `test_retention_match_no_wildcard`; delete 4 purge-behavior tests + helpers.
- `crates/unimatrix-server/src/services/status.rs` — gate the sole surviving reclamation driver (`sweep_held_buffers`) with `reclaim_permitted_by_retention`.
- `crates/unimatrix-server/src/infra/session.rs` — **delete `clear_transcripts_for_feature`** + its 3 tests.
- `crates/unimatrix-server/src/infra/transcript_hold.rs` — **delete `purge_held_for_feature`**.
- `crates/unimatrix-server/src/infra/{transcript_hold_tests,transcript_hold_ac11_tests,transcript_hold_activity_tests}.rs`, `infra/validation.rs` — migrate backstop coverage onto `sweep_expired(Duration::ZERO)`; add `transcript: None` to param literals.

## New/updated tests (all passing)
- distill_scope: `test_parse_iso8601_*`, `test_epoch_boundary_triple_inside_on_outside`, `test_skewed_plane_b_ts_resolved_via_window_not_exact`, `test_phase_contains_is_self_bounding_no_window`, `test_block_within_ts_none_byte_fallback`, `test_validate_scope_regex_ok_and_error`, R-01 matrix (`test_search_complete_false_per_single_loss_condition`, `_on_combined_loss_or_not_and`, `test_clean_primary_nomatch_is_trustworthy_negative`, `test_match_never_collapses_to_bare_boolean`, `test_loss_row_present_on_match_hit_too`, `test_anchor_only_scope_matched_is_none`).
- distill_handler: `test_helper_returns_none_when_scope_none`, `test_empty_scope_returns_full_candidate_set`, `test_match_scope_narrows_intersection`, `test_non_destructive_repeat_identical_candidates`; existing gate/poison/corrupt tests migrated to the new signature; `test_exhaustiveness_fifth_return_fails` migrated (retrieve×4, attach×4, purge==0, rationale); attach-before-purge ordering test deleted with rationale.
- server: `test_retention_match_no_wildcard` repointed to `reclaim_permitted_by_retention`.

## Confirmations (requested)
- **Four purge calls deleted:** `grep -c self.purge_cycle_transcripts tools.rs` = 0.
- **Three helpers deleted:** no `purge_cycle_transcripts` / `clear_transcripts_for_feature` / `purge_held_for_feature` definitions remain (real deletes, no `#[allow]`).
- **Retention match re-homed:** `server::reclaim_permitted_by_retention` (no `_` arm) consulted at `services/status.rs` TTL-sweep driver; OSS behavior byte-unchanged.
- **`"summary"` dropped ×4:** 0 `"markdown" | "summary"` arms remain; all four loci route to `ERROR_INVALID_PARAMS` with the exact `Valid values: "markdown", "json".` message.

## Rework (coordinator follow-up) — anchor/phase wired end-to-end (commit 38b646c7)

**New signature** (widened by one param — the architecture's 6-param shape was under-specified for its own anchor/phase semantics; docs OVERVIEW / ADR-006 / brief Key-Signatures should reconcile to this):
```
fn retrieve_scoped_candidates(
    registry: &SessionRegistry,
    feature_cycle: &str,
    observations: &[ObservationRecord],
    cfg: &RetentionConfig,
    scope: Option<&TranscriptScope>,
    reviewer_session_id: Option<&str>,
    resolved_bounds: Option<ResolvedBounds>,   // NEW — handler-resolved anchor/phase span
) -> Option<TranscriptCandidatesSection>
```
Companion handler resolver (tools.rs, expressed once, invoked at all four success returns):
```
fn resolve_transcript_scope_bounds(
    scope: Option<&TranscriptScope>,
    hotspots: &[HotspotFinding],       // anchor F-NN → evidence[].ts span
    cycle_events: &[CycleEventRecord], // phase → compute_phase_stats window (sec→ms), self-bounding
) -> Option<ResolvedBounds>            // None ⇒ absent section (FR-7), never an error
```
Anchor-first. `retrieve_scoped_candidates` guards: a scope that requests anchor/phase whose id did not resolve (`resolved_bounds == None`) ⇒ absent section, NOT a full dump. Full-pipeline + force resolve both filters end-to-end; cached-MetricVector/memo/purged degenerate paths (no hotspots/cycle_events in scope) resolve to absent (documented in-code). `resolved_bounds` is also surfaced to the caller via `attach_search_status` (FR-16).

New tests: `resolve_anchor_bounds` unit matrix (distill_scope_tests); `test_resolve_transcript_scope_bounds_anchor_and_phase` (tools.rs — anchor→hotspots span, phase→cycle_events sec→ms window, unknown→None); end-to-end filtering (distill_handler): `test_anchor_bounds_filters_candidates_within_window`, `test_anchor_and_match_and_compose`, `test_unknown_anchor_id_yields_absent_section`, `test_phase_bounds_are_self_bounding_ignore_window`, `test_anchor_scope_loss_honesty_preserved`.

## Flags / deviations (for the leader)
- **Anchor label `F-NN` is positional over `report.hotspots`** (1-based, matching the markdown `format!("F-{:02}", i+1)` convention and the JSON path's raw ordering). The markdown formatter assigns labels over its *collapsed* findings view; positional resolution is exact when detection rules are distinct (the common case) — same-rule collapse is a documented edge.
- **Degenerate-path honesty:** on cached-MetricVector (empty hotspots), memo-hit, and purged-signals returns, `cycle_events` are not in scope, so `phase` resolves to an absent section there; `anchor` resolves at memo/purged when the served report carries hotspots. Full anchor+phase resolution is on the full-pipeline (and `force`) return. This is the coordinator-accepted degenerate behavior, documented in-code at each site.
- **`land_activity_fold` is present once (full-pipeline), not literally ×4.** The "fold gated ×4" in activity-fold.md is a design abstraction; the real code folds once before report build. Left UNCHANGED (not touched). The surviving ×4 source invariants are `retrieve_scoped_candidates` and `attach_to_response_assembly`; no fabricated fold-×4 assertion was added (it would not compile-match).
- **500-line limit:** `distill_scope.rs` split to 352 lines (tests in sibling). `distill_handler.rs` is 1092 lines — it was already 806 (over-limit) pre-crt-057; the task directed extending it inline (cumulative fixtures), so its inline test module was kept. Production-only line counts are well under 500 for both.
- Out of scope per instructions: `.claude/skills/**` + `.claude/protocols/**` (Wave 3), integration tests (Stage 3c).

## Knowledge Stewardship
- Queried: `context_search` (pattern `context_cycle_review four success returns...`; decision `crt-057 ... topic=crt-057`) — surfaced #4750 (four-success-return gating), #4866 (assembly-level attach), ADR-006 #5438 / ADR-002 #5434 / ADR-004 #5436 / ADR-003 #5435. Applied all.
- Stored: entry #5439 "Orphan-deleting a pub fn in the unimatrix-server binary crate is mandatory (dead_code), and its coverage must re-home onto the surviving path" via `/uni-store-pattern` (also captures the serde-only-observe-type → `#[schemars(with = "Option<serde_json::Value>")]` gotcha; related to existing #3813).
