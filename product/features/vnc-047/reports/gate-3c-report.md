# Gate 3c Report: vnc-047

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-07-09
> Result: **PASS**

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | R-01…R-16 each map to passing test(s) at the required tier; verified against actual test bodies, not report claims |
| Test coverage completeness | PASS | All Phase-2 risk-to-scenario mappings exercised; assembled + integration + concurrency covered |
| Specification compliance | PASS | AC-01…AC-09 + AC-EXTRA-1…4 all verified; gating ACs proven, AC-09 non-gating confirmed present |
| Architecture compliance | PASS | Single hook-path writer, whole-set-once EXISTS guard under BEGIN IMMEDIATE, two independent version cascades, GC-by-omission — all as designed |
| Integration test validation | PASS | Smoke 35/35 (mandatory gate); protocol/security/tools/lifecycle run; xfails GH-tracked; no deletions |
| Knowledge stewardship | PASS | Tester report carries `## Knowledge Stewardship` with Queried + Stored entries |

**Verification method**: Every gate-critical claim in RISK-COVERAGE-REPORT.md was checked by reading the actual test bodies and source (two independent read-only sweeps), not accepted on the report's word.

## Detailed Findings

### Check 1 — Risk mitigation proof
**Status**: PASS
**Evidence**: All 16 risks have passing coverage; the five gate-critical obligations are PROVEN in code:

1. **Assembled-path (R-03/SR-08, AC-02/AC-05)** — `test_cycle_start_tags_flow_from_hook_to_cycle_tags` (listener.rs:8091) drives the real seam `drive_cycle_event → dispatch_request(HookRequest::RecordEvent) → handle_cycle_event`, then reads back via `store.get_cycle_tags` (`assert_eq!(tags, ["arm:A","workflow:v1.3"])`). It does NOT call `insert_cycle_start_with_tags` directly. `test_review_surfaces_tags_json_and_markdown_assembled` (listener.rs:8388) drives `populate_review_tags` (real DB seam) + `format_retrospective_markdown`, asserting BOTH `md.contains("## Tags")` + tag lines AND JSON `"tags"` + values. Neither uses a hand-built literal.

2. **Two version cascades (R-01/R-02/SR-01)** — v31: `CURRENT_SCHEMA_VERSION=31` (migration.rs:26) with discrete per-path tests (fresh-create, migration, idempotent re-run, populated-v30-intact, fresh-vs-migration DDL-identical) in migration_v30_to_v31.rs + exact pin `test_schema_version_is_31` (sqlite_parity.rs:1055). SUMMARY v6: `SUMMARY_SCHEMA_VERSION=6` (cycle_review_index.rs:58), pinned `test_summary_schema_version_is_6`, plus mandatory backward-read `test_v5_blob_deserializes_tags_default_empty` (types.rs:1171) confirming a v5 blob → empty vec via `#[serde(default)]` (field at types.rs:442, no `skip_serializing_if`).

3. **GC non-vacuous (R-07/SR-09)** — `test_gc_protected_tables_regression` (retention.rs:522) seeds 4 `cycle_tags` rows, runs BOTH `gc_cycle_activity` AND `gc_unattributed_activity`, asserts `cycle_tags` count unchanged, with POSITIVE CONTROLS asserting 3 sessions purged (2 cycle-activity, 1 unattributed).

