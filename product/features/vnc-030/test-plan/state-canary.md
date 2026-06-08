# Test Plan — C3 `state.js` `stamp_miss` canary

Source: ADR-006 rev2. AC: AC-06. Risks: R-03, R-08, R-14, R-19. File: `packages/unimatrix/test/hook-client/state-canary.test.js` (NEW); reuse `tempStateDir()` + `readHealth()` (state.test.js idioms). `npm test -- state-canary`.

The canary is a **zero-tolerance invariant** (`stamp_miss == 0`), not a rate signal. **Removed entirely** (assert their absence): 0.20 threshold, `fnf_record_send_count` denominator, `anyOtherCycleFile` concurrent-file rule, per-deployment baseline, human re-set ritual. **Test-module doc comment pins claude 2.1.167** and states OQ-E Branch-A/B disposition (test-time invariant ships either way; only the production signal is probe-gated).

## `bumpStampMiss` — content-free RMW (R-03, FR-09)

### test_bumpStampMiss_increments_count_only
- `health.json` default gains `stamp_miss: 0`. `bumpStampMiss(stateDir)` → `true`; `readHealth().stamp_miss === 1`; a second call → 2 (RMW, not overwrite).

### test_bumpStampMiss_content_free_no_topic_no_sid_no_path (SECURITY, ADR-006 §1)
- After several `bumpStampMiss` calls, assert `health.json` contains **only a count** — no topic, no session_id, no path, no any other free-form field. A malicious topic cannot poison the breadcrumb.

### test_bumpStampMiss_failopen_never_throws (R-03, NFR-03)
- Inject EACCES/EROFS on the health RMW → `false`, no throw, no stdout, no secret in stderr. Missing/corrupt `health.json` → field-by-field degrade, `stamp_miss` re-defaults to 0.

### test_health_default_stamp_miss_zero
- A fresh `health.json` breadcrumb carries `stamp_miss: 0`.

## Subagent-gated canary fixture set (R-19, FR-09, AC-06) — GATE-BLOCKING

The four binding fixtures (also in `seam-and-roundtrip.md` §4). Depth/subagent-context is simulated on the FNF decoration miss branch (the depth≥1 indicator + the inherited root session_id the event carries).

### test_depth0_never_declare_no_increment
- depth-0 top-level event, no tracker → structural noise → `bumpStampMiss` NOT called; `stamp_miss == 0`.

### test_depth1_subagent_inherited_tracker_present_no_increment
- `cycles/{root}.json` exists; a depth≥1 subagent event carrying root id finds it and stamps → no increment; `stamp_miss == 0`.

### test_depth1_subagent_noninherited_id_root_tracker_exists_one_increment
- Root tracker exists; a depth≥1 subagent carries a **non-inherited** id (inheritance drift) → no tracker found for the carried id → `bumpStampMiss` called **exactly once**; `stamp_miss == 1`.

### test_depthgt1_grandchild_no_tracker_lands_in_stamp_miss (R-14 forward-compat, ADR-006 §5)
- A depth>1 grandchild id with no tracker, while the root tracker exists → lands in `stamp_miss` (increment), proving silent loss is impossible (the canary is the sole tripwire for depth>1).

### test_healthy_single_declared_session_with_subagent_stamp_miss_zero (zero-tolerance, ships either OQ-E branch)
- End-to-end: one declared root + one depth-1 subagent inheriting the root id → `stamp_miss == 0` after the full flow.

## CLI-drift re-run check (R-08, FR-10)

### test_canary_fixtures_are_the_cli_drift_check
- Doc-comment + assertion: these AC-06 fixtures ARE the re-run-on-CLI-bump drift check (cheap, part of the standard suite). The healthy fixture references the pinned version; drift surfaces as a nonzero counter, never silent loss (NFR-08).

## Coverage requirement
Zero-tolerance invariant (`stamp_miss == 0`), not a rate signal; coverage is the subagent-inheritance-drift fixture set (positive + negative + forward-compat); content-free breadcrumb asserted; removed-knobs absence asserted; pinned CLI named in the doc comment. The test-time invariant ships regardless of the OQ-E branch.
