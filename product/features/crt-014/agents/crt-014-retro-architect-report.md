# crt-014 Retrospective — Architect Report

**Agent ID:** crt-014-retro-architect
**Role:** Architecture retrospective — pattern extraction, ADR validation, lesson extraction
**Date:** 2026-03-15

---

## Summary

All 6 crt-014 ADRs stored in Unimatrix. Two supersession ADRs (ADR-003, ADR-004) correctly deprecated their predecessor entries. One new pattern stored (SupersessionGraph module). Two lessons stored. Pattern #1588 verified accurate. Procedure #487 covers compile_cycles hotspot — no new entry needed.

---

## 1. Patterns

### New Entries
- **#1607** — `SupersessionGraph: two-pass DAG build, graph_penalty 6-priority dispatch, find_terminal_active iterative DFS`
  - Topic: `unimatrix-engine` | Category: `pattern`
  - Covers: opaque struct layout, two-pass build algorithm, 6-priority penalty dispatch, iterative DFS with depth cap, caller pattern for full-store read in search.rs, crt-017 extension point

### Verified Entries (no change)
- **#1588** — `Store::query(QueryFilter::default()) returns Active-only — use query_by_status per variant for full-store reads`
  - Confirmed complete and accurate. The code snippet, affected file/line reference, and impact description (silent exclusion of Deprecated entries from the supersession DAG) are all correct. No correction needed.

### Skipped
- No existing pattern entries covered the SupersessionGraph module before this retrospective. The pattern search for `petgraph graph DAG supersession topology` returned no pattern-category results.

---

## 2. Procedures

### Existing Entry (no change)
- **#487** — `How to run workspace tests without hanging`
  - Already covers targeted per-crate invocations (`cargo test -p unimatrix-{crate} --lib`) and explicitly recommends `--lib` for routine validation. The compile_cycles hotspot (39 cycles) is addressed by this entry. No update needed — the guidance is current and applicable.

### Skipped
- No new procedure entry for compile_cycles. The existing procedure #487 already prescribes the correct behavior. The hotspot reflects agent execution choices, not a missing procedure.

---

## 3. ADR Status

### Stored in Unimatrix

| ADR File | Entry ID | Title | Action |
|----------|----------|-------|--------|
| ADR-001-petgraph-stable-graph-only.md | #1601 | ADR-001 (crt-014): petgraph with stable_graph feature only | Stored |
| ADR-002-per-query-graph-rebuild.md | #1602 | ADR-002 (crt-014): Per-Query Graph Rebuild (no caching) | Stored |
| ADR-003-supersede-system-adr-003-multi-hop.md | #1603 | ADR-003 (crt-014): Multi-Hop Supersession Traversal (supersedes #483) | Stored |
| ADR-004-supersede-system-adr-005-penalties.md | #1604 | ADR-004 (crt-014): Topology-Derived Penalty Scoring (supersedes #485) | Stored |
| ADR-005-cycle-fallback-strategy.md | #1605 | ADR-005 (crt-014): Cycle Detection Fallback Strategy | Stored |
| ADR-006-graph-penalty-constants.md | #1606 | ADR-006 (crt-014): Named Penalty Constants in graph.rs (Fixed for v1) | Stored |

### Superseded Entries Deprecated

| Old Entry | Title | Deprecated By | Reason |
|-----------|-------|---------------|--------|
| #483 | ADR-003: Single-Hop Supersession Traversal | #1603 | crt-014 delivered petgraph cycle detection — prerequisite for lifting single-hop limit. find_terminal_active() replaces entry.superseded_by guard. |
| #485 | ADR-005: Deprecated 0.7x and Superseded 0.5x Penalty Multipliers | #1604 | graph_penalty() with 6 named topology-derived constants replaces DEPRECATED_PENALTY and SUPERSEDED_PENALTY. Prerequisite (petgraph) now met. |

---

## 4. Lessons

### New Entries
- **#1608** — `Bash Permission Retries Persist Across Features -- Cargo Allowlist Fix Not Yet Applied (crt-014 recurrence)`
  - Topic: `architect` | Category: `lesson-learned`
  - Recurrence evidence: nan-002 (6), col-022 (28), crt-014 (21) retries. Prior entries #1164 and #1270 were already deprecated. This entry updates the recurrence count and notes the fix remains unapplied (human config action required).

- **#1609** — `ServiceError variant names must be verified against error.rs -- no ServiceError::Internal exists`
  - Topic: `unimatrix-server` | Category: `lesson-learned`
  - Spawn prompt specified `ServiceError::Internal` — a variant that does not exist. Correct variants: `ServiceError::EmbeddingFailed(String)` for join errors, `ServiceError::Core(CoreError::Store(e))` for store failures. Caught at implementation time (gate-3b key deviation). Generalizable: error variant names in pseudocode must be grep-verified against the actual enum.

### Skipped

| Hotspot | Disposition |
|---------|-------------|
| cold_restart (2 events, 115KB load) | Entry #1271 covers this. crt-014 had 5 components across 2 crates — 115KB context load and 16-69 file re-reads are within normal bounds per #1271 (≤75KB per component threshold). Not actionable. |
| session_timeout (3.5h gap) | Entry #324 covers coordinator checkpointing lesson. crt-014 gap caused re-reads but no task rework. No new information beyond #324. |
| mutation_spread (41 files) | Cross-cutting constant removal (DEPRECATED_PENALTY, SUPERSEDED_PENALTY) across all importers + new graph module. Structurally unavoidable for this type of refactor. No generalizable lesson — the scope was correctly defined. |
| post_completion_work (84.3%) | Inflated by delivery protocol's mandatory post-implementation work (gate validation, report writing, worktree cleanup). Entry #1271 context applies. Not a real issue. |
| compile_cycles (39) | Addressed by existing procedure #487. No new lesson — the procedure already prescribes `--lib` for routine validation. |
| permission_retries (Read, 11 retries) | Read retries are user-approval latency, not a config issue. No actionable fix available. Noted in #1608 for completeness. |

---

## 5. Retrospective Hotspot Actions

| Hotspot | Severity | Action Taken |
|---------|----------|--------------|
| permission_retries (Bash: 10, Read: 11) | Warning | Stored #1608 (recurrence lesson). Cargo allowlist fix remains a human config action. |
| compile_cycles (39) | Warning | No new entry. Existing procedure #487 covers targeted invocations. |
| cold_restart (2 events) | Warning | No new entry. #1271 and #324 cover this. Within normal bounds for 5-component feature. |
| sleep_workarounds (6) | Info | No entry. Procedure recommendation (run_in_background) already documented elsewhere. |
| search_via_bash (11.8%) | Info | No entry. Feature-specific tooling friction, not a recurring pattern. |
| post_completion_work (84.3%) | Info | No entry. Inflated by protocol structure, not a real issue. |
| mutation_spread (41 files) | Warning | No entry. Structurally required by the constant removal scope. |
| ServiceError::Internal mismatch | (gate finding) | Stored #1609 (lesson). |

---

## Knowledge Stewardship

- Queried: `context_search` for petgraph/graph/DAG patterns (no prior pattern entries), procedure for targeted test invocations (found #487 — current), session gap lessons (found #324, #1271 — current), hardcoded penalty decisions (found #485 — superseded), settings.json allowlist lessons (found #1164, #1270 — both deprecated).
- Stored: #1601–#1606 (6 ADRs), #1607 (pattern), #1608 (lesson), #1609 (lesson)
- Deprecated: #483 (superseded by #1603), #485 (superseded by #1604)
