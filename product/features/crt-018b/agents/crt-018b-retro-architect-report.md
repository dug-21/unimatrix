# crt-018b Retrospective — Architect Agent Report

> Agent: crt-018b-retro-architect (uni-architect)
> Date: 2026-03-15
> Feature: crt-018b Effectiveness-Driven Retrieval (PR #262, merged)

---

## 1. Patterns

### Updated

**#1560** — Background-tick state cache pattern: `Arc<RwLock<T>>` shared through ServiceLayer, sole writer is the tick
- Corrected from #1493 (ConfidenceStateHandle sharing pattern, crt-019 only)
- crt-018b delivered `EffectivenessStateHandle` as a second confirmed application of this pattern
- Updates: generalized from ConfidenceState-specific to any background-tick-computed state; added cold-start safety requirement; added non-optional constructor parameter rule; added cross-reference to generation-cached snapshot sub-pattern; recorded crt-018b as second validation

### New

**#1561** — Generation-cached snapshot: avoid per-call HashMap clone via generation counter + `Arc<Mutex<Snapshot>>`
- Standalone sub-pattern distinct from #1560 (which covers sharing structure; this covers clone avoidance for large collections)
- First implementation: crt-018b `EffectivenessState` (HashMap<u64, EffectivenessCategory>, generation: u64)
- Captures: structure, read/write patterns, lock ordering rules (R-01: read guard before mutex, R-02: write guard before SQL), clone-sharing rationale, cost amortization model
- Validated through Gate 3b and 3c; all 4 critical lock-ordering tests pass

### Skipped

- **Auto-quarantine from background tick structural pattern**: covered by existing #1542 (error semantics) and #1366 (tick loop error recovery). The collect-inside-lock / release / SQL-outside pattern is the NFR-02 / write-lock-before-SQL convention already referenced in gate reports (#1366, #1542).
- **Sole-writer invariant as standalone pattern**: fully subsumed by #1560 update.

---

## 2. Procedures

### New

**#1562** — Extract env-var parser to free function for testability without `unsafe set_var` (Rust 2024)
- Technique: split env-var reading (single callsite) from parsing/validation logic (pure `fn(Option<&str>) -> Result`)
- Tests call the pure function directly with controlled inputs — no env mutation, no `unsafe`, hermetic
- First used: crt-018b `parse_auto_quarantine_cycles_str()` in `background.rs`
- Generalizes to any server-level configuration read from env vars with validation logic

### Not stored

- **`if let Some(...)` vs guarded `.unwrap()` style**: Gate 3b WARN at background.rs:413 is a known project coding standard deviation (no `.unwrap()` in non-test code). This is a convention already understood; no new procedure adds value. The correct fix is enforced by code review, not by a stored procedure.

---

## 3. ADR Validation

All four crt-018b ADRs confirmed. No supersession required.

| ADR | Entry | Status | Validation Evidence |
|-----|-------|--------|---------------------|
| ADR-001: Generation counter + Arc<Mutex<EffectivenessSnapshot>> | #1543 | Confirmed | Gate 3b: verified at search.rs lines 168-194 and briefing.rs 183-205; unit tests test_generation_read_write_no_simultaneous_locks, test_snapshot_read_guard_dropped_before_mutex_lock |
| ADR-002: Hold-not-increment on tick error | #1544 | Confirmed | Gate 3b: EffectivenessState not touched on Err path; emit_tick_skipped_audit called before early return; test_emit_tick_skipped_audit_detail_fields |
| ADR-003: Utility delta inside penalty multiplication | #1545 | Confirmed | Gate 3b: verified at all four rerank_score call sites; test_utility_delta_inside_deprecated_penalty and test_utility_delta_inside_superseded_penalty provide numeric assertions |
| ADR-004: EffectivenessStateHandle as non-optional BriefingService constructor parameter | #1546 | Confirmed | Gate 3b: compile error enforcement confirmed; missing wiring is caught at compile time |

One implementation deviation from pseudocode (not an ADR issue): pseudocode used `quarantine_entry()` as a label; actual store API is `update_status(id, Status::Quarantined)`. Behavioral equivalence confirmed by Gate 3b. No ADR references the method name; no correction needed.

---

## 4. Lessons

### New

**#1563** — Design agents following an established pattern should query the pattern entry, not re-read source files
- Derived from hotspot: 601KB context load before first write (3.9 sigma outlier); top files were uni-design-protocol.md, confidence.rs, effectiveness/mod.rs, briefing.rs
- Root cause: architect read full protocol + multiple source files instead of retrieving stored ConfidenceStateHandle pattern (#1493 at design time) + one reference file
- Corrective workflow for adapter features: query Unimatrix pattern → retrieve entry → read one reference file → design
- No gate failures resulted, but 26 files accessed / 21 re-read signals compensatory reading for missing structured retrieval

### Not stored (already exists)

- **Architect report missing Knowledge Stewardship structural block** (#1267 already active): Gate 3a noted this WARN; gate 3a's own report confirmed "already captured as a pattern in gate-failure lessons." Entry #1267 covers this. No new entry.

---

## 5. Retrospective Findings

### Hotspot: context_load outlier (601KB, 3.9 sigma)

**Finding**: Design phase loaded 9x the project mean before producing any artifact. The feature explicitly reuses the ConfidenceState pattern from crt-019, which was already stored in Unimatrix as #1493.

**Action taken**: Lesson #1563 stored. Pattern #1493 updated to #1560 with richer content (cold-start, constructor rule, sub-pattern cross-reference) so future agents retrieving it get more of what they need without source-file reads.

**Residual gap**: The uni-design-protocol.md re-read pattern suggests agents may not be internalizing the protocol from their spawn prompt. This is a spawn-prompt design issue, not a Unimatrix content issue.

### Hotspot: file_breadth (26 files), reread_rate (21 files re-read)

**Finding**: Consistent with the context_load outlier — same root cause. For adapter features, breadth should be 5-8 files (pattern entry, 1 reference implementation, SCOPE.md, ARCHITECTURE.md of prior feature, plus produced artifacts).

**Action taken**: Captured in lesson #1563.

### Gate 3a WARN: architect report missing Knowledge Stewardship structural block

**Finding**: Substance correct (4 ADRs stored), format non-compliant (no `## Knowledge Stewardship` section). Pre-existing lesson #1267 covers this. No new entry needed.

**Observation**: The WARN recurred despite #1267 existing since col-022. The lesson is stored but not surfaced to architect agents at spawn time. This suggests the lesson needs to be injected via briefing or the architect spawn prompt needs a structural checklist requirement.

### Gate 3b WARN: bare `.unwrap()` at background.rs:413

**Finding**: `if report.effectiveness.is_some()` followed by `.unwrap()` on the next line. Safe but violates project style. The correct form is `if let Some(effectiveness_report) = report.effectiveness.as_ref()`.

**Action taken**: Not stored as a lesson (too narrow, already a known project convention). The pattern is: always prefer `if let Some(x) = opt` over `guard.is_some()` + `.unwrap()`. This is covered by the existing "no bare unwrap in non-test code" convention.

### Gate 3b WARN: background.rs exceeding 500-line limit

**Finding**: ~1007 lines of production code; 1811 lines total. Pre-existing growth from cumulative test infrastructure constraint (extend existing files, not isolated scaffolding). Not a new pattern; the architecture explicitly requires this trade-off.

**Action taken**: None — this is an accepted consequence of the test infrastructure accumulation rule. Not generalizable beyond the existing architectural constraint.

---

## Knowledge Stewardship

- Queried: `context_search` (pattern) — "Arc RwLock in-memory cache background tick sole writer"
- Queried: `context_search` (pattern) — "generation counter clone avoidance HashMap snapshot cache"
- Queried: `context_search` (pattern) — "background maintenance auto-quarantine store mutation after lock release"
- Queried: `context_search` (pattern) — "write lock scope before SQL synchronous store quarantine spawn_blocking"
- Queried: `context_search` (pattern) — "ConfidenceState ConfidenceStateHandle crt-019 background tick writer pattern"
- Queried: `context_search` (procedure) — "env var parse testability avoid unsafe set_var Rust 2024"
- Queried: `context_search` (lesson-learned) — "architect report knowledge stewardship section missing structural compliance"
- Queried: `context_lookup` (topic: crt-019, category: pattern)
- Retrieved full content: #1493, #1267, #1542, #1366, #1543, #1544, #1545, #1546
- Corrected: #1493 → #1560 (generalized, second validation)
- Stored: #1561 (generation-cached snapshot pattern)
- Stored: #1562 (env-var parser extraction procedure)
- Stored: #1563 (design agents over-reading lesson)
- Not stored: ADR corrections (all 4 confirmed correct); is_some+unwrap style lesson (existing convention); background.rs file size (architectural trade-off, not a new pattern)
