# vnc-012 Retrospective Architect Report

**Agent**: vnc-012-retro-architect
**Date**: 2026-03-29
**Feature**: vnc-012 — MCP string-encoded integer coercion

---

## Patterns

| Action | Entry | Title |
|--------|-------|-------|
| Updated | #3784 → #3813 | Custom serde deserializer + schemars schema override for MCP tool param structs (added vnc-012 tag + validation confirmation) |
| Updated | #3786 → #3814 | MCP tool param deserialization fixes require infra-001 transport-level validation (added vnc-012 tag + IT-01/IT-02 confirmation) |
| Verified | #3791 | Use deserialize_option (not deserialize_any) for Option<T> serde visitor helpers |
| Verified | #3792 | serde deserialize_with path must be crate-absolute, not module-relative |
| Verified | #3797 | infra-001: call_tool without format=json returns summary row, not entry content |
| Skipped | — | infra-001 test file structure (covered by #3814 and #3820) |

---

## Procedures

| Action | Entry | Title |
|--------|-------|-------|
| New | #3820 | How to add an integer parameter to an existing MCP tool in unimatrix-server |

Six-step procedure: struct field annotation (required attribute triple), single-pass editing discipline, unit test requirements per field type, schema snapshot assertion, and infra-001 smoke test. References #3792, #3815, #3797.

---

## ADR Status

All four ADRs validated — no supersessions required.

| Entry | ADR | Status | Evidence |
|-------|-----|--------|----------|
| #3787 | ADR-001: serde_util submodule placement | Validated | File at exact path, three `pub(crate)` functions, `mod serde_util;` in mod.rs |
| #3788 | ADR-002: schemars(with) for schema preservation | Validated | `test_schema_integer_type_preserved_for_all_nine_fields` asserts `"type": "integer"` for all 9 fields |
| #3789 | ADR-003: mandatory infra-001 integration test | Validated | IT-01 assertion bug (missing `format=json`) confirmed ADR thesis — unit tests passed but integration test caught semantic gap |
| #3790 | ADR-004: mandatory None-for-absent tests + serde(default) | Validated | All 5 optional fields carry `#[serde(default)]`; R-01 and R-03 PASS |

---

## Lessons

| Entry | Title | Source |
|-------|-------|--------|
| #3815 | Single-file annotation iteration drives compile cycles: batch all field attributes before first build | compile_cycles hotspot (57 cycles) |
| #3816 | Bootstrapping problem: early-session MCP tool calls fail when the session is fixing the tool being used | tool_failure_hotspot (26 failures: 10 store + 16 get) |
| #3818 | Navigating dependency source code (rmcp) to resolve dispatch behaviour adds significant file breadth | file_breadth warning (82 files, OQ-04 rmcp source navigation) |
| Skipped | IT-01 format=json lesson | Already fully captured as pattern #3797 |

---

## Retrospective Findings

**Hotspot actions:**
- `compile_cycles` (57): Stored #3815 — distinct single-file annotation-iteration variant not covered by existing lessons
- `tool_failure_hotspot` (26 failures): Stored #3816 — bootstrapping window where session was fixing the bug causing failures
- `file_breadth` (82 files): Stored #3818 — intentional rmcp source navigation to resolve OQ-04 (distinguishes from aimless browsing)
- `lifespan` (70–76 min): No new entry — run_in_background recommendation already in retro protocol

**Tag corrections:**
- #3784 deprecated → superseded by #3813 (added `vnc-012` tag)
- #3786 deprecated → superseded by #3814 (added `vnc-012` tag)
