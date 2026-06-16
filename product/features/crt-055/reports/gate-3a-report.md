# Gate 3a Report: crt-055

> Gate: 3a (Component Design Review)
> Date: 2026-06-16
> Result: **PASS** (final re-validation, iteration 2 — AC-22 rework verified complete)
>
> --- FINAL RE-VALIDATION (iteration 2) appended at top; iteration-1 and iteration-0 reports retained below for provenance ---

## FINAL RE-VALIDATION (iteration 2) — Result: PASS

**Focus**: confirm the AC-22 fix from the iteration-1 rework is now complete across the ENTIRE feature, with special attention to `test-plan/OVERVIEW.md` (lines 35, 93) which iteration 1 flagged as still carrying the old "+500ms counts / == 2" defect.

### Verdict per requested item

| Item | Finding | Status |
|------|---------|--------|
| (a) `test-plan/OVERVIEW.md` lines 35 & 93 corrected | **FIXED.** Line 35 (R-08 row) now reads "Seed exact-boundary / −500ms / +1s: +1s (floors `T+1`) counts; exact-boundary (floor `T`, strict `>`) and −500ms (floor `T−1`) do not; injected millis mismatch caught." Line 93 now seeds `T*1000` / `T*1000−500` / `T*1000+1000`, asserts `compaction_reread_count == 1`, gate `(ts_millis ÷ 1000) > compacted_at` floor + strict `>`. Byte-for-byte aligned with `compaction_reckoning.md`. | PASS |
| (b) no "+500ms counts" / "== 2" / "count of 2" / "two after-reads" / "counts the two" residue anywhere in spec, acceptance-map, risk-strategy, all pseudocode, all test-plans | **Confirmed clean.** Repo-wide scan: the ONLY hits are in (1) this gate report's prior iterations and (2) agent rework reports under `agents/`, both quoting the old defect for provenance. ZERO residue in any source artifact (SPECIFICATION, ACCEPTANCE-MAP, RISK-TEST-STRATEGY, all `pseudocode/*`, all `test-plan/*`, BRIEF, ARCHITECTURE, ADRs, SCOPE). | PASS |
| (c) gate stayed floor + strict `>` (`ts_millis ÷ 1000 > compacted_at`); no drift to `>=` or rounding | **Confirmed.** Every gate reference across spec AC-22/AC-12, ACCEPTANCE-MAP, RISK-TEST R-08/coverage/edge, ADR-006, ARCHITECTURE §4, and all pseudocode/test-plan files is strict `>` over an integer-floor `÷1000`. `compaction_reckoning.md:118` explicitly forbids `>=` and rounding-for-floor. No drift. | PASS |
| (d) expected `compaction_reread_count == 1` everywhere; seeds exact-boundary / −500ms / +1s | **Confirmed.** Count = 1 in SPECIFICATION AC-22, ACCEPTANCE-MAP AC-22, RISK-TEST (lines 121/124/129/318), compaction_reckoning pseudocode (line 93) + test-plan (lines 33–37), OVERVIEW (lines 35/93). Seeds are uniformly exact-boundary (`T*1000`, NOT counted), −500ms (`T*1000−500` → floor `T−1`, NOT counted, floor-catching guard), +1s (`T*1000+1000` → floor `T+1`, counted). No +500ms seed survives. | PASS |
| (e) reload/store test-plan prose reads `round(fraction × 10000)` of a [0.0,1.0] fraction | **Confirmed.** `reload_overlap_engine.md` (lines 18, 19, 36) and `store_cycle_review.md` (line 19) all read `round(fraction × 10000)` against the live `-> f64` fraction; zero `percentage × 100` / "returns a percentage" residue. 37.5%→3750 numeric expectations intact. | PASS |

### Regression re-confirmation (previously-PASSED checks — all still hold)

