# Gate 3a Report: crt-053

> Gate: 3a (Component Design Review)
> Date: 2026-06-10
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | Single component = `seed_ids` filter inside `if self.ppr_expander_enabled`; typed-enum predicate; no decomposition. Matches "single surgical edit." |
| Specification coverage (FR-01..FR-08) | PASS | All eight FRs addressed; no added logic, helper, redirect, penalty change, or config. |
| Risk coverage (Critical R-01..R-04) | PASS | All four Critical risks mapped to scenarios; R-04 differential control arms on both AC-01 and AC-05. |
| Interface consistency | PASS | OVERVIEW types match per-component usage; no new symbols; integration surface reused verbatim. |
| ANTI-AC-01 honored | PASS | No deprecated-absence-in-Flexible assertion; explicitly forbidden in both plan files + grep gate. |
| C-01/C-02 boundary | PASS | Production diff = filter clause alone; test-support `new_with_expander` framed as non-production, does not count against C-01. |
| Stewardship compliance | PASS | Architect: `Stored:` #4917 + reasoned "nothing else novel." Pseudocode (read-only): `Queried:` entries present. |

## Detailed Findings

### Architecture alignment
**Status**: PASS
**Evidence**: `pseudocode/OVERVIEW.md:16-18` defines exactly one affected component — the Phase 0 `seed_ids` build in `search.rs`, edit at line 915 inside `if self.ppr_expander_enabled` (line 911). This is precisely the ARCHITECTURE "The Change (single surgical edit)" (`ARCHITECTURE.md:39-64`) and the Component Breakdown (`ARCHITECTURE.md:68-76`: "No new components, modules, structs, traits, config flags, or files. No new function"). The predicate is the typed `e.status == Status::Active` enum comparison (`search-seed-filter.md:34, 62`; `Predicate Design` §89-99), matching `ARCHITECTURE.md:113` and ADR-001. Lives strictly inside the enabled branch — matches the C-02 lexical-scope guarantee (`ARCHITECTURE.md:103-109`). `graph_expand` signature confirmed unchanged. The pseudocode also tightens C-01 by verifying no import edit is needed (`OVERVIEW.md:75-80`), which is consistent with — not a departure from — the architecture.

### Specification coverage (FR-01..FR-08)
**Status**: PASS
**Evidence**:
- FR-01 (active-only seed predicate) — `search-seed-filter.md:32-36, 59-64`.
- FR-02 (enum-based predicate) — `Predicate Design` §89-99: "Never a string compare... `#[repr(u8)]` discriminant comparison."
- FR-03 (terminal-active heads retained) — `OVERVIEW.md:117-122` (Sequencing Constraint: 6b heads pass by construction) and AC-04 retention test.
- FR-04 (downstream of BFS unchanged) — `search-seed-filter.md:67-83` body shows lines 919-969 unchanged.
- FR-05 (traversal semantics unchanged) — `OVERVIEW.md:59-60`; Data Flow table row "forward BFS ... unchanged."
- FR-06 (HNSW ranking path untouched) — `search-seed-filter.md:124-126`: deprecated entries keep HNSW penalty, stay visible in Flexible.
- FR-07 (default-off equivalence) — `OVERVIEW.md:83-95` Off-Path Equivalence Argument; bit-for-bit identical via lexical scope.
- FR-08 (single production edit site) — `OVERVIEW.md:78-80`: "the diff is the filter clause alone."
No scope additions: no `find_terminal_active` on injected entries, no `penalty_map` mutation, no new config flag, no new function — the pseudocode explicitly flags any such addition as a C-01/R-03 scope-creep failure (`search-seed-filter.md:11`).

