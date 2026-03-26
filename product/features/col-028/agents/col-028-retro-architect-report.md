# col-028 Retrospective: Architect Report

> Agent: col-028-retro-architect (uni-architect)
> Date: 2026-03-26
> Feature: col-028 — Unified Phase Signal Capture (Read-Side + query_log)

---

## 1. Patterns

### New Entries

**#3540** — Atomic change unit for SQLite positional column additions: INSERT + SELECT(s) + row deserializer must be modified together

Rationale: The four sites (analytics INSERT, both SELECT queries, row deserializer) are linked by positional index. Omitting or reordering any one is a silent runtime error — the compiler cannot catch it. This generalizes from col-028 SR-01 and is reusable for any future analytics table column addition. It is distinct from the idempotent ALTER TABLE pattern (#1264) and from the schema version cascade pattern (#2933/#3539).

**#3541** — Free function at module scope for testable SessionRegistry reads in MCP handlers

Rationale: The `current_phase_for_session` free function form from col-028 ADR-001 is a generalizable pattern applicable any time a repeated SessionState read needs isolated unit testing. Methods on UnimatrixServer cannot be unit-tested without constructing the full server dependency graph. This pattern has already been applied twice (crt-025 style inline, col-028 explicit free function) and is now documented for future handlers.

### Corrections to Existing Entries

**#3027 → #3538** — Phase snapshot in MCP handlers: first-statement discipline, free function form, single-binding dual-consumer rule

The original entry (#3027) documented the pattern for `context_store` only. col-028 extended it to all four read-side tools and introduced two new constraints not in the original:
- The free function form (`current_phase_for_session`) as the preferred extraction mechanism
- The single-binding / dual-consumer rule at `context_search` (SR-06): one `get_state` call shared by `UsageContext.current_phase` and `QueryLogRecord::new` to prevent divergence

**#2933 → #3539** — Schema Version Cascade: Complete checklist including column-count and parity tests

The original entry (#2933) documented the core cascade pattern but had two gaps discovered by col-028:
- `migration_v10_to_v11.rs` has a column-count assertion that also needs updating when columns are added to existing tables (not just the most recent migration test file)
- `sqlite_parity.rs test_schema_column_count` was mentioned in the original but not as a separate explicit step — col-028 confirmed it as a distinct gap that gate reviewers can miss

The ADR-007 cascade file list in the col-028 spec was also incomplete; the implementation handled these correctly but the spec did not enumerate them. The corrected entry now has an explicit per-step checklist.

### Skipped (no new information)

- **#1264** (Idempotent ALTER TABLE via pragma_table_info): col-028 confirms this pattern exactly as documented. No extension.
- **#3180** (SessionState field additions / make_state_with_rework): col-028 confirms the pattern. No extension.
- **#3527** (D-01 guard early-return before filter_access): stored by the delivery agent during implementation. Complete and accurate.
- **#3503**, **#3510** (UsageDedup weight-0 gotcha entries): superseded / complemented by #3527. No additional action.

---

## 2. Procedures

### New Entry

**#3546** — How to add a nullable column to the query_log table (or any analytics table with positional binds)

This is a five-step procedure (identify sites, schema migration, atomic change unit, compile-fix sites, tests) synthesized from col-028 SR-01/SR-02/SR-03, the C-09 atomic change unit constraint, and the Gate 3b WARN (IR-03 compile-silent raw INSERT helper gap). It references patterns #3539 (schema version cascade) and #3540 (atomic change unit) as sub-procedures. No equivalent procedure existed before col-028.

### Existing Procedures (no update required)

- **#374** (in-place SQLite schema migration): covers column decomposition, not column addition. Distinct.
- **#836** (adding a new table to v6+ schema): covers new tables, not new columns on existing tables. Distinct.

---

## 3. ADR Status

All seven col-028 ADRs (#3513–#3519) are validated by Gate 3b PASS and Gate 3c PASS. No supersessions required.

| ADR | ID | Status | Notes |
|-----|----|--------|-------|
| ADR-001: Phase helper as free function | #3513 | Validated | Confirmed at line 291 of tools.rs |
| ADR-002: Phase snapshot first-statement before await | #3514 | Validated | Confirmed in all four handlers (lines 312, 437, 678, 972) |
| ADR-003: D-01 guard in record_briefing_usage before filter_access | #3515 | Validated | Guard at line 322 precedes filter_access; load-bearing placement confirmed by negative-arm test |
| ADR-004: confirmed_entries trigger uses request-side cardinality | #3516 | Validated | `params.id.is_some()` and `target_ids.len()==1` are equivalent; both forms documented in spec and gate report |
| ADR-005: confirmed_entries shipped without consumer | #3517 | Validated | Field populated but not consumed in col-028; reserved for Thompson Sampling (ass-032) |
| ADR-006: UsageContext.current_phase doc comment as deliverable | #3518 | Validated | Doc comment updated at lines 61-75 of usage.rs |
| ADR-007: phase column as last positional param; atomic change unit | #3519 | Validated with note | Implementation correct. Cascade file list in ADR text was incomplete (migration_v10_to_v11.rs column count and sqlite_parity.rs not enumerated). Handled correctly in implementation but spec gap documented. Pattern #3539 now has the complete list. |

No ADRs require supersession. ADR-007's incomplete cascade enumeration is a spec gap, not an architectural error — the decision itself (last positional param, atomic change unit) is sound and fully validated.

---

## 4. Lessons

### New Entries

**#3543** — Nullable column addition in test helpers is a compile-silent spec violation: raw SQL INSERT helpers must be explicitly updated

Source: Gate 3b WARN (FR-20 / IR-03). The `eval/scenarios/tests.rs insert_query_log_row` helper used an 8-column INSERT after query_log gained a 9th column. SQLite silently inserted NULL; the compiler did not complain. The spec requirement for an explicit `?9 = NULL` bind was unmet. Fixed in Stage 3c. The generalizable lesson: when adding a column, grep for raw INSERT helpers for the table separately from constructor call sites.

**#3544** — Cascading struct field addition drives compile cycles: complete type definitions before first build

Source: Retrospective hotspot F-01 (168 compile/check cycles, 2.5σ). The phase column touched 7+ files. The lesson: identify all use sites before editing, plan the complete edit set, execute in one pass, compile once. The pseudocode spec's SR-02/SR-03 compile-fix site enumeration is the mechanism for enabling this — delivery agents should read it as a planning checklist, not a post-hoc fix list.

**#3545** — Use Grep/Glob tools for content search — Bash search commands are a CLAUDE.md rule violation

Source: Retrospective hotspot F-11 (17.1% of Bash calls were search commands, 102/596). This is an existing CLAUDE.md rule. The lesson entry documents it as a confirmed pattern to reinforce the rule for future agents. Exception noted: AC-22-style grep commands in acceptance criteria are legitimate spec-defined verification steps.

### Corrections to Existing Lessons

**#1267 → #3542** — Agent reports omit Knowledge Stewardship section unless structurally enforced — recurring pattern (col-022, col-028)

col-028 is a second confirmed instance of Gate 3a failure due to missing Knowledge Stewardship sections — this time both architect and pseudocode agent reports were missing the section simultaneously. The lesson is now updated to reflect the pattern is systemic across agent types (not just architects), document the second data point, and explicitly extend the gate 3a checklist to all design-phase agent types.

---

## 5. Retrospective Findings

### Hotspot Analysis

**F-01 (compile_cycles: 168, Warning)** — Addressed by lesson #3544. Root cause: cascade schema change touching 7+ files. Recommendation to batch field additions before compiling is now documented as a generalizable lesson. The spec mechanism (enumerating compile-fix sites in SR-02/SR-03) already exists; the lesson reinforces that delivery agents should use it as a planning tool.

**F-04 (tool_failure_hotspot: context_search failed 23×, context_get failed 13×, Warning)** — These failures occurred during the develop/1 phase. Based on the gate reports (no mention of data loss or lookup failures affecting implementation), these were transient MCP connection issues consistent with pre-existing issue GH#52 (MCP server connection drops). No new lesson warranted — GH#52 is the tracked item. No fallback pattern is needed beyond what PidGuard and DatabaseLocked error handling already provide (vnc-004).

**F-11 (search_via_bash: 17.1%, Info)** — Addressed by lesson #3545. This is a CLAUDE.md rule violation. The lesson is stored to reinforce the rule for future agents; it is not a new rule, but a documented violation pattern.

**sleep_workarounds (2 instances, recommendation)** — The retrospective recommendation (use run_in_background + TaskOutput instead of sleep polling) is noted. No Unimatrix entry stored — this is a workflow tool-use pattern, not a codebase pattern.

### Baseline Outliers

**total_tool_calls: 2033 (2.5σ)** — Expected for a cascade schema change touching 7+ files across two crates. Not pathological; driven by the breadth of the change.

**knowledge_entries_stored: 44 (4.6σ)** — Cross-feature worktree contamination. The col-028 cycle itself stored 0 new knowledge entries during the feature cycle (ADRs were stored but not tagged to the cycle). This is a known limitation of cross-feature worktree sessions, not a col-028 quality issue.

**bash_for_search_count: 474 (1.6σ)** — Consistent with F-11 lesson. Addressed.

**Gate 3a rework** — Knowledge Stewardship sections missing from architect and pseudocode reports. Addressed by updated lesson #3542. The fix was a manual SM intervention before re-run; both sections were confirmed present in the final gate 3a PASS.

**Gate 3b WARN → resolved in 3c** — IR-03 nullable column bind gap in eval helper. Addressed by lesson #3543. Fixed by tester agent in Stage 3c; gate 3c confirmed PASS.

**Scope restart mid-design** — Two GH issues (#394, #397) combined into a single feature after partial artifacts were created. D-01 through D-05 decisions were preserved verbatim. No generalizable lesson warranted beyond good scope-combination hygiene; the combination was the correct architectural decision given shared root cause.

---

## Knowledge Stewardship

Queried:
- `context_search` (category: pattern) for schema migration ALTER TABLE idempotency → found #1264 (confirmed, no update)
- `context_search` (category: pattern) for SessionState lock-and-mutate / HashSet field addition → found #3412, #3027, #3180 (assessed)
- `context_search` (category: pattern) for free function module scope phase snapshot → found #3027, #1265, #3004 (assessed)
- `context_search` (category: pattern) for early-return guard weight-0 UsageDedup → found #3503, #3510, #3527 (confirmed, #3527 complete)
- `context_search` (category: pattern) for positional column index SQLite analytics atomic change unit → found #1264, #681 (gap confirmed — new pattern needed)
- `context_search` (category: procedure) for schema version cascade → found #374, #836, #2933 (correction warranted)
- `context_search` (category: lesson-learned) for Knowledge Stewardship section gate failure → found #1267 (correction warranted — second instance)
- `context_get` (full content) for #1264, #2933, #1267, #3527, #3027, #3180 to assess update need
- `context_lookup` (category: decision, topic: col-028) to confirm ADR IDs #3513–#3519

Stored:
- #3538 — Correction to #3027: phase snapshot pattern generalized to all handlers + free function form + single-binding dual-consumer rule
- #3539 — Correction to #2933: schema version cascade checklist extended with column-count test files and sqlite_parity.rs as explicit steps
- #3540 — New pattern: atomic change unit for SQLite positional column additions
- #3541 — New pattern: free function at module scope for testable SessionRegistry reads
- #3542 — Correction to #1267: Knowledge Stewardship omission lesson updated with second confirmed instance (col-028)
- #3543 — New lesson: nullable column addition in test helpers is a compile-silent spec violation
- #3544 — New lesson: cascading struct field addition drives compile cycles — complete type definitions before first build
- #3545 — New lesson: Bash search commands are a CLAUDE.md rule violation
- #3546 — New procedure: how to add a nullable column to an analytics table with positional binds

Declined:
- Sleep workaround recommendation: workflow tool-use pattern, not a codebase pattern — not stored in Unimatrix
- Scope restart lesson: the combination of GH #394 and #397 was correct architectural judgment; the pattern is too feature-specific to generalize
- F-04 tool failures: consistent with pre-existing GH#52; no new lesson beyond existing PidGuard/error-handling coverage
- #3412, #1265, #3004, #1264, #3180: all confirmed accurate and complete — no corrections needed
