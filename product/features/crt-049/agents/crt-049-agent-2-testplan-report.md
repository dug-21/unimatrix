# Agent Report: crt-049-agent-2-testplan

**Phase**: Stage 3a — Test Plan Design
**Agent ID**: crt-049-agent-2-testplan

---

## Deliverables

All seven test plan files produced:

- `product/features/crt-049/test-plan/OVERVIEW.md`
- `product/features/crt-049/test-plan/feature-knowledge-reuse.md`
- `product/features/crt-049/test-plan/extract-explicit-read-ids.md`
- `product/features/crt-049/test-plan/compute-knowledge-reuse.md`
- `product/features/crt-049/test-plan/compute-knowledge-reuse-for-sessions.md`
- `product/features/crt-049/test-plan/render-knowledge-reuse.md`
- `product/features/crt-049/test-plan/schema-version-bump.md`

---

## Risk Coverage Summary

| Risk | Priority | AC | Test File | Status |
|------|----------|----|-----------|--------|
| R-01: Triple-alias serde chain silent zero | Critical | AC-02 [GATE] | feature-knowledge-reuse.md | Planned: 5 unit tests |
| R-02: normalize_tool_name omission | High | AC-06 [GATE], AC-12(d) | extract-explicit-read-ids.md | Planned: mandatory prefixed-name test |
| R-03: total_served semantics change | High | AC-14, AC-15, AC-17 [GATE] | compute-knowledge-reuse.md + render | Planned: set-union unit tests + render guard |
| R-04: Cap silently partial | Medium | — | compute-knowledge-reuse-for-sessions.md | Planned: structural check + constant verify |
| R-05: Early-return guard old condition | Medium | AC-09, AC-17 [GATE] | compute-knowledge-reuse.md + render | Planned: zero-exposure, non-zero-reads test |
| R-06: Section-order regression | Medium | AC-07 | render-knowledge-reuse.md | Planned: golden-output test with 5 assertions |
| R-07: attributed not threaded through | Medium | AC-05 | compute-knowledge-reuse-for-sessions.md | Planned: store-backed integration test |
| R-08: SUMMARY_SCHEMA_VERSION not bumped | High | AC-08 | schema-version-bump.md | Planned: CRS-V24-U-01 update to 3 |
| R-09: explicit_read_by_category contract | High | AC-13 [GATE] | compute-knowledge-reuse.md | Planned: category map unit test |
| R-10: Filter-based lookup included | Medium | AC-04, AC-12(b) | extract-explicit-read-ids.md | Planned: no-id exclusion tests |
| R-11: N+1 query pattern | Medium | structural | compute-knowledge-reuse-for-sessions.md | Code review check |
| R-12: Deduplication not applied | Medium | AC-14 [GATE], AC-15 [GATE] | compute-knowledge-reuse.md | Planned: set-union overlap test |
| R-13: Fixture update incomplete | Medium | AC-10 | retrospective.rs fixtures | Compile-time catch |

---

## Integration Suite Plan

**Mandatory**: smoke (`-m smoke`)
**Required**: `lifecycle`, `tools`
**Not required**: security, confidence, contradiction, volume, edge_cases, protocol

One new unit-level integration test in `tools.rs` covers AC-05 (store-backed, not
infra-001 harness). No new infra-001 suites needed.

---

## Expected Test Count Delta

| File | New Tests | Updated Tests |
|------|-----------|---------------|
| `unimatrix-observe/src/types.rs` | +8 | — |
| `unimatrix-server/src/mcp/knowledge_reuse.rs` | +17 | N (field rename: delivery_count → search_exposure_count) |
| `unimatrix-server/src/mcp/tools.rs` | +2 | 1 (signature + field rename) |
| `unimatrix-server/src/mcp/response/retrospective.rs` | +6 | fixtures (compile-time catch) |
| `unimatrix-store/src/cycle_review_index.rs` | 0 | 1 (CRS-V24-U-01 to assert 3) |

---

## Open Questions for Stage 3b

1. **`ObservationRecord` constructor**: What is the minimum required set of fields for
   constructing a synthetic `ObservationRecord` in tests? The `#[cfg(test)]` helper
   `make_obs` needs to know which fields have defaults vs. which are required. Delivery
   agent should check `unimatrix-core/src/observation.rs` for the struct definition.

2. **`render_knowledge_reuse` label for by_category**: The existing label "By category
   (all N served)" refers to `delivery_count`. After the rename, this label is ambiguous
   (it is search-exposure category data). The spec mentions "Search exposure categories"
   as a possible relabeling. The exact new label is not specified in the AC list — delivery
   agent should confirm the exact string to use in the golden-output test.

3. **Test name for CRS-V24-U-01**: The existing test is named
   `test_summary_schema_version_is_two`. Stage 3c should confirm whether any gate tooling
   uses grep patterns that reference this exact name, before renaming it to
   `test_summary_schema_version_is_three`.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found ADRs #4218, #4215, #4216 for
  crt-049 directly; lesson #885 (serde alias gate failures), lesson #3932 (compile cycles),
  pattern #238 (test infrastructure cumulative), pattern #3253 (grep per test name)
- Queried: `context_search("crt-049 architectural decisions", category: "decision")` —
  retrieved all three crt-049 ADRs
- Queried: `context_search("knowledge reuse testing patterns serde alias")` —
  found ADR #920 (col-020b serde alias, backward compat), ADR #4215 (crt-049 triple alias)
- Stored: nothing novel to store — the normalize_tool_name prefix gotcha is already
  captured in pattern #4211; the serde alias gate failure pattern is in lesson #885;
  no new test infrastructure patterns discovered that aren't already in Unimatrix