### Risk coverage (Critical R-01..R-04)
**Status**: PASS
**Evidence** (`test-plan/OVERVIEW.md:38-51` Risk→Test Mapping + `search-seed-filter.md`):
- R-01 (no eval-harness gate) — GATE-04 + all ACs assert entry-ID presence/absence/rank; "ZERO P@5/MRR/soft-GT pass/fail gates." Reinforced in test discipline header (`OVERVIEW.md:10-11`).
- R-02 (positive retention) — AC-04 (`test_seed_filter_retains_terminal_active_head`) asserts the 6b head AND its neighbor are injected; AC-01 positive arm asserts the active-seed neighbor IS present (`search-seed-filter.md:123-138, 70-73`). Proves RETAIN, not only drop.
- R-03 (diff-scope trip-wires) — GATE-01 (diff touches only `seed_ids` build), GATE-02 (existing `graph_expand` write-only negative tests UNCHANGED — the #4495 vnc-018 trip-wire), GATE-03, GATE-05 (`search-seed-filter.md:207-216`).
- R-04 (differential control arm) — AC-01 and AC-05 each carry a required `_control` arm (`search-seed-filter.md:80-91, 111-117`) plus an explicit fixture-precondition assert that the deprecated seed's out-edge exists and the neighbor is reachable by no active path (`search-seed-filter.md:48-55`). No absence assertion stands alone.

### Interface consistency
**Status**: PASS
**Evidence**: `pseudocode/OVERVIEW.md` Types-In-Play table (§64-73) matches the per-component Integration Surface table (`search-seed-filter.md:148-156`): `Status`, `EntryRecord.status`, `results_with_scores : Vec<(EntryRecord, f64)>`, `seed_ids : Vec<u64>`, `graph_expand` signature, `ppr_expander_enabled`. The pseudocode corrected the score type to `f64` (vs `f32` in source docs) and noted it is irrelevant to the predicate (score bound to `_`) — a documented accuracy refinement, not a contradiction. No new symbol introduced anywhere. OQ-2 resolved: `results_with_scores` is the sole seed source (`OVERVIEW.md:99-112`).

### ANTI-AC-01 honored
**Status**: PASS
**Evidence**: No test in either plan file asserts deprecated absence in Flexible. AC-01/AC-05 explicitly note "A itself (Deprecated) MAY still appear in results from the HNSW path — that is correct (C-03). Do NOT assert A absent (ANTI-AC-01)" (`search-seed-filter.md:77-78, 109`). AC-03 instead requires a positive presence-of-deprecated-in-Flexible assertion (`search-seed-filter.md:173-177`). A grep gate (ANTI-AC-01, `search-seed-filter.md:216`) enforces no such assertion is added.

### C-01/C-02 boundary preserved
**Status**: PASS
**Evidence**: Production diff = the filter clause alone (`OVERVIEW.md:78-80`, `search-seed-filter.md:25-39`). The chosen test surface is `crates/unimatrix-server/tests/pipeline_e2e.rs` plus a test-support-only `TestHarness::new_with_expander` constructor in `test_support.rs`, explicitly framed as "test support only — not a production edit; it does NOT count against C-01... test infrastructure is cumulative per CLAUDE.md" (`test-plan/OVERVIEW.md:110-121`, `search-seed-filter.md:18-36`). C-02 off-path identity is asserted by AC-02 (`test_off_path_identical_to_baseline`) plus the lexical-scope review companion (`search-seed-filter.md:141-161`). Boundary correctly preserved.

### Knowledge stewardship compliance
**Status**: PASS
**Evidence**:
- Architect (active-storage tier): `## Knowledge Stewardship` present (`crt-053-agent-1-architect-report.md:27-29`) with `Queried:` (context_briefing) and `Stored: entry #4917 "ADR-001..."` plus a reasoned "Nothing else novel — the recurring pattern (#4887) already exists."
- Pseudocode (read-only tier): `## Knowledge Stewardship` present (`crt-053-agent-1-pseudocode-report.md:29-31`) with `Queried:` entries (context_search #3992/#4434/#3744, context_get #4917) and "Deviations: none." Read-only tier correctly has no storage obligation.
All design-phase reports satisfy the obligation; no missing block, no unreasoned "nothing novel."

## Rework Required

None.

## Scope Concerns

None.
