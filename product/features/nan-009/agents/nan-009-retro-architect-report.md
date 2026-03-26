# nan-009 Retrospective: Architect Report

Agent ID: nan-009-retro-architect
Unimatrix agent_id: uni-architect
Feature: nan-009 (Phase-Stratified Eval Scenarios)
Mode: retrospective

---

## 1. Patterns

### 1a. Serde Null Suppression Asymmetry (3-site annotation rules)

**Assessment:** ADR-specific, already captured. ADR-001 (#3562) is the authoritative record for nan-009's three-site serde annotation rules. Pattern #3255 covers the general `serde(default)` + `skip_serializing_if` principle. Pattern #3557 covers the dual-direction test requirement. No standalone pattern entry needed.

However, the **dual-type constraint pattern** (#3550) did not previously document the passthrough-metadata variant (non-metric fields with asymmetric serde across ScenarioContext, runner ScenarioResult, and report ScenarioResult). Updated via `context_correct`.

**Action:** Updated #3550 → #3574. The updated entry documents the passthrough-metadata variant alongside the existing metric-field variant, including the three-site serde asymmetry and its rationale.

### 1b. Dual-Type Pipeline Pattern

**Assessment:** #3550 (now #3574) covers the general dual-type constraint. nan-009 confirmed the pattern holds for passthrough metadata fields, not just metric fields. The correction adds this variant explicitly.

**Action:** Updated #3550 → #3574 (see above).

### 1c. "(unset)" Sort-Last Override

**Assessment:** New reusable pattern. No prior entry existed for the technique of forcing a parenthesized null-bucket label to sort last when `(` (ASCII 40) precedes `a-z` in standard ordering. This is applicable to any future feature that groups by `Option<String>` with a display label for the null bucket.

**Action:** Stored new pattern #3575. Title: `"(unset)" sort-last override for Option<String> group keys with ASCII-ordered sort`.

### 1d. compute_phase_stats Grouping Pattern

**Assessment:** New reusable pattern. The combination of (1) all-None early-exit guard that suppresses the report section entirely, (2) `HashMap<Option<String>, Vec<_>>` grouping, (3) `unwrap_or_else(|| "(unset)")` label assignment, and (4) custom sort override is a complete idiom for Option<String> stratification. Applicable to any future eval report stratification feature (e.g., profile × phase cross-product, retrieval_mode stratification).

**Action:** Stored new pattern #3576. Title: `compute_phase_stats grouping pattern: Option<String> stratification with empty-vec guard and sort override`.

### Summary

| Component | Action | Entry ID |
|-----------|--------|----------|
| Serde 3-site asymmetry | ADR already stored (#3562); general principle in #3255; no new pattern | — |
| Dual-type pipeline (passthrough metadata variant) | Updated existing pattern | #3574 (was #3550) |
| "(unset)" sort-last override | New pattern | #3575 |
| compute_phase_stats grouping | New pattern | #3576 |

---

## 2. Procedures

### File-split trigger procedure (#3568)

The existing pattern #3568 ("Splitting flat Rust test files: declare sibling modules in mod.rs") was stored during nan-009 delivery and covers the correct splitting technique. Content is accurate — the nan-009 rework agent followed it and produced the correct module structure (`#[cfg(test)] mod tests_phase;` in mod.rs, `use super::...` in tests_phase.rs).

**Assessment:** No update needed. #3568 is accurate and complete.

### "(unset)" sort procedure

Covered by new pattern #3575 which includes the full Rust implementation. No separate procedure entry needed.

### Build/test/integration process

Unchanged from prior features. No new procedure entries warranted.

---

## 3. ADR Validation

### ADR-001 (#3562): Serde Null Suppression — 3-site annotation rules

**Status: VALIDATED**

Gate 3b confirmed all three annotation sites were correctly implemented:
- `types.rs` ScenarioContext.phase: `#[serde(default, skip_serializing_if = "Option::is_none")]` — confirmed at lines 73–74.
- `runner/output.rs` ScenarioResult.phase: `#[serde(default)]` only, no `skip_serializing_if` — confirmed at lines 86–87.
- `report/mod.rs` ScenarioResult.phase: `#[serde(default)]` only — confirmed at lines 122–123.

Tests `test_scenario_result_phase_null_serialized_as_null` and `test_scenario_context_phase_null_absent_from_jsonl` exercise the two directions explicitly and pass. No gaps found. ADR content accurately describes what was implemented.

### ADR-002 (#3563): Round-Trip Integration Test as Dual-Type Guard

**Status: VALIDATED**

The mandatory test `test_report_round_trip_phase_section_7_distribution` was absent at gate 3b (reworkable fail) but was implemented during the rework wave and confirmed passing at gate 3c. Gate 3c evidence confirms all five mandatory assertions are present: section 6 present, "delivery" in section 6, section 7 present, `pos("## 6.") < pos("## 7.")`, `!content.contains("## 6. Distribution Analysis")`. The ADR accurately predicted the dual-type risk; the round-trip test did guard against partial updates. The fact that it was initially missing (and therefore a gate failure) validates the ADR's claim that this test is mandatory — its absence is a gate-blocking gap.

One note: the ADR was stored (by the delivery leader, not the architect) before implementation. The test being omitted in the first wave and added in the rework wave means the ADR served exactly the purpose it was written for.

### ADR-003 (#3565): Phase Is a Soft Vocabulary Key

**Status: VALIDATED**

Implementation confirmed: `query_log.phase` is `TEXT` with no CHECK constraint, free-form strings accepted. Known vocabulary (`design`, `delivery`, `bugfix`) documented in `docs/testing/eval-harness.md` as a snapshot, not an allowlist (AC-07 passed). The `"(unset)"` canonical label is used in exactly one place (`aggregate.rs` line 447: `key.unwrap_or_else(|| "(unset)".to_string())`). No `"(none)"` found anywhere in code, tests, or documentation. Sort-last override implemented as specified. Gate 3c: PASS on all phase-vocabulary related tests.

No gaps found. ADR content is accurate.

---

## 4. Lessons

### 4a. Gate 3a — Missing Knowledge Stewardship blocks

**Prior entry:** #3542 (col-022, col-028 — two instances).

**nan-009 adds a third instance.** The architect report and synthesizer report were both missing `## Knowledge Stewardship` blocks. Notably, the architect report documented a failed store attempt in its body text but had no formal stewardship section. The section was treated as optional when a store attempt failed rather than written with "Stored: nothing stored — agent lacked Write capability."

**Action:** Updated #3542 → #3577. Entry now spans three features (col-022, col-028, nan-009), includes synthesizer as an affected agent type, and documents the specific failure mode of omitting the section when an inline store attempt fails.

### 4b. Gate 3b — Entire mandatory test modules absent

**Prior entry:** #3386 covers "happy-path tests implemented, edge-case tests skipped." This is a different failure mode.

**nan-009 failure mode:** Implementation agent delivered correct production code but zero phase-specific tests. The entire `tests_phase` and `tests_phase_pipeline` modules did not exist. This is not "edge cases skipped" — it is "no tests at all for the feature's primary new functionality." 14 tests and 2 new test modules had to be created in the rework wave.

**Action:** Stored new lesson #3579. Title: `Gate 3b: implementation wave delivers production code but zero mandatory tests — entire test modules absent`.

### 4c. Gate 3b — File size violations discovered at gate

**Prior entries:** #3568 covers the splitting technique. #161 is the 500-line convention. Neither addresses the failure mode of discovering violations at gate time rather than proactively.

**nan-009 specifics:** `render.rs` = 544 lines (acknowledged in agent report but not fixed), `tests.rs` = 1054 lines (not checked by agent). The rework wave had to split tests.rs into 5 modules simultaneously with adding 14 missing tests.

**Action:** Stored new lesson #3580. Title: `Gate 3b: file size violations discovered at gate — 500-line limit not self-checked by implementation agent`.

---

## 5. Retrospective Findings

### Hotspot: compile_cycles (81 cycles)

**Prior entries:** #3544 (col-028, 168 cycles) and #3439 (col-026/bugfix-236) both cover the compile-batching principle. The nan-009 recommendation ("batch struct field additions before compiling") is already fully documented.

**Assessment:** 81 cycles for a 5-component feature with dual-type additions across runner and report modules is lower than prior hotspots but still above baseline. The existing lessons (#3544, #3439) already prescribe the correct approach. No new entry needed. The retrospective recommendation aligns with what is already documented.

**Action:** No new entry. Existing lessons #3544 and #3439 cover this.

### Hotspot: tool_failure_hotspot (context_store 19x, context_search 9x)

**Prior entry:** #3387 covers this recurrence pattern (nan-004, col-024). nan-009 adds a third instance and reveals a new dimension: `context_search` failures (Read capability missing) in addition to `context_store` failures (Write capability missing).

**Action:** Updated #3387 → #3578. Entry now spans three features (nan-004, col-024, nan-009) and documents the search-failure dimension as a new variant (both Read and Write blocked, not just Write).

### Hotspot: cold_restart (43-min gap, 46 re-reads)

**Prior entry:** #1271 explicitly normalizes this pattern: "46 re-reads after a gap" and "context_load and cold_restart hotspots scale with component count." The nan-009 figures (43-min gap, 46 re-reads, 5 components) are within the normalized range documented there.

**Assessment:** Not actionable. The existing lesson already covers this case. No new entry needed.

**Action:** None.

### Hotspot: context_load (157KB before first write)

**Prior entry:** #1271 normalizes heavy pre-read for multi-component features ("251KB for a 5-component feature is expected"). 157KB is below that threshold.

**Assessment:** Not actionable.

**Action:** None.

### Baseline outlier: knowledge_entries_stored (26 vs mean 7.6)

26 entries stored is 2.3× above the 1-sigma upper bound (mean 7.6, σ 2.0). This reflects the design-heavy nature of nan-009 (5 pipeline components, full artifact set, 3 ADRs). Not a problem — confirmed as NewSignal/positive in the retrospective summary.

**Assessment:** Informational. No action.

---

## Knowledge Stewardship

Queried:
- `context_search` for "serde null suppression skip_serializing_if option passthrough" → found #3449, #885, #3255, #3562, #3557
- `context_search` for "eval report section rendering markdown" → found #3449, #3569, #3426, #3566, #949
- `context_search` for "eval scenarios SQL extraction query_log fields" → found #3555, #361, #2806
- `context_search` for "dual type pipeline independent struct copies JSON file boundary" → found #320, #1161, #343, #1103
- `context_search` for "sort last special case null bucket string ordering override" → no relevant prior entries
- `context_search` for "group by optional string aggregate stats empty vec guard" → no relevant prior entries
- `context_search` for "knowledge stewardship agent report required sections gate failure" → found #3542, #1267
- `context_search` for "missing tests gate fail implementation wave deferred testing" → found #3386, #3548
- `context_search` for "file size limit 500 lines split module extract" → found #3568, #161
- `context_search` for "compile cycles cascading type errors iterative field additions struct" → found #3544, #3439
- `context_search` for "architect agent Write capability MCP -32003 ADR storage blocked" → found #3387, #1206
- `context_search` for "context loss cold restart long session re-read files orientation" → found #324, #1271
- `context_lookup` for #3542, #3387, #3544, #3550, #3562, #3563, #3565, #3568, #3255, #3555, #3557, #3386, #1271, #3439

Stored:
- Updated dual-type constraint pattern: #3550 → #3574 (passthrough metadata serde asymmetry variant added)
- New pattern #3575: "(unset)" sort-last override for Option<String> group keys
- New pattern #3576: compute_phase_stats grouping pattern with empty-vec guard and sort override
- Updated stewardship lesson: #3542 → #3577 (nan-009 as third instance; synthesizer added as affected agent type)
- Updated MCP permission lesson: #3387 → #3578 (nan-009 as third recurrence; Read capability failure added)
- New lesson #3579: Gate 3b — entire mandatory test modules absent (distinct from #3386 edge-case omission)
- New lesson #3580: Gate 3b — file size violations discovered at gate rather than proactively
