# Retrospective Architect Report: col-022

**Agent ID:** col-022-retro-architect
**Feature:** col-022 (Explicit Feature Cycle Lifecycle)
**Date:** 2026-03-13

---

## 1. Patterns

### New Patterns Stored

| ID | Title | Reason |
|----|-------|--------|
| #1265 | Dual-Path Validation: Shared Pure Function for MCP Tool + Hook Handler | col-022 established a reusable pattern: single `validate_cycle_params()` in validation.rs called by both MCP tool and hook handler, with different error handling per caller. Generalizable to any future MCP tool that also has a hook-side path. |
| #1266 | Specialized Event-Type Handler Before Generic RecordEvent Dispatch | col-022 added `handle_cycle_start` before the generic #198 RecordEvent handler. This "specialize-before-generic" dispatch structure is reusable for any future event_type that needs custom handling while preserving generic observation persistence. |

### Updated Patterns

| Original ID | New ID | Title | Change |
|-------------|--------|-------|--------|
| #620 | #1264 | Idempotent ALTER TABLE Guard via pragma_table_info | Added table-existence guard for tables created after migration (sessions table case from col-022). |

### Skipped

- **Server-Side Observation Intercept (#763)**: col-022 does not follow this exact pattern (it uses RecordEvent dispatch, not side-effect observation in a non-RecordEvent arm). No update needed.
- **Safety-Guard Validation (#604)**: col-022's `validate_cycle_params` follows this pattern correctly (permissive safety guard, not format enforcement). Still accurate.
- **ToolContext Pattern (#317)**: col-022's `context_cycle` handler follows the existing tool handler pattern. No drift detected.

---

## 2. Procedures

### Updated Procedures

| Original ID | New ID | Title | Change |
|-------------|--------|-------|--------|
| #619 | #1263 | How to add new fields to the v6+ normalized schema | Added step for table-existence guard (step 2d), step for updating *_COLUMNS constants (step 8), and added col-022 (v11->v12) to validated list. |

### No New Procedures

No new build/test/integration procedures emerged. The schema migration, MCP tool addition, and hook handler extension all followed existing procedures.

---

## 3. ADR Status

### Validated ADRs (all 5)

| ADR | Unimatrix ID | Status | Notes |
|-----|-------------|--------|-------|
| ADR-001: Reuse RecordEvent wire protocol | #1273 | Validated | Implementation confirmed: zero new HookRequest variants, cycle events flow through RecordEvent with shared CYCLE_START_EVENT/CYCLE_STOP_EVENT constants. 99 tests pass. |
| ADR-002: Force-set for explicit attribution | #1274 | Validated | `set_feature_force` implemented with `SetFeatureResult` enum. 11 tests cover all three variants (Set, AlreadyMatches, Overridden). Security review noted `Set` returned for unregistered sessions is misleading (Finding 2) but not functionally broken. |
| ADR-003: JSON column for keywords | #1275 | Validated | `keywords: Option<String>` on SessionRecord, stored as JSON array string. Round-trip tests pass including unicode and special characters. |
| ADR-004: Shared validation function | #1276 | Validated | Single `validate_cycle_params()` used by both MCP tool and hook handler. 33 validation tests. Split-brain risk (SR-07) fully mitigated. |
| ADR-005: Schema v12 keywords migration | #1277 | Validated | ALTER TABLE with pragma_table_info idempotency guard and table-existence guard. 16 migration integration tests pass. |

### Flagged for Supersession

None. All 5 ADRs were validated by successful implementation with no revealed inaccuracies.

### Implementation-Revealed Issues (not ADR-level)

- **ADR-002 edge case**: `set_feature_force` returns `Set` for unregistered sessions (security review Finding 2). This is a minor API design issue, not an ADR-level decision error. The ADR's decision is correct; the implementation detail of the return value for unregistered sessions could be improved with a `SessionNotFound` variant. Deferred to follow-up.
- **ADR-001 consequence materialized**: The `.to_string()` vs `.as_str()` keywords bug (Gate 3b WARN) is exactly the "implicit coupling via string constants" consequence ADR-001 predicted, but manifested in the data layer rather than the event_type layer. The event_type constants were shared correctly; the keywords Value extraction was not.

---

## 4. Lessons

### New Lessons Stored

| ID | Title | Source |
|----|-------|--------|
| #1267 | Agent reports omit Knowledge Stewardship section unless structurally enforced | Gate 3a REWORKABLE FAIL |
| #1268 | Test payloads must match real producer serialization to catch .to_string() vs .as_str() bugs | Gate 3b WARN |
| #1271 | Context load and cold restart hotspots scale with component count -- normalize before flagging | Hotspot analysis |
| #1272 | Mutation spread hotspot inflated by design artifacts -- separate code vs artifact mutation counts | Hotspot analysis |

### Updated Lessons

| Original ID | New ID | Title | Change |
|-------------|--------|-------|--------|
| #1165 | #1269 | High Compile Cycles Signal Need for Targeted Test Invocations | Added col-022 evidence (106 cycles), multi-crate targeted build tip, recurrence confirmation. |
| #1164 | #1270 | Bash Permission Retries Indicate Missing Allowlist Entries | Added col-022 evidence (28 total retries across 3 tools), MCP tool retry context, recurrence note. |

---

## 5. Retrospective Findings

### Hotspot-Derived Lessons

| Hotspot | Severity | Action |
|---------|----------|--------|
| permission_retries (Bash 9, Read 7, context_store 12) | Warning | Updated existing lesson #1164 -> #1270. Bash allowlist fix still not applied after 2 features. MCP tool retries are inherent to approval model. |
| compile_cycles (106) | Warning | Updated existing lesson #1165 -> #1269. Confirmed systemic: nan-002 had 60, col-022 has 106. Targeted builds not adopted. |
| cold_restart (46-min gap, 18 re-reads) | Warning | Stored #1271. Acceptable for 5-component feature. Per-component load ~50KB is within bounds. |
| context_load (251KB before first write) | Warning | Covered by #1271. 251KB / 5 components = ~50KB each, proportional. |
| file_breadth (86 files) / mutation_spread (62 files) | Warning | Stored #1272. ~75% of mutations are design artifacts, not code churn. Code-only mutation spread is 7 files across 2 crates, well-contained for 5 components. |

### Recommendation Actions

| Recommendation | Action Taken |
|----------------|-------------|
| Add common build/test commands to settings.json allowlist | Reinforced in lesson #1270. This has been recommended twice (nan-002, col-022) without being applied. Escalating as a persistent issue. |
| Consider incremental compilation or targeted cargo test invocations | Reinforced in lesson #1269 with specific multi-crate command examples. |

### Baseline Outliers

None detected (baselines empty).

### Security Review Follow-ups

All 3 findings are low severity and non-blocking:
1. Duplicate `update_session_keywords` (listener vs store) -- the Store method appears unused. Should be removed or consolidated in a follow-up.
2. `set_feature_force` returns `Set` for unregistered sessions -- misleading return value, consider `SessionNotFound` variant.
3. Keywords stored without JSON validation at listener level -- defense-in-depth is adequate, upstream validation covers it.

---

## Knowledge Stewardship

### Queried
- Searched patterns: MCP tool handler, hook handler dispatch, schema migration, shared validation (4 searches)
- Searched procedures: schema migration, MCP tool addition (1 search)
- Searched lessons: permission retries, compile cycles, cold restart, serialization bugs (3 searches)
- Searched decisions: RecordEvent wire protocol, force-set attribution, JSON column keywords (3 searches)
- Looked up related features: col-022 decisions (1 lookup)
- Read 6 existing entries in full: #763, #620, #619, #604, #1164, #886

### Stored
- 2 new patterns: #1265 (dual-path validation), #1266 (specialized event dispatch)
- 5 ADRs: #1273-#1277 (were file-only from design phase, now in Unimatrix)
- 4 new lessons: #1267 (stewardship enforcement), #1268 (serialization test fidelity), #1271 (context load normalization), #1272 (mutation spread normalization)
- 3 corrections: #619->#1263 (procedure), #620->#1264 (pattern), #1165->#1269 (lesson), #1164->#1270 (lesson) -- 4 total corrections
