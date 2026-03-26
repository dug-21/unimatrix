# Retrospective Architect Report: bugfix-408

> Agent ID: bugfix-408-retro-architect
> Mode: retrospective
> Date: 2026-03-26

## 1. Patterns

**Query:** `co-access subsystem patterns maintenance cleanup staleness`

No new pattern was established by this fix. A single-constant change does not introduce reusable structure. The only pattern result relevant to this domain (#3553) is a lesson, not a structural pattern.

- New entries: none
- Updated entries: none
- Skipped: this was a one-constant fix; no recurring structural pattern emerged. Judgment applied per instructions.

## 2. Procedures

**Finding:** The tester used `sleep` workarounds in 2 instances during the testing phase. The run_in_background+TaskOutput technique was not found in the procedure category for this specific use case.

- **Stored #3561**: "Avoid sleep polling in tester agents: use run_in_background + TaskOutput" — procedure
  - Covers: when to use run_in_background vs sleep, why sleep polling is the anti-pattern, what constitutes an acceptable short sync wait.

The compile_cycles recommendation ("batch field additions before compiling") was NOT stored. The retrospective system misclassified the 28 compile cycles: they were full test-suite runs by the tester during a 50-minute testing phase, not iterative per-field struct changes by the rust-dev agent. Storing the recommendation would produce misleading guidance.

## 3. ADR Status

### ADR-002: Two-Mechanism Co-Access Architecture (crt-013) — entry #702

**Status: VALIDATED, no changes needed.**

This ADR governs the two surviving CO_ACCESS consumers (MicroLoRA adaptation + scalar boost). The bugfix-408 change is to the staleness constant that governs retention and cleanup — it does not affect which mechanisms consume CO_ACCESS data, the boost formula, the +0.03 cap, or the transitional framing. The ADR remains accurate.

### ADR-002: Maintenance Opt-Out on context_status (crt-005) — entry #178

**Status: PARTIALLY STALE — correction stored.**

The ADR states that co-access cleanup is gated on `maintain=true`. This was accurate at crt-005 ship time. bugfix/252 removed that gate: `run_maintenance` (and with it `cleanup_stale_co_access`) now runs unconditionally every background tick. The investigator's discovery of this discrepancy was the critical finding of bugfix-408.

Action taken:
- Entry #178 left active (the confidence refresh and HNSW compaction gating it describes remains accurate).
- **Stored #3559**: "ADR-002 correction: co-access cleanup is unconditional since bugfix/252 (supersedes #178)" — decision entry tagged `supersedes:#178`, with a corrected operations table distinguishing what is and isn't gated on `maintain=true`.

## 4. Lessons

### Entry #3553 — Stewardship review

**Status: RETAIN as-is. Quality is good.**

Content assessment:
- What happened: yes — 30-day constant causes silent deletion of co-access signal during dormant feature cycles.
- Root cause: yes — constant gates both FILTER and DELETE; cleanup runs unconditionally (maintain=true gate was removed in bugfix/252).
- Takeaway: yes and actionable — 365 days is the safe stopgap; long-term fix is feature-cycle-aware retention policy (#403); no schema migration needed (cutoff computed at call time).

No deprecation needed.

### Entry #3558 — Stewardship review

**Status: DEPRECATED.**

Content was a raw machine-generated telemetry dump: file paths, cluster counts, hotspot signal strings, and one misapplied recommendation (compile_cycles for iterative dev applied to full test-suite runs). Not an actionable lesson. No generalizable knowledge.

- **Deprecated #3558** with reason: machine-generated session telemetry, not an actionable lesson; contains misapplied compile_cycles recommendation; superseded by targeted lessons stored separately.

### New lesson: call-site assumption verification

**Stored #3560**: "Verify call-site assumptions before diagnosing from bug reports: gates can be silently removed"

The critical investigator finding — the bug report's framing (cleanup gated on maintain=true) was stale because bugfix/252 removed the gate without updating the ADR — generalizes beyond co-access. The lesson covers: why call-site assumptions go stale, the verification procedure (find all call sites, check git log on callers, correct the diagnosis before proposing a fix), and a co-access-specific pointer to status.rs.

Not already stored: searches for "verify call-site assumptions" and "maintain=true gate removed" returned no matching lessons.

## 5. Retrospective Findings

### Hotspot analysis

| Hotspot | Classification | Action |
|---------|---------------|--------|
| compile_cycles: 28 | Expected — full test-suite runs by tester, not iterative dev. Recommendation does not apply. | No lesson stored. |
| file_breadth: 21 files | Expected — discovery phase exploring code paths across coaccess.rs, search.rs, status.rs. | No action. |
| tool_failure_hotspot: context_search failed 4 times | Transient MCP failures at session start. Pre-existing infrastructure noise. | No lesson stored (not a new pattern). |
| reread_rate: 19 files re-read | Expected for discovery phase. | No action. |
| search_via_bash: 8.2% | Below threshold concern; Grep/Glob used correctly per baseline outliers. | No action. |
| output_parsing_struggle | cargo output piped through multiple filters. Expected for test result parsing. | No action. |
| sleep_workarounds: 2 | Actionable — run_in_background pattern not followed by tester. | **Stored #3561** (procedure). |

### Baseline outliers (all positive)

All six positive outliers (zero permission friction, low bash-search count, zero SM respawn, context loaded within budget, clean stop after gate, sleep count below mean) confirm this was a well-executed, clean single-pass delivery. No corrective action needed. Notable: zero gate failures and zero rework commits.

### Recommendation actions

| Recommendation | Action taken |
|----------------|-------------|
| sleep_workarounds: use run_in_background | **Stored #3561** as procedure |
| compile_cycles: batch field additions | Not stored — misapplied to this session's full-suite test runs |

## Knowledge Stewardship Summary

| Entry | Action | Reason |
|-------|--------|--------|
| #3553 | Retained | Good quality lesson; no changes needed |
| #3558 | Deprecated | Raw telemetry dump; misapplied recommendation |
| #702 | Validated | Still accurate; fix did not affect two-mechanism architecture |
| #178 | Left active + correction stored | Partially stale on co-access cleanup gating; confidence refresh / compaction gating still accurate |
| #3559 (new) | Stored | ADR-002 correction: co-access cleanup unconditional since bugfix/252 |
| #3560 (new) | Stored | Lesson: verify call-site assumptions when diagnosing from bug reports |
| #3561 (new) | Stored | Procedure: use run_in_background instead of sleep in tester agents |
