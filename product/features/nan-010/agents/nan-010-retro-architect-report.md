# nan-010 Retrospective — Architect Report

> Agent: nan-010-retro-architect (uni-architect)
> Feature: nan-010 — Distribution Gate for eval profiles
> Date: 2026-03-27
> Mode: retrospective (not design)

---

## 1. Patterns

### 1.1 New Entries

**#3610 — Eval Harness Extension: 7-Component Decomposition for Per-Profile Gate Features** (new)

The 7-component architecture validated by nan-010 is generic enough to store. It maps directly onto the eval harness structure: types → validation → runner-sidecar → aggregation → renderer → dispatch → sidecar-load. Each layer has a well-defined responsibility, known file location, and fixed sequencing constraint. nan-008 (phase-stratified eval) is a 5-component precursor that matches layers 1–2 and 4–6; nan-010 added layers 3 (runner sidecar) and 7 (sidecar load) when decoupling became necessary. Future eval harness features extending or replacing Section 5 should follow this decomposition rather than rediscovering it.

### 1.2 Existing Entries — Status

| Entry | Status | Notes |
|-------|--------|-------|
| #3582 — Eval Harness Side-Car Metadata File Pattern | Complete — no update needed | nan-010 validated it without modification. The "decoupling run from report" rationale held exactly as written. |
| #3585 — Eval harness atomic sidecar write pattern | Complete — no update needed | Atomic write protocol (`.tmp` → rename → fallback copy) implemented verbatim; AC-05 and R-04 tests confirm correctness. |
| #3583 — render.rs 500-line split | Complete — no update needed | Pattern applied correctly for both `render_distribution_gate.rs` and `aggregate/` pre-split. No surprises. |
| #3601 — eval/report module split visibility pattern | Complete — no update needed | `pub(super)` vs `pub(crate)` distinction validated again by this feature. |
| #3602 — Rust pub(super) re-export trap | Complete — no update needed | nan-010 agents were aware of this; no re-export failures in gate-3b. |
| #3512 — Dual-type constraint pattern | Complete — no update needed | ADR-002 was built on this pattern; both `ScenarioResult` copies verified unchanged in gate-3b. |

### 1.3 Skipped

- **Compile-cycles pattern**: #3544 and #3439 already cover "batch field additions before compiling" with sufficient detail. nan-010's 95-cycle hotspot is consistent with those lessons (7-component feature with type system changes across multiple crates). No new lesson — the existing entries apply directly.

---

## 2. Procedures

### 2.1 New Entry

**#3614 — Correcting interface signatures across multi-document design specs** (new)

The gate-3a double-rework exposed a missing procedure: there was no documented method for correcting an interface signature that appears in multiple design documents. The procedure now stored covers: enumerate all documents before editing any (ARCHITECTURE.md Integration Surface table, ARCHITECTURE.md body text, all pseudocode caller and callee files, OVERVIEW.md, IMPLEMENTATION-BRIEF.md); edit in a single pass; grep for the old signature to confirm zero residual matches; and explicitly check for stale pre-computation blocks that become inconsistent when a parameter moves.

### 2.2 Existing Procedures — Status

- **#555 — How to verify cross-file consistency** covers workflow/protocol files. The new #3614 is narrower and domain-specific: it targets design spec documents (architecture + pseudocode) and specifically addresses the caller/callee asymmetry and stale pre-computation block failure mode that #555 does not cover. These are complementary, not overlapping.

---

## 3. ADR Validation

All 5 ADRs are validated by successful delivery. Gate-3b and gate-3c both passed with 2183 tests, 0 failures.