| Check | Status | Evidence |
|-------|--------|----------|
| basis-points integer encoding | PASS | `test_basis_points_roundtrip` (store), `test_context_reload_pct_basis_points_encode` + `_rounding_to_nearest` (reload): 0.375→3750, INTEGER not REAL, round-to-nearest. Unchanged. |
| read-before-purge (AC-08) | PASS | activity_fold_landing ordering + inversion-zeroes-columns + review_pipeline INVARIANT B intact. |
| single-writer / no-clobber (AC-17, three #5022) | PASS | `test_exactly_one_store_cycle_review_writer_site` + three #5022 behavioral assertions intact. |
| structural leak gate (AC-19) | PASS | `test_candidates_structurally_absent_from_memoized_report` + `test_no_content_field_on_record` intact. |
| all 22 ACs homed | PASS | OVERVIEW §2 risk→AC table + §4.3 integration table map AC-01..AC-22; no AC orphaned by the edit. |
| Architecture / column set / gate semantics / producer contract | PASS | No structural change in the rework — only the AC-22 worked example was reconciled to the already-correct floor + strict-`>` gate. |

### Verdict

The single REWORKABLE-FAIL defect that drove iterations 0 and 1 — the self-contradictory AC-22 worked example — is now consistent across the entire feature. The canonical example (floor + strict `>`, seeds exact-boundary / −500ms / +1s, expected count = 1) is identical in spec, acceptance-map, risk-strategy, pseudocode, and ALL test plans including the previously-lagging `test-plan/OVERVIEW.md`. No new defects introduced. No regressions. **Gate 3a PASSES.** The feature may proceed to Stage 3b.

### Rework Required
None.

### Scope Concerns
None.

---

> --- RE-VALIDATION PASS (iteration 1 rework review) appended below; original iteration-0 report retained below the second divider ---

## RE-VALIDATION (iteration 1) — Result: REWORKABLE FAIL

**Focus**: confirm the AC-22 fix (floor + strict-`>`, expected count = 1) is complete and consistent, and the non-blocking basis-points prose hygiene item, with no regressions.

### Verdict per requested item

| Item | Finding | Status |
|------|---------|--------|
| (a) no remaining "+500ms counts" / "== 2" anywhere | FIXED in all five named target files. **NOT fixed in `test-plan/OVERVIEW.md`** (the test-plan index/routing doc) — lines 35 and 93 still encode "+500ms counts" and "counts the two after-reads" (= 2). | **FAIL** |
| (b) gate stayed floor + strict `>` (no drift to `>=`) | Confirmed. Every gate reference across all artifacts is strict `>`; pseudocode explicitly forbids `>=` and rounding-for-floor. No `>=` drift. | PASS |
| (c) expected count = 1 everywhere | True in all five named files (SPECIFICATION AC-22, ACCEPTANCE-MAP AC-22, RISK-TEST R-08/edge/coverage, compaction_reckoning pseudocode + test plan). OVERVIEW line 93 implies 2. | PASS (named files) / FAIL (OVERVIEW) |
| (d) reload + store test-plan prose now `round(fraction × 10000)` | FIXED. `test-plan/reload_overlap_engine.md` (lines 18, 36) and `test-plan/store_cycle_review.md` (line 19) now read `round(fraction × 10000)` of a [0.0,1.0] fraction; zero `percentage × 100` / "returns a percentage" prose remains in either file. | PASS |

### Regression re-confirmation (previously-PASSED checks)

| Check | Status | Evidence |
|-------|--------|----------|
| basis-points integer encoding | PASS | `test_basis_points_roundtrip` (store), `test_context_reload_pct_basis_points_encode` + `_no_float_column` (reload); 0.375→3750, INTEGER not REAL. No edit regressed it. |
| read-before-purge (AC-08) | PASS | activity_fold_landing + review_pipeline ordering + inversion-zeroes-columns intact. |
| single-writer / no-clobber (AC-17, three #5022) | PASS | `test_exactly_one_store_cycle_review_writer_site` + three #5022 behavioral assertions intact. |
| structural leak gate (AC-19) | PASS | `test_candidates_structurally_absent_from_memoized_report` + `test_no_content_field_on_record` intact. |
| all 22 ACs have a test-plan home | PASS | OVERVIEW §2 risk→AC table + §4.3 integration table still map AC-01..AC-22. |

### The one remaining defect (drives the gate)

**`test-plan/OVERVIEW.md` was not updated during the rework** and still carries the iteration-0 contradiction — the same defect class, relocated to the test-plan index:

- **Line 35** (R-08 summary row): "`+500ms/+1s counts, −500ms does not`" — asserts the +500ms read COUNTS. Under the (now-canonical) floor + strict-`>` gate, `ts_millis = T*1000+500` floors to `T`, and `T > T` is false → it does **NOT** count.
- **Line 93** (`test_cycle_review_compaction_reread_seconds_boundary`, the marquee mandated integration test): seeds reads at `+500ms`, `+1s`, `−500ms` and asserts "`compaction_reread_count` **counts the two after-reads**, NOT the before-read." "The two after-reads" = +500ms and +1s = a count of **2** — the exact original defect, now without the literal `== 2` string. This directly contradicts the corrected `test-plan/compaction_reckoning.md` (seeds exact-boundary / −500ms / +1s, **asserts count == 1**, does not seed +500ms).

**Why this still blocks**: OVERVIEW.md is the routing/index doc the tester reads in Stage 3a/3c to know which integration tests to author and what they assert. A tester following line 93 will write `compaction_reread_count == 2` with a +500ms read counting — exactly the believable-wrong-number this feature's seconds-normalization contract exists to kill, and the precise failure that drove iteration 1. It is now self-contradictory against `compaction_reckoning.md` (count = 1). This is a narrow two-line fix, not a semantics change.

### Rework Required (iteration 1 → 2)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| `test-plan/OVERVIEW.md` line 35 (R-08 row) asserts "+500ms counts"; line 93 (the `test_cycle_review_compaction_reread_seconds_boundary` mandated integration test) seeds +500ms and asserts "counts the two after-reads" (= 2) — contradicting the reconciled count = 1 in `compaction_reckoning.md`. | uni-tester (test-plan OVERVIEW owner) | Align both lines to the canonical floor + strict-`>` example already in `compaction_reckoning.md`: seed exact-boundary (`T*1000`, NOT counted), `−500ms` (`T*1000−500`, floors to `T−1`, NOT counted, the floor-catching guard), `+1s` (`T*1000+1000`, floors to `T+1`, counted); assert `compaction_reread_count == 1`. Line 35 summary → "+1s counts, −500ms/exact do not (floor + strict-`>`); injected millis mismatch caught." Do NOT seed +500ms and do NOT assert a count of 2. |

### Scope Concerns
None. Same design-internal consistency defect, now isolated to one un-reworked file. Architecture, column set, gate semantics, and all other artifacts are sound. The feature can proceed once OVERVIEW.md lines 35/93 match the already-correct `compaction_reckoning.md`.

---

## ORIGINAL ITERATION-0 REPORT (retained for provenance)

> Gate: 3a (Component Design Review)
> Date: 2026-06-16
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | 9 components map 1:1 to ARCHITECTURE §2; seams, crate locations, ADR refs all match. |
| Specification coverage | PASS | All 23 FR + 8 NFR have pseudocode homes; no scope additions. |
| Risk coverage (test plans) | WARN | All 18 R-XX and 22 AC have a test-plan home, BUT the AC-22 worked example is encoded with a self-contradictory boundary expectation (see below). |
| Interface consistency | WARN | One unresolved consistency item: AC-22 boundary semantics contradict between pseudocode (count=1) and test plan / spec (count=2). |
| basis-points resolution (specific) | PASS | Pseudocode's `round(fraction × 10000)` is CORRECT against live `session_metrics.rs:104` (returns a fraction, not a percentage). Round-trip test guards it. |
| Every metric column INTEGER | PASS | 15 INTEGER + 1 TEXT; no f64/REAL; no `is_finite()` AC. |
| compaction_reread seconds-normalization | FAIL | Normalization (`÷1000` floor) + strict-`>` gate is correctly designed, but the AC-22 +500ms expected value contradicts it (REWORKABLE). |
| read-before-purge (AC-08 inversion) | PASS | Inversion test present and load-bearing. |
| single-writer / no-clobber (AC-17, three #5022) | PASS | Three #5022 assertions + structural single-writer all present. |
| structural leak gate (AC-19, no content field) | PASS | No content field; metadata-only consumed surfaces asserted. |
| Stewardship compliance | PASS | Both design-phase agents have stewardship blocks with Queried + reasoned Declined. |

**Net**: One REWORKABLE FAIL (the AC-22 / compaction-gate boundary contradiction). Everything else passes. The specific load-bearing basis-points item flagged in the spawn prompt is resolved correctly.

---

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS
**Evidence**: OVERVIEW.md "Components" table maps all 9 components to the ARCHITECTURE §2 Component Breakdown with matching seams (`cycle_review_index.rs`, `migration.rs`, `db.rs`, `session_metrics.rs`, `cycle_aggregates.rs` new module, `tools.rs` pipeline). Crate assignments match (unimatrix-store / unimatrix-observe / unimatrix-server). ADR references in every component file correspond to the architecture ADR index (ADR-001..010, Unimatrix #5037/#5039/#5042/#5044/#5045/#5046/#5047/#5048/#5051). Verified codebase anchors (OVERVIEW lines 13–29) checked against live code: `SUMMARY_SCHEMA_VERSION = 4` (cycle_review_index.rs:49 ✓), `CURRENT_SCHEMA_VERSION = 29` (migration.rs:24 ✓) — so crt-055's v29→v30 migration and 4→5 bump are correctly anchored. Single-writer / four-return discipline preserved (no new write site). crt-054 producer surfaces consumed read-only as architecture §9 dictates.

### Check 2 — Specification coverage
**Status**: PASS
**Evidence**: Every FR maps to pseudocode:
- FR-01/02 (fail-loud) → fail_loud_guard.md (per-metric `MetricAvailability`, render branch).
- FR-03/04 (migration + version bump) → cycle_review_index_schema.md (three-path bump, pragma-guarded ALTERs).
- FR-05/06/07/08 (rank 1/2/3, #556, #320) → aggregate_reckoning.md (num/den pairs, union dedup).
- FR-09..FR-12 (fold surfacing, read-before-purge, bytes-only, width) → activity_fold_landing.md.
- FR-13/14/14b/15 (compaction count + reread + clock/unit + boundary) → compaction_reckoning.md.
- FR-16/17 (dual reload) → reload_overlap_engine.md.
- FR-18/19 (auto_close) → auto_close.md.
- FR-20 (#206-4 knowledge-that-helped) → review_pipeline.md (response-time, no column, ADR-009).
- FR-21/22/23 (single-writer / no-clobber / guarded recompute) → store_cycle_review.md.
NFR-01..08 addressed (leak gate, content opacity, migration hygiene, forward-compat JSON, informs-never-controls). No scope additions found — no token field, no orchestration surface, #206-4 kept response-only.

### Check 3 — Risk coverage (test plans)
**Status**: WARN (see Check 7 for the FAIL that drives the gate)
**Evidence**: test-plan/OVERVIEW.md §2 maps all R-01..R-18 to a test-plan file and a primary AC. Critical risks (R-01..R-06, R-08) carry the load-bearing negative/inversion/regression assertions:
- R-01/R-02 → the three #5022 assertions + structural single-writer (store_cycle_review, review_pipeline).
- R-03 → read-before-purge ordering + inversion-zeroes-columns (activity_fold_landing, review_pipeline).
- R-04 → held-route non-empty-fold regression guard (activity_fold_landing).
- R-05 → declared-only attribution + evicted-no-fabricated-zero (#4140) (compaction_reckoning).
- R-06 → per-metric "unavailable" + behavioral-signal coarse/directional (fail_loud_guard).
- R-08 → AC-22 mandated MCP integration test (compaction_reckoning).
All 22 ACs (AC-01..AC-22) have a test-plan home (OVERVIEW §2 + §4.3 new-integration-tests table). **However** the AC-22 test encodes a boundary expectation that is internally inconsistent — see Check 7.

### Check 4 — Interface consistency
**Status**: WARN
**Evidence**: Shared types defined once in OVERVIEW.md and consistent across files: `CycleReviewRecord` 16 new fields (schema → store_cycle_review bind order ?13..?28 identical), `CycleAggregates` bundle (Components 3/4/5/6 populate, Component 2 reads), `MetricAvailability` (Component 7, presentation-only, not persisted — consistent everywhere), `ReloadWindow` enum (Component 4), catalog index contract (`class_counts[0]=error`, `[1]=refusal`). INSERT/UPDATE/SELECT column order is pinned identically across schema and store files. Data-flow boundary types match.
**Issue**: the AC-22 boundary semantics are inconsistent across artifacts — pseudocode reasons the expected reread count is 1, the test plan and spec assert 2 (see Check 7). This is the one cross-file contradiction.

### Check (specific, spawn-prompt) — basis-points resolution
**Status**: PASS — CONFIRMED CORRECT against live code (ground truth)
**Evidence**:
- (a) The live function is ground truth. `crates/unimatrix-observe/src/session_metrics.rs:47-105` returns `reload_files as f64 / total_files_in_subsequent as f64` — a **fraction in [0.0, 1.0]** (line 104), with `0.0` for single-session/empty. It does NOT return a 0–100 percentage. The doc comment (line 43) confirms "fraction".
- The pseudocode (reload_overlap_engine.md §4b, OVERVIEW lines 30–31) correctly resolves the ADR-005/brief wording discrepancy: ADR-005 (#5047) and the brief phrase the source as "a percentage" with `round(pct × 100)`; the pseudocode binds `round(fraction × 10000)` (0.375 → 3750). Arithmetically `pct × 100` (of a 0–100 percentage) and `fraction × 10000` (of a 0–1 fraction) are identical for the same physical overlap; the pseudocode binds the **fraction** form, which matches the live function. This is the correct binding.
- (b) The round-trip test guards it: reload_overlap_engine test plan `test_context_reload_pct_basis_points_encode` (37.5% → 3750), `test_context_reload_pct_rounding_to_nearest` (0.005%→1, 99.995%→10000), and store_cycle_review `test_basis_points_roundtrip` all pin 37.5% → 3750.
- One residual cosmetic inconsistency (WARN, not blocking): the reload_overlap_engine and store_cycle_review **test plans** still describe the encode as `round(percentage × 100)` / "compute_context_reload_pct returns a percentage" (test-plan/reload_overlap_engine.md lines 18–19, 36; store_cycle_review.md line 19), echoing the stale ADR wording rather than the pseudocode's corrected `× 10000`-of-a-fraction. The numeric expectations (37.5%→3750) are correct either way, so this does not change a single asserted value — but the prose should be corrected at impl time so the tester writes the multiplier that matches the live `-> f64` fraction. Pseudocode binds the correct one; flagging for prose hygiene only.

### Check — Every metric column INTEGER (no f64/REAL, no is_finite AC)
**Status**: PASS
**Evidence**: cycle_review_index_schema.md §1b lists 15 `i64` fields + 1 `String` (`signal_class_counts_json`); migration §1f ALTERs 15× `INTEGER NOT NULL DEFAULT 0` + 1× `TEXT NOT NULL DEFAULT '{}'`. `context_reload_pct` is `i64` basis points. No `f64`/REAL column. store_cycle_review test plan `test_no_float_reaches_bind` and reload `test_context_reload_pct_no_float_column` + schema `test_context_reload_pct_column_is_integer_not_real` assert it structurally. No `is_finite()` AC anywhere (correctly designed out per the human decision; matches AC-14/AC-20 which drop the float guard).

### Check — compaction_reread gate: ÷1000 before seconds gate (AC-22)
**Status**: FAIL (REWORKABLE)
**Evidence**: The normalization design is correct: compaction_reckoning.md §5c gates `read_ts_secs = r.ts / 1000` (integer floor, `session_metrics.rs:115` convention) `> boundary_secs`, normalizing the read side only, boundary untouched (seconds, producer contract). This is the right design and matches FR-14b / Constraint 9 / ADR-006.
**Issue — internal contradiction in the AC-22 worked example (the mandated marquee test)**:
- **Pseudocode** (compaction_reckoning.md line 76) reasons, correctly, under floor + strict-`>`:
  `compacted_at = T` (s), read at `ts_millis = T*1000 + 500` → `read_ts_secs = (T*1000+500)/1000 = T` → gate `T > T` is **false → NOT counted**. It concludes "+1000ms → counted; +500ms and −500ms → NOT counted."
- **Test plan** (compaction_reckoning test plan lines 33–37) and **SPECIFICATION AC-22** (SPECIFICATION.md:141, ACCEPTANCE-MAP AC-22) assert: "+500ms after → MUST count; +1s → MUST count; −500ms → MUST NOT count" and **`compaction_reread_count == 2`**.
- These cannot both be true. A read +500ms after a compaction at integer second T floors back to T, and a CORRECT floor+strict-`>` implementation counts it as 0, giving a reread count of **1**, not 2.
- The spec's stated rationale for choosing the sub-second boundary ("a ±1s window would pass even if the floor were absent/wrong") is itself defeated: the +500ms case it relies on to catch a broken floor is, under the agreed floor+strict-`>` semantics, NOT counted by a correct implementation. The boundary that distinguishes "floor present" from "floor absent" with strict-`>` is actually the **−500ms vs +1000ms** pair (or a `>=` gate), not +500ms-counts.
- **Why this blocks**: if the tester writes AC-22 to the spec's `== 2`, the mandated integration test will (a) fail against the correct pseudocode implementation, or (b) pressure the implementer to change the gate to `>=` or to round-half-up instead of floor — silently altering the binding seconds-normalization contract this feature exists to protect. The pseudocode author noticed the tension ("Tester confirms the exact ±boundary expectation against the strict `>` and floor") but did not reconcile it; the contradiction must be resolved at design level before code.

### Check — read-before-purge ordering (AC-08 inversion)
**Status**: PASS
**Evidence**: activity_fold_landing.md §6 and review_pipeline.md INVARIANT B pin the fold read strictly before every `purge_cycle_transcripts` site. Test plan has `test_read_before_purge_ordering` (precedence assertion) AND `test_inverted_order_zeroes_columns` (the load-bearing inversion that zeroes columns) plus an end-to-end harness non-zero check.

### Check — single-writer / no-clobber (AC-17, three #5022 assertions)
**Status**: PASS
**Evidence**: store_cycle_review.md §2d enumerates the four returns; only RETURN 4 (full pipeline) writes the new columns; guarded-recompute clears the memo and falls through to RETURN 4 (no second writer near the memo/`check_stored_review` site). The three #5022 assertions are present (store_cycle_review test plan + review_pipeline integration): (a) stale+present recomputes fresh non-zero at v5; (b) stale+purged retain byte-identical, no write; (c) force+purged no-clobber. Plus structural `test_exactly_one_store_cycle_review_writer_site`.

### Check — structural leak gate (AC-19, no content field)
**Status**: PASS
**Evidence**: cycle_review_index_schema.md §1b adds only `i64`/`String`-aggregate fields, no content field. store_cycle_review test plan keeps `test_candidates_structurally_absent_from_memoized_report` and adds `test_no_content_field_on_record`. activity_fold_landing asserts the consumed `ActivitySnapshot` is metadata-only (no `Display`/content on the persist path). `signal_class_counts_json` is a count map, serialized via serde (no string concat), not content bytes.

### Check — Knowledge stewardship compliance
**Status**: PASS
**Evidence**: Design-phase active-storage agents already stored ADRs (architect — ADR-001..010 in Unimatrix, referenced by id throughout). Read-only design agents have `## Knowledge Stewardship` blocks with `Queried:` and reasoned no-store: SPECIFICATION.md (lines 229–230, Queried context_briefing, read-only tier no storage with reason), RISK-TEST-STRATEGY.md (lines 314–316, Queried context_search/get, "nothing novel to store -- {reason: already captured as #5022/#4153/#4140/#4178}"), test-plan/OVERVIEW.md (lines 134–136, Queried context_briefing/search/get, deferred-with-reason), pseudocode/OVERVIEW.md cites verified anchors from queries. All blocks present with reasons. No missing-block fails.

---

## Rework Required (REWORKABLE FAIL)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| AC-22 worked-example contradiction: under the agreed floor (`ts_millis ÷ 1000`) + strict-`>` gate, a read +500ms after `compacted_at=T` floors to T and is NOT counted, so the correct reread count is 1, not the spec's/test-plan's 2. The pseudocode (line 76) reasons it to 1; the test plan and SPECIFICATION AC-22 assert 2. | uni-risk-strategist / uni-specification (owners of AC-22 + RISK-TEST §R-08) and uni-tester (test plan) — coordinated by the SM | Pick ONE coherent boundary semantics and propagate it to SPECIFICATION AC-22, ACCEPTANCE-MAP AC-22, RISK-TEST §R-08 edge cases, and compaction_reckoning pseudocode+test plan. Recommended: keep floor + strict-`>` (the pseudocode's correct reading) and **re-state the worked example so the "counts" case actually clears the gate** — e.g. `compacted_at=T`; read at `T*1000+1000` (+1s, floors to T+1 → counts) and read at `T*1000−500` (−500ms, floors to T−1 → not counted), expected reread count = 1; and add the unit-mismatch guard (unnormalized millis ts must not flip to all-or-nothing). Preserve the #4236 intent (a boundary that fails if the floor is absent) by using the −500ms-floors-to-T−1 case, which a millis-unnormalized gate would wrongly count. If instead the team wants +500ms-after to count, the gate must change to `>=` against the floored second (a real semantics change) — document that explicitly; do not let the test silently force it. |
| (Prose hygiene, non-blocking — fold into the same rework pass) reload_overlap_engine + store_cycle_review TEST PLANS still describe `compute_context_reload_pct` as "returns a percentage" with `round(percentage × 100)`, echoing stale ADR-005 wording rather than the pseudocode's corrected `round(fraction × 10000)` (the live function returns a fraction). | uni-tester | Correct the multiplier prose in the two test plans to match the live `-> f64` fraction and the pseudocode (`× 10000` of a fraction). All numeric expectations (37.5%→3750) already correct — no asserted value changes. |

## Scope Concerns

None. This is a design-internal consistency defect, not a scope/technology/architecture limitation. The architecture, column set, single-writer discipline, leak gate, basis-points resolution, and producer contract are all sound. The feature can proceed once the AC-22 boundary worked-example is made self-consistent with the (correct) floor + strict-`>` gate.
