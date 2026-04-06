# bugfix-491-retro-architect — Retrospective Report

## Patterns

### Checked existing entries #4167, #4168, #4169, #4170

All four entries stored during the session are complete and accurate. No corrections applied.

- **#4167** (lesson: inclusive SQL filter undercounts on new EDGE_SOURCE additions) — accurate; no change needed.
- **#4168** (pattern: exclusive NOT IN filter for inferred_edge_count) — accurate; no change needed.
- **#4169** (lesson: stale xfail assertion content after semantic contract change) — accurate; no change needed.
- **#4170** (pattern: EDGE_SOURCE constants in format!() SQL require ASCII lowercase+underscore constraint at declaration) — accurate; no change needed.

## Procedures

### Sleep workarounds (#3561)

Entry #3561 ("Avoid sleep polling in tester agents: use run_in_background + TaskOutput") already exists and fully covers the 6-sleep hotspot observed in bugfix-491. No new entry or correction needed.

### Monitoring metric semantic contract change procedure

Not stored. #4169 (stale xfail staleness) already covers the most actionable consequence. A generic procedure would be thinner than the specific lesson — skipped.

### Table-driven test case enumeration

Covered by the correction to #4150 -> #4173. No separate procedure entry.

## ADR Status

### ADR-004 crt-041 (#4063) — deferral resolved in part

ADR-004 deferred per-source edge count breakdowns (s1_edge_count, s2_edge_count, etc.) to a follow-up. bugfix-491 resolved the related but distinct undercounting bug in the aggregate `inferred_edge_count`. The per-source breakdown deferral is still active — the ADR remains accurate as written. No correction applied.

## Lessons

### Gate failure: #3927 -> corrected to #4172

Entry #3927 was served during bugfix-491 but gate-3b still failed for missing agent report files. The original entry focused on batch bugfix sessions. Extended via `context_correct` to explicitly cover single-agent sessions (same failure mode, different cause: spawn prompt treats file as optional rather than as a hard deliverable).

New entry: **#4172** — extends #3927 with single-agent failure pattern and concrete three-point spawn prompt checklist applicable to all bugfix types.

### Compile cycles: #4150 -> corrected to #4173

37 compile cycles with a burst of 12 in 5 minutes during the testing phase. Distinct from the struct-cascade mechanism in #4150: TC-15 was written case-by-case against a known 7-case test plan (6 counted + 1 excluded sources). Extended entry with table-driven test discipline: write the full table before the first compile when the test plan is already known.

New entry: **#4173** — adds per-case compile churn pattern for table-driven tests to the existing struct-cascade lesson.

## Retrospective Findings

| Hotspot | Action | Reason |
|---------|--------|--------|
| compile_cycles (37, burst 12) | Corrected #4150 -> #4173 | Added table-driven per-case churn nuance — distinct mechanism from struct cascade |
| sleep_workarounds (6) | No action | #3561 already covers this; no new signal |
| tool_failure_hotspot (4x context_store, 4x Bash) | No action | Transient failures, no pattern to generalize |
| edit_bloat (avg 112.4KB) | No action | Large files (read.rs 2934L, nli_detection_tick.rs 3720L) are pre-existing; no new lesson |
| search_via_bash (18.2%) | No action | Positive outlier (bash_for_search_count below mean); 18% is within acceptable range |
| reread_rate (18 files) | No action | Large file re-reads expected in a 3720-line file; no actionable pattern |
| Gate 3b FAIL -> PASS (rework 1) | Corrected #3927 -> #4172 | Single-agent session failure not covered by original batch-focused entry |

## Knowledge Stewardship

Queried:
- `context_briefing` (task-scoped) — returned entries #4167, #4168, #3621, #3591, #3592, #4063, #4156, #3600, #4041, #4162
- `context_search` ("table-driven test edge source coverage inference") — #3386, #4082, #4167
- `context_search` ("gate failure missing agent report stewardship spawn prompt") — #3927, #4022, #3933, #2958, #683
- `context_search` ("monitoring metric semantic contract label change") — no relevant new signal
- `context_search` ("tester agent sleep polling async test background process") — #3561 (already covers it)
- `context_search` ("compile cycles table-driven test iterative development") — no new signal beyond #4150
- `context_get` on #3927, #4022, #4063, #4150, #3561, #4167, #4168, #4169, #4170

Stored:
- **#4172** — corrected #3927: extended lesson on missing agent reports to cover single-agent bugfix sessions with concrete spawn-prompt checklist
- **#4173** — corrected #4150: extended compile-cycle lesson to cover table-driven test per-case churn pattern (bugfix-491 TC-15)