| ADR | Entry | Validated | Notes |
|-----|-------|-----------|-------|
| ADR-001: Module pre-split as first implementation step | #3586 | Yes | Pre-splits committed and building before feature code; render.rs at exactly 500 lines (gate-3b), aggregate/mod.rs at 490. R-01 (Critical) passes. |
| ADR-002: Sidecar file, zero ScenarioResult changes | #3587 | Yes | Both `ScenarioResult` copies confirmed identical at 5 fields in gate-3b. `test_report_without_profile_meta_json` passes (R-15). |
| ADR-003: mrr_floor as veto | #3588 | Yes | Structurally separate `mrr_floor_passed` field in `DistributionGateResult`. `test_distribution_gate_mrr_floor_veto` and `test_distribution_gate_distinct_failure_modes` both pass. R-05, R-06 all pass. |
| ADR-004: Atomic sidecar write | #3589 | Yes | `.tmp` → rename protocol confirmed in `test_write_profile_meta_schema` (no orphan `.tmp`, bidirectional serde). R-04 passes. |
| ADR-005: Per-profile Section 5 rendering | #3590 | Yes | Per-profile dispatch loop; `test_distribution_gate_section_header` validates both Single (`## 5.`) and Multi (`### 5.N`) heading variants. R-09 passes. |

No ADR needed revision during implementation. The gate-3a rework touched the architecture document (Integration Surface table and Component 5 body text) but the ADR content itself was correct throughout — the rework corrected a transcription error in the pseudocode, not a decision error.

**Flag for supersession:** None. All 5 ADRs remain active and accurate.

---

## 4. Lessons

### 4.A Gate Failure Lessons

**#3611 — Interface signature correction must update all dependent design docs simultaneously** (new)

Generalizable from gate-3a rework root cause: fixing `render_distribution_gate_section`'s parameter count in `section5-dispatch.md` left `report-sidecar-load.md` and ARCHITECTURE.md Component 5 body text with the old count. Gate failed a second time on the caller side. Lesson: enumerate all documents referencing the interface before editing any; edit in one pass; grep to confirm zero residual old signatures.

**#3612 — Gate rework: verify caller pseudocode when fixing callee interface (and vice versa)** (new)

Distinct from #3611 — this captures the reviewer's verification discipline specifically. After a rework, the reviewer must explicitly check the structural counterpart (caller for callee) not just the file that was reported as failing. The stale "Step 4.5: pre-compute distribution gates" in `report-sidecar-load.md` survived rework-1 because the reviewer only re-checked `section5-dispatch.md`.

### 4.B Hotspot Lessons

**Compile cycles (95 cycles) — no new lesson.** #3544 and #3439 fully cover this. The recommendation "batch field additions before compiling" is exactly what those entries say. The 95-cycle count for a 7-component feature with new types propagating to 3 subsystems is within the expected range given that pattern.

**#3613 — Avoid multi-filter git show pipelines — use git show SHA:path or git diff instead** (new)

From output_parsing_struggle hotspot at +135m: agent used 8 chained filters on `git show` in 3 minutes trying to extract a historical function body. The stored lesson captures the correct alternatives: `git show <sha>:path/to/file` retrieves the full file at a commit; `git diff <sha>..HEAD -- file` shows the delta; more than 2 piped filters is a signal the extraction strategy is wrong.

### 4.C Positive Signal