4. **Absent/evicted session (R-04/#519)** — `test_evicted_session_tags_persist` (listener.rs:8339) drives assembled with `register=false`; `assert_eq!(get_cycle_tags, ["arm:A"])`.

5. **Whole-set-once EXACT equality + concurrency (R-08/R-15/SR-05)** — store + assembled tests assert EXACT `assert_eq` of the stored set across changed/subset/superset/different re-starts and tagless-does-not-lock. `test_concurrent_same_cycle_starts_one_whole_set` (cycle_tags.rs:310, multi-thread) fires two concurrent same-cycle starts {A,B}/{C,D} and asserts `stored == {A,B} || stored == {C,D}` — never a merge/partial. `BEGIN IMMEDIATE` literally present in `insert_cycle_start_with_tags` (db.rs:409).

### Check 2 — Test coverage completeness
**Status**: PASS
**Evidence**: Every risk-to-scenario mapping from the Risk-Based Test Strategy is exercised. Single-writer confirmed: the only production `INSERT INTO cycle_tags` is db.rs:457 inside `insert_cycle_start_with_tags` (the other occurrence, retention.rs:590, is a test seed). Markdown no-spurious-section proven by `test_render_no_spurious_section_when_empty` (retrospective.rs:4387): `!text.contains("## Tags")` when tags empty. C12 ack echo + C13 freeze trace present (R-16) — confirmed non-gating.

### Check 3 — Specification compliance
**Status**: PASS
**Evidence**: All gating ACs verified with cited tests (see RISK-COVERAGE-REPORT §Acceptance Criteria Verification, independently spot-checked). AC-EXTRA-1 (no second route) proven by `test_bare_mcp_cycle_tags_not_persisted` (Python, test_lifecycle.py:5683) + single-writer grep. AC-06 (additive param, no new tool) proven by `test_context_cycle_accepts_tags_param` + confirmed no new MCP tool registered in the diff. AC-EXTRA-4 (SR-02 re-verification) recorded at Stage 3c start (v31/SUMMARY v6 free at HEAD). AC-09 present but correctly non-gating.

### Check 4 — Architecture compliance
**Status**: PASS
**Evidence**: Implementation matches ARCHITECTURE.md C1–C13: source-of-truth junction, hook-only persistence, BEGIN IMMEDIATE whole-set-once guard, `summary_json`-riding review field, GC-by-omission with regression extension. No architectural drift.

### Check 5 — Integration test validation
**Status**: PASS
**Evidence**:
- **Smoke (mandatory gate)**: 35 passed / 0 failed.
- **Suites run**: protocol 14/0, security 26/0, tools 226 pass + 2 xfail, lifecycle 102 pass + 6 xfail + 1 xpass. Total 368 passed, 0 hard failures.
- **New vnc-047 Python tests exist and assert as claimed**: `test_context_cycle_accepts_tags_param` (test_tools.py:5791), `test_context_cycle_ack_echoes_tags` (test_tools.py:5804, non-gating), `test_bare_mcp_cycle_tags_not_persisted` (test_lifecycle.py:5683, handles both the `-32010`-error and tag-less-success outcomes, fails only if tag strings surface).
- **xfail hygiene**: `test_context_edge_tool_registered` (test_tools.py:3325) marked xfail referencing **GH#942** — confirmed OPEN, title matches, tool count 14→15 drift from vnc-045's `context_tag`. **Confirmed genuinely unrelated to vnc-047: vnc-047 registers NO new MCP tool** (diff verified). All other xfails pre-existing (GH#405/#111/#291/#406 cited).
- **No deletions**: `git diff main...feature/vnc-047` on suites is additive only (78 + insertions, 0 deletions); no test function removed or commented out.

### Check 6 — Knowledge stewardship compliance
**Status**: PASS
**Evidence**: `product/features/vnc-047/agents/vnc-047-agent-6-tester-report.md` contains a `## Knowledge Stewardship` section with a `Queried:` entry (context_briefing — testing-infra patterns #748/#750/#4452) and a `Stored:` entry (/uni-store-procedure — chunking slow infra-001 pytest suites under the 10-min foreground ceiling).

## Warnings (non-blocking)

| # | Item | Assessment |
|---|------|------------|
| W1 | Two lifecycle xfails (test_lifecycle.py:2140, :2199, "No embedding model in CI") cite no GH number | Pre-existing CI-environment xfails, not introduced by vnc-047 and untouched by it. Not a vnc-047 defect. Recommend the team backfill a tracking issue during a future infra sweep — does not block this gate. |
| W2 | In-file schema pin is `test_current_schema_version_is_at_least_31` (`>=`) | Cosmetic. The constant is exactly 31 and `sqlite_parity::test_schema_version_is_31` pins the exact value, so the version is authoritatively locked. No action required. |

## Rework Required

None.

## Scope Concerns

None.
