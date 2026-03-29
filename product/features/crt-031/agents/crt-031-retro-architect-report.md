# crt-031 Retrospective — Architect Report

> Agent: crt-031-retro-architect (uni-architect)
> Date: 2026-03-29
> Mode: retrospective (post-merge knowledge extraction)

---

## 1. Patterns

### New entries stored

**#3782** — Two independent RwLock fields on a shared struct — separate hot-path and policy locks

`CategoryAllowlist` added a second `RwLock<HashSet<String>>` field (`adaptive`) alongside the existing `categories` field. Each method reads exactly one lock; there is no data dependency between them. The pattern generalizes: when a struct accumulates logically distinct datasets with different access frequencies, split them into independent `RwLock` fields rather than a single `RwLock<(A, B)>`. Constraint: all fields must use the same `.unwrap_or_else(|e| e.into_inner())` poison recovery — no exceptions.

### Already stored (verified accurate, no update needed)

**#3774** — Splitting Default impl from serde default creates silent test failures

Verified against gate-3a and gate-3b reports. The pattern correctly captures the mechanism: `KnowledgeConfig::default()` returning `vec![]` while the serde default fn returns `["lesson-learned"]`. Gate-3a confirmed `test_default_config_boosted_categories_is_lesson_learned` was rewritten to cover the serde path (AC-18), and `test_knowledge_config_default_boosted_is_empty` covers the Default path (AC-17). Entry is accurate.

**#3770** — KnowledgeConfig parallel list fields follow the boosted_categories structural pattern

Verified accurate. `adaptive_categories` mirrors `boosted_categories` in field placement, serde annotation, validate_config cross-check, and merge_configs semantics.

**#3771** — KnowledgeConfig parallel list defaults collide in validate_config test fixtures

Verified accurate. All 27 ACs passed at gate-3c; the R-01 mitigation (zero ALL parallel lists in fixtures with custom `categories`) was correctly applied across the test suite.

### Skipped (existing coverage sufficient)

**Module split pattern** (`categories.rs` → `categories/mod.rs + lifecycle.rs`): Three entries already cover this domain — #3586 (nan-010 ADR: module pre-split as first implementation step), #2618 (oversized eval files: split before delivery), #3778 (test file splitting via pub(super)). The crt-031 split follows #3586's pattern exactly and adds no novel variation. Not stored.

---

## 2. Procedures

### No new procedure stored

The retrospective recommendation "Batch struct field additions before compiling" maps to existing entries #3439 and #3544:

- **#3439**: "Batch structural changes before compiling — multi-file struct extension and bugfixes both benefit from complete-then-compile discipline" (tags: compile-cycles, struct-extension, multi-file)
- **#3544**: "Cascading struct field addition drives compile cycles: complete type definitions before first build" (tags: compile-cycles, struct-field-addition, schema-cascade)

crt-031's compile hotspot (98 cycles, KnowledgeConfig + CategoryAllowlist + StatusService + background function signatures added incrementally) is the same root cause as both entries. crt-031 confirms the lesson but does not extend it. No update needed.

---

## 3. ADR Status

### Validated

**ADR-001 (entry #3775)** — CategoryAllowlist lifecycle policy: constructor hierarchy, config model, status format asymmetry, Default/serde separation.

Validated by successful delivery:
- Gate-3b PASS: all 13 integration-surface entries from ARCHITECTURE.md implemented correctly; constructor hierarchy `new → from_categories → from_categories_with_policy` honored; status format asymmetry (summary adaptive-only, JSON all categories) confirmed in `format_status_report`.
- Gate-3c PASS: 3,470 tests passed / 0 failed; all 27 ACs verified; integration smoke 20/20.
- No ADR supersession required. All four ADR-001 decisions (constructor API, status format asymmetry, validation fail-fast, Default/serde separation) remain accurate and current.

### No ADRs flagged for supersession

crt-031 adds to the CategoryAllowlist without replacing any prior decision. ADR-003 (entry #86, RwLock for CategoryAllowlist) is extended by the second independent lock — not superseded.

---

## 4. Lessons

### New entry stored

**#3783** — context_get id must be a bare integer — quoted strings cause type errors; 38 failures in crt-031

Root cause of hotspot F-05 (38 `context_get` failures, 11 clusters). The `id` and `original_id` parameters are i64 in the MCP schema; passing them as quoted strings (`"3267"` instead of `3267`) produces `-32602: invalid type: string, expected i64`. This is a recurring agent error — also documented in bugfix-439 (13 failures, entry #3728). Entry #3783 covers the prevention rule. Entry #3728 covers the mitigation (context_search fallback). Together they form a complete guard.

### Scope expansion lesson — existing coverage sufficient

The crt-031 retrospecive shows: `boosted_categories` de-hardcoding was added after initial design scope, causing the design-review gate to fail on its first pass (vision alignment + synthesis rework needed). This maps to **entry #2397**: "Incremental Scope Discussion Produces Incomplete First Design Pass" (dsn-001, tags: scope-expansion, design-protocol, rework). The mechanism is identical — scope added mid-session was not fully synthesized into the architecture before the gate ran. Not stored; #2397 is accurate and applicable.

---

## 5. Retrospective Findings

### F-01: compile_cycles (98 cycles, threshold ~30)

- **Root cause:** Fields added to `KnowledgeConfig`, `CategoryAllowlist`, `StatusService`, and four background function signatures in separate compilation passes rather than completing all field definitions before first build.
- **Action taken:** Existing lessons #3439 and #3544 already cover this. No new entry stored.
- **Note for future delivery:** crt-031 touched 7 components across wave 1, 2, and 3. Wave 1 completion before any wave 2 compilation would reduce cycles significantly. The build wave structure in OVERVIEW.md was correct; the compile discipline was not applied.

### F-05: tool_failure_hotspot (context_get 38x, Read 14x, context_store 14x)

- **context_get failures:** Covered by new entry #3783 (integer ID prevention). The Read failures (14x) and context_store failures (14x) are likely transient MCP connection issues consistent with GH #52 — not attributable to agent error. No separate lesson stored for those.

### F-02: context_load (101 KB before first write)

- **Assessment:** crt-031 involved 80 distinct files and a complex multi-component scope (7 components, 70 mutation files). The 101 KB load is proportionate to the problem size. The existing lesson #1563 (crt-018b: excessive context load before first artifact) applies here. The crt-031 load is high but not an outlier given the scope breadth. No new lesson stored.

### Design rework (2 passes, scope expansion)

- **Assessment:** Fully covered by existing #2397. The second design pass (+1h 20m, +354 records) was driven by the boosted_categories de-hardcoding addition. Both the cause (scope added mid-session) and the remedy (re-run vision alignment before gate) are already documented.

---

## Knowledge Stewardship

- Queried: `context_briefing` (19 entries), `context_search` x5 across patterns, procedures, lessons, scope expansion, and MCP tool failure topics.
- Stored: #3782 (pattern — two independent RwLock fields), #3783 (lesson — context_get integer ID prevention).
- Skipped with reason: module split pattern (3 existing entries sufficient), compile cycles procedure (2 existing lessons sufficient), scope expansion lesson (#2397 accurate), serde/Default split (#3774 accurate).