**knowledge_entries_stored = 22.0 vs. mean 8.0 (2.75× above average).** This is the highest recent count. Breakdown: 5 ADRs (design phase) + ~4 patterns (design + delivery phases) + this retrospective adds 5 more entries (#3610–#3614). The high count reflects a feature with multiple genuine design trade-offs (5 ADRs, all independently motivated) and significant new structural territory (first feature to introduce a runner sidecar for eval harness decoupling). This is not an anomaly to correct — it reflects that the knowledge stewardship process worked as intended for a feature of this complexity. The 5-ADR count should be expected for features that introduce new cross-subsystem patterns; it is not inflated.

---

## 5. Retrospective Findings

### 5.1 Hotspot-Derived Actions

| Hotspot | Assessment | Action |
|---------|------------|--------|
| `context_load` 146 KB before first write | Likely caused by 10-read cluster at session start (scope + all architecture/pseudocode files). Not pathological for a 7-component feature. | No action — context load scales with component count. |
| `compile_cycles` 95 (peak 17 in 5 min) | High but expected for type system changes across 3 subsystems. Covered by #3544, #3439. | No new entry. Agents should be pointed to existing lessons at spawn. |
| `file_breadth` 82 distinct files in scope phase | Eval harness touches profile, runner, report, aggregate, render — plus test files and docs. 82 is plausible for a 7-component feature. | No action. |
| `mutation_spread` 64 files across 10 clusters | Consistent with component count and test coverage (20 non-negotiable test names + extended existing tests). | No action. |
| `tool_failure_hotspot` 6 Read + 4 Bash failures at +0m | Context search burst at session start; likely MCP latency or server cold-start. Pre-existing pattern. | No action — covered by existing server reliability work (vnc-004). |
| `reread_rate` 66 files re-read | Agents re-reading interface definitions for cross-checking. Expected for a multi-component feature. | No action. |
| `output_parsing_struggle` at +135m | 8 filters on `git show`. Stored as lesson #3613. | Done. |

### 5.2 Recommendation Actions

| Recommendation | Action Taken |
|----------------|-------------|
| "Batch field additions before compiling — resolve all type definitions in-memory before each build" | Confirmed covered by #3544 and #3439. No new entry. Recommendation aligns with existing lessons. |

### 5.3 Baseline Outlier Notes

| Outlier | Value vs. Mean | Note |
|---------|---------------|------|
| `knowledge_entries_stored` | 22.0 vs. 8.0 | Positive signal. 5 ADRs reflect genuine design complexity, not over-documentation. See Section 4.C. |
| `parallel_call_rate` | 0.4 vs. 0.3 | Above average. Agents used parallel tool calls effectively (evidence of good spawn prompt structure). |
| `permission_friction_events` | 0.0 vs. 15.9 | Zero friction. All agents had correct capabilities; no blocked operations. |
| `post_completion_work_pct` | 0.0 vs. 4.9 | Clean delivery stop. No scope creep after gate-3c. |
| `coordinator_respawn_count` | 0.0 vs. 1.3 | No SM context loss across 2 sessions. |
| `sleep_workaround_count` | 0.0 vs. 3.4 | No polling hacks. |

### 5.4 Gate-3a Rework Summary

Gate-3a failed twice before passing. Both failures trace to the same root cause: interface corrections applied one-file-at-a-time rather than across all dependent documents simultaneously.

- **Rework 1 root cause:** `render_distribution_gate_section` parameter count fixed in ARCHITECTURE.md Integration Surface table and `section5-dispatch.md` but not in ARCHITECTURE.md Component 5 body text or `report-sidecar-load.md`.
- **Rework 2 root cause:** After rework-1 fixed the parameter count in the caller, a stale "Step 4.5: pre-compute distribution gates" block remained in `report-sidecar-load.md` — a pre-computation step that had been superseded by the corrected design but was not removed in rework-1 because the reviewer only re-checked the primary changed file.

Both failure modes are now documented as lessons (#3611, #3612) and a corrective procedure (#3614).

---

## Knowledge Stewardship

Queried before analysis:
- `/uni-query-patterns` for eval harness component decomposition, Rust module split patterns, sidecar patterns: found #3582, #3583, #3585, #3601, #3602, #3512, #3568
- Searched for interface-sync procedure: found #555 (workflow files, not design specs), #723 (arch/spec cross-validation at handoff, different trigger)
- Searched for compile-cycles lessons: found #3544, #3439 — confirmed no new entry needed
- Searched for git show output parsing: no existing entry

Stored:
- #3610 — 7-component eval harness decomposition pattern (new)
- #3611 — Interface signature correction must update all dependent design docs simultaneously (lesson)
- #3612 — Gate rework: verify caller pseudocode when fixing callee interface (lesson)
- #3613 — Avoid multi-filter git show pipelines (lesson)
- #3614 — Correcting interface signatures across multi-document design specs (procedure)
