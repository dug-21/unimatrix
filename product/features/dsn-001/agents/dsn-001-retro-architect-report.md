# Retrospective Architect Report: dsn-001

Agent ID: dsn-001-retro-architect
Mode: retrospective
Date: 2026-03-19

---

## 1. Patterns

### New Entries Stored

| ID | Title | Category | Rationale |
|----|-------|----------|-----------|
| #2395 | Two-Level TOML Config Merge: Global + Per-Project with Replace Semantics | pattern | First complete config externalization in the codebase; generalizes to any multi-level config scenario |
| #2396 | Option<T> for Optional Config Fields: Type-Level Absence vs. Zero Distinction | pattern | Concrete technique used in dsn-001 to solve merge false-positive problem; widely applicable |

### Already Stored (Pre-Existing, Confirmed)

| ID | Title | Status |
|----|-------|--------|
| #2325 | Tool rename blast radius: build passing is not sufficient | Validated by dsn-001 gate-3c |
| #2328 | Startup Config Injection: Non-Fatal Fallback with Arc<ConfidenceParams> Threading | Validated by dsn-001 |
| #2313 | config.rs: cross-level custom preset prohibition enforced at per-file validation | Validated by dsn-001 |

### Skipped (Not Reusable)

| Component | Reason |
|-----------|--------|
| `CategoryAllowlist::from_categories()` constructor split | One-off refactor to add config-driven constructor; pattern is just "add a constructor that takes Vec<T>" — too obvious to store |
| `SearchService` boosted_categories HashSet | Feature-specific; the general pattern (HashSet field replacing hardcoded comparisons) is implicit in Rust conventions |
| `AgentRegistry` permissive flag threading | Feature-specific configuration threading; not a novel pattern |

---

## 2. Procedures

### New Procedures

None. The multi-file rename process (#2340 lesson already covers grep-tree-separately) does not constitute a new reusable procedure — the existing lesson is sufficient. The two-level config merge is captured as a pattern (#2395), not a procedure, because it is a design structure rather than a step-by-step how-to.

### Existing Procedures Referenced

- #487 "How to run workspace tests without hanging" — referenced by tester agent

---

## 3. ADR Status

### Validated by Successful Implementation (gate-3c PASS)

| Unimatrix ID | ADR | Status |
|-------------|-----|--------|
| #2284 | ADR-001: ConfidenceParams Struct Extended with Six Weight Fields | Validated |
| #2285 | ADR-002: Config Type Placement — unimatrix-server owns UnimatrixConfig | Validated |
| #2286 | ADR-003: Two-Level Config Merge — Replace Semantics | Validated |
| #2287 | ADR-004: [confidence] Section Promoted from Stub to Live | Validated |

All four were confirmed by gate-3c source audit. No supersession required.

### Newly Stored (Were File-Only)

| Unimatrix ID | ADR | File |
|-------------|-----|------|
| #2393 | ADR-005: Preset Enum Design and Weight Table | `architecture/ADR-005-preset-enum-and-weights.md` |
| #2394 | ADR-006: Preset Resolution Pipeline — Single Site for ConfidenceParams | `architecture/ADR-006-preset-resolution-pipeline.md` |

Both were MCP-unavailable during the second design pass. Now complete.

### Flagged for Supersession

None. No prior ADRs are invalidated by dsn-001 decisions.

---

## 4. Lessons

### New Entries Stored

| ID | Title | Trigger |
|----|-------|---------|
| #2397 | Incremental Scope Discussion Produces Incomplete First Design Pass | Design run twice due to preset system added post-first-pass |
| #2398 | API Extension Gap: New Struct Fields Not Propagated to All Call Sites | GH#311: ConfidenceParams not propagated to inline serving-path call sites |
| #2399 | bash-for-search at 8.3σ in dsn-001: Spawn Prompt Reinforcement Required | 838 search-via-Bash calls; reinforces #1371 with severity context |
| #2400 | 186 Compile Cycles in dsn-001: Use cargo test -p, Not Full Workspace, Per Change | Reinforces #1269 with wave-based compile checkpoint approach |

---

## 5. Retrospective Findings: Hotspot-Derived Actions

| Hotspot | Severity | Action Taken |
|---------|----------|-------------|
| bash_for_search (8.3σ) | Extreme | Lesson #2399 stored. Root cause: spawn prompts don't enforce Grep/Glob. Spawn prompt fix still needed in agent definitions. |
| compile_cycles (186) | Warning | Lesson #2400 stored. Wave-based compile checkpoint discipline documented. |
| context_load (358 KB before first write) | Warning | No action — inherent to design sessions that must read architecture + source before producing artifacts. |
| mutation_spread (99 files, 11 clusters) | Warning | Driven by rename blast radius; covered by #2325 and #2340. |
| permission_retries (4 on context_store) | Warning | MCP connection instability during session; caused ADR-005/ADR-006 to be file-only. Resolved by storing now. |
| edit_bloat (8375 KB, 6.4σ) | Info | Caused by full design re-run after scope expansion. Covered by lesson #2397. |
| follow_up_issues (5, 4.5σ) | Info | GH#311 (call site gap) → lesson #2398. GH#313 (pre-existing runtime panic) → no action. Others are enhancement/harness work. |
| sleep_workarounds (2) | Info | No action — too minor; existing conventions cover run_in_background. |

---

## Open Follow-Up Issues Filed During dsn-001

| Issue | Description | Action |
|-------|-------------|--------|
| GH#311 | ConfidenceParams not propagated to inline serving-path call sites | Lesson #2398 stored; issue remains open for delivery |
| GH#313 | context_cycle_review tokio runtime panic (pre-existing) | Pre-existing; no architectural action |
| GH#308–#310, #312 | Other follow-ups filed during session | Feature-specific; no architectural pattern extracted |
