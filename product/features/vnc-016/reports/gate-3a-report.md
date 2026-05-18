# Gate 3a Report: vnc-016

> Gate: 3a (Design Review)
> Date: 2026-05-18
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 5 components match architecture decomposition; boundaries, technology choices, and ADR references are consistent |
| Specification coverage | PASS | All 7 FRs and all 13 ACs have explicit pseudocode and test-plan coverage; no scope additions |
| Risk coverage | PASS | All 10 risks (R-01 through R-10) mapped to test scenarios in test-plan files |
| Interface consistency | PASS | Shared types (`UsageContext`, `feature_entries` column name) defined in OVERVIEW.md and used correctly across all per-component files; no contradictions |
| Knowledge stewardship compliance | PASS | Both active-design agents (pseudocode, test-plan) have `## Knowledge Stewardship` sections with `Queried:` entries; test-plan agent also has a `Stored:` entry (#4452) |

## Detailed Findings

### Architecture Alignment

**Status**: PASS

**Evidence**:

All five components match the architecture decomposition exactly:

- Component 1 (SQL fix at `read.rs:1618`) — pseudocode `sql-fix.md` names the identical file and line, changes only `fe.feature_cycle` → `fe.feature_id`, preserves function signature `pub async fn query_stale_prerequisite_edges_for_cycle(&self, feature_cycle: &str) -> Result<Vec<(u64, u64)>>` unchanged.
- Component 2 (Rust unit test in `read.rs mod tests`) — `rust-unit-test.md` places both tests in the existing `mod tests` block at line 1887, uses `open_test_store` + `write_pool` pattern cited in ARCHITECTURE.md ADR-001 (#4449). References `test_query_graph_edges_returns_rows` (line 2056) as the structural template.
- Component 3 (harness client extension `client.py`) — `harness-client.md` adds `feature_cycle: str | None = None` after `edges` with the `if feature_cycle is not None` guard matching ARCHITECTURE.md §Component 3 exactly. `uds_client.py` explicitly excluded.
- Component 4 (usage gate fix `usage.rs` + `tools.rs`) — `usage-gate-fix.md` identifies 5 `UsageContext` construction sites in `tools.rs` (search ~473, lookup ~609, store ~826, get ~922, briefing ~1594) and sets `write_capable: true` only at the store site after `require_cap(Write)`. Both gate blocks (`record_mcp_usage` lines 207-218, `record_hook_injection` lines 272-283) are replaced — consistent with ARCHITECTURE.md §Production Bug Fix (ADR-002, #4451). Note: the architecture text says "4 sites" but the pseudocode correctly enumerates 5 sites and acknowledges the discrepancy ("grep shows 4 sites... actually 5 occurrences"). This is a copy-editing error in the architecture document, not a design inconsistency; the pseudocode list is correct.
- Component 5 (integration tests `test_tools.py`) — `integration-tests.md` appends to the vnc-015 section (after line 3048/3620 end), uses `server` fixture, unique `cycle_id` + `test_agent_id` per function, `force=True` hard literal, `_seed_observation_sql(db_path, [cycle_id], num_records=20)` — all matching ARCHITECTURE.md §Integration Test Structure.

Technology choices (SQLite via sqlx, Rust `#[tokio::test]`, Python pytest with live MCP server fixture) are consistent with the established workspace conventions. No ADR conflicts.

### Specification Coverage

**Status**: PASS

**Evidence**:

Every functional requirement traces to pseudocode:

- FR-01 (SQL fix) → `pseudocode/sql-fix.md`: exact line, before/after SQL, single-token change, function signature unchanged.
- FR-02 (Rust unit test) → `pseudocode/rust-unit-test.md`: positive test with all three required sub-assertions (`is_ok()`, `len() == 1`, `[0] == (A, B)`); negative companion with `is_empty()` assertion; no `unwrap_or_else(|_| vec![])` anywhere.
- FR-03 (harness client) → `pseudocode/harness-client.md`: modified signature, guard body, serde contract analysis, backward compat analysis.
- FR-04 (UsageContext gate fix) → `pseudocode/usage-gate-fix.md`: struct field added with no `Default`, both gate blocks replaced, all 5 construction sites enumerated with explicit `write_capable` values, two unit tests in `usage.rs mod tests`.
- FR-05 (positive integration test) → `pseudocode/integration-tests.md` Test 1: 9-step scenario matches FR-05.2 steps 1-9 exactly; assertion matches FR-05.3a-d including exact `any(h["rule_name"] == "dependency_on_deprecated" for h in data["hotspots"])`.
- FR-06 (negative integration test) → `pseudocode/integration-tests.md` Test 2: independent `cycle_id` with `vnc016neg-` prefix; two active entries with no stale edge; assertion uses `not any(...)` not `hotspots == []` per FR-06.4b.
- FR-07 (gate unit tests) → `pseudocode/usage-gate-fix.md` Part E: two sync `#[test]` functions for both branches of the gate logic.

Non-functional requirements:

- NFR-01 (backward compat) — verified: `feature_cycle` is keyword-only with `None` default; `uds_client.py` unchanged.
- NFR-02 (no test failures) — test plans specify `cargo test -p unimatrix-store` pass.
- NFR-03 (no new MCP tools or schema changes) — pseudocode confirms; only `client.py` gains a new optional kwarg.
- NFR-04 (no new Python deps) — no import beyond already-present `json`, `uuid`, `sqlite3`.
- NFR-05 (self-contained tests) — unique `cycle_id` and `test_agent_id` per function; own setup steps.
- NFR-06 (fmt/clippy) — `usage-gate-fix.md` Part D notes removal of `trust` variable eliminates dead-variable warning; explicitly flags need to check unused imports.
- NFR-07 (no default for `write_capable`) — pseudocode states "No `Default` impl. No `#[serde(default)]`. No `#[derive(Default)]`."
- NFR-08 (`trust_level` retained) — pseudocode retains field; gate fix narrowly targets only the `feature_recording` block.

No unrequested features appear in any pseudocode file.

### Risk Coverage

**Status**: PASS

**Evidence**:

All 10 risks from the Risk-Based Test Strategy have explicit test scenarios:

| Risk ID | Priority | Covered By |
|---------|----------|-----------|
| R-01 (vacuous pass — feature_entries absent) | Critical | `integration-tests.md` step 4 uses `test_agent_id` (Restricted+Write); OVERVIEW test plan notes positive test must fail against un-fixed code |
| R-02 (vacuous pass — memoized result) | Critical | Both integration tests include `force=True` as hard literal; unique `cycle_id` per test |
| R-03 (Rust assertion structurally always-true) | Critical | `rust-unit-test.md` mandates all three sub-assertions: `is_ok()` + `len() == 1` + `[0] == (A, B)`; prohibits `unwrap_or_else(|_| vec![])` |
| R-04 (negative companion absent) | Critical | `rust-unit-test.md` marks negative companion as "not optional"; defines `test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry` |
| R-05 (unwrap_or_else re-conceals future regression) | High | Both test-plan files explicitly state the Rust unit test is the sole regression guard; integration test also required but insufficient alone |
| R-06 (Restricted agent silently drops feature_entries) | High | `integration-tests.md` step 4 requires `agent_id=test_agent_id` (Restricted+Write), not `"human"`; `usage-gate-fix.md` unit tests confirm gate logic |
| R-07 (cycle_id mismatch → empty-cycle exit) | High | Both integration tests bind `cycle_id` once at top of function; same variable passed to `_seed_observation_sql` and `context_cycle_review` |
| R-08 (negative assertion too broad) | Medium | Negative test asserts `not any(h["rule_name"] == "dependency_on_deprecated" for h in data["hotspots"])` — not `hotspots == []` |
| R-09 (feature_cycle forwarded as null) | Low | `harness-client.md` verifies `if feature_cycle is not None` guard; architecture analysis confirms serde contract |
| R-10 (existing call sites broken by client.py change) | Medium | `harness-client.md` specifies full pytest regression run; `feature_cycle` placed after `edges` (keyword-only) |

Risk priorities are appropriately reflected in test plan emphasis: Critical risks each have dedicated sections in both the pseudocode and test-plan files; the four Critical risks receive the most detailed assertion specifications.

One note: The Risk Strategy (R-06, §Security Risks) states `agent_id="human"` is the requirement for `context_store`, reflecting the pre-Phase-2a understanding. The pseudocode correctly diverges from this by requiring `test_agent_id` (Restricted+Write) — this is the correct design per the architecture document's "Production Bug Fix" section and the corrected test setup in AC-12. The OVERVIEW test plan's risk-to-test mapping table also correctly records `test_agent_id` (not `"human"`) for R-06. This inconsistency is in the Risk Strategy document itself (R-06 §Coverage Requirement says `"human"` but all implementation artifacts correctly use `test_agent_id`), which was written before the gate-fix design was fully resolved. The pseudocode and test plans correctly implement the architecture; the risk strategy text is a stale snapshot and not a blocker.

### Interface Consistency

**Status**: PASS

**Evidence**:

Shared types defined in `pseudocode/OVERVIEW.md` are used correctly across all per-component files:

1. `UsageContext` struct — OVERVIEW defines the 8-field struct with `write_capable: bool` as the last field. `usage-gate-fix.md` Part A reproduces the same field order. All 5 construction sites in Part D set `write_capable` explicitly. No field omissions or contradictions.

2. `feature_entries` table — OVERVIEW explicitly states "Column name is `feature_id` — NOT `feature_cycle`." `sql-fix.md` changes `fe.feature_cycle` → `fe.feature_id` in the SQL. `rust-unit-test.md` seeds `feature_entries` with column `feature_id`. `integration-tests.md` calls `context_store(feature_cycle=cycle_id)` which routes through the analytics write path to `feature_id`. All consistent.

3. Function signature `query_stale_prerequisite_edges_for_cycle(&self, feature_cycle: &str) -> Result<Vec<(u64, u64)>>` — defined in ARCHITECTURE.md and repeated identically in `sql-fix.md`, `rust-unit-test.md`, and `harness-client.md` data flow description.

4. Test function names — OVERVIEW ARCHITECTURE.md table lists `test_query_stale_prerequisite_edges_for_cycle_returns_pair` and `test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry`. These match exactly in `rust-unit-test.md`. Integration test names `test_dependency_on_deprecated_e2e` and `test_dependency_on_deprecated_no_finding_without_stale_edge` match exactly in `integration-tests.md`.

5. Wave sequencing — OVERVIEW states SQL fix must precede Rust tests; harness client must precede integration tests; usage gate fix must precede integration tests. `test-plan/OVERVIEW.md` cross-component dependency section states the same ordering. No contradictions.

Data flow diagrams in OVERVIEW.md (`pseudocode/`) and ARCHITECTURE.md are consistent end-to-end.

### Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

Pseudocode agent (`vnc-016-agent-1-pseudocode`):
- `## Knowledge Stewardship` section present.
- `Queried:` entries: `mcp__unimatrix__context_briefing` (returned #4451, #4450), `context_search(category=pattern)` for SQLite patterns (found #4445, #2745). Evidence of pre-implementation queries per read-only agent obligation.
- No `Stored:` required (read-only agent); no store claimed.

Test-plan agent (`vnc-016-agent-2-testplan`):
- `## Knowledge Stewardship` section present.
- `Queried:` entries: `mcp__unimatrix__context_briefing` (returned #4449, #4445, #4451), two `context_search` calls.
- `Stored:` entry #4452 "Gate-fix integration tests must use an agent from the previously-broken trust/capability class" via `/uni-store-pattern`. This is a genuinely novel cross-feature pattern (distinct from #4416 which covers rejection tests). Store is justified.

Risk agent (`vnc-016-agent-3-risk`) for completeness:
- `## Knowledge Stewardship` section present.
- `Queried:` entries: four `/uni-knowledge-search` calls with full content retrieval via `context_get`.
- `Stored: nothing novel to store -- #4445 already captures the SQL column alias / unwrap_or_else pattern; trust-level + feature_entries pattern is ADR-007 specific`. Reason provided. Compliant.

All read-only agents have `Queried:` entries. No agent is missing the stewardship block.

## Rework Required

None.

## Knowledge Stewardship

- Stored: nothing novel to store -- gate-3a results for vnc-016 are feature-specific; the test-plan agent (entry #4452) already stored the cross-feature pattern discovered during this phase. No additional generalizable gate-failure patterns observed.
