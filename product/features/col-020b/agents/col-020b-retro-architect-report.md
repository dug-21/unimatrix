# col-020b Retrospective Architect Report

Agent ID: col-020b-retro-architect

## 1. Patterns

### New Entries
| ID | Title | Reason |
|----|-------|--------|
| #923 | Serde Alias for Unidirectional Field Renames in Non-Persisted Types | First use of serde(alias) in codebase. Distinct from #646 (serde(default) for config extension). Reusable whenever renaming fields on computed/transient types. |
| #924 | Parallel Delivery Agents Grouped by File to Eliminate Merge Conflicts | col-020b validated this with 4 agents, 0 conflicts. Generic technique applicable to any multi-file feature. |

### Validated (No Update Needed)
| ID | Title | Assessment |
|----|-------|------------|
| #884 | Server-Side Cross-Table Computation as Scoped Exception to Trait Abstraction | col-020b ADR-004 upheld col-020 ADR-001. Pattern still accurate. FeatureKnowledgeReuse computation stayed server-side per pattern guidance. No drift. |

### Skipped (Not Reusable)
| Component | Reason |
|-----------|--------|
| normalize_tool_name (C1) | Private helper, single consumer, one-off. The `mcp__unimatrix__` prefix is an implementation artifact. If a second consumer appears, promoting to pub(crate) is trivial — no pattern needed. |
| classify_tool curate category (C2) | Additive change to existing match table. No new structural pattern. |
| knowledge_curated counter (C3) | Follows existing knowledge_served/knowledge_stored counter pattern already in the code. |
| Data flow debug tracing (C6) | Standard tracing::debug! usage. Not a novel pattern. |
| Re-export update (C7) | Mechanical rename. No pattern. |

## 2. Procedures

No new or updated procedures. The build/test/integration process did not change. No schema migration was involved (changes were to computed types, not persisted tables). The existing integration test procedure (#840) remains accurate.

## 3. ADR Status

### Stored in Unimatrix (were file-only, now complete)
| ADR | File | Unimatrix ID | Status |
|-----|------|-------------|--------|
| ADR-001 | ADR-001-normalize-tool-name-placement.md | #918 | Validated by implementation. normalize_tool_name is private, single-consumer. |
| ADR-002 | ADR-002-integration-testing-scope.md | #919 | Validated. Rust-only tests sufficient for pure computation bugs. Follow-up integration tests still deferred. |
| ADR-003 | ADR-003-serde-backward-compat-strategy.md | #920 | Validated. All 6 serde backward-compat test scenarios passed. First serde(alias) usage established. |
| ADR-004 | ADR-004-knowledge-reuse-stays-server-side.md | #921 | Validated. Upholds col-020 ADR-001 (#864) and pattern #884. No architectural drift. |
| ADR-005 | ADR-005-issue-193-investigation-boundary.md | #922 | Validated. Scope boundary held — no Store-layer changes made. Debug tracing shipped as designed. |

### Flagged for Supersession
None. All 5 ADRs were validated by successful implementation with zero rework.

## 4. Lessons

| ID | Title | Key Insight |
|----|-------|-------------|
| #925 | Clean first-pass delivery from precise scope decomposition and file-aligned parallelism | SCOPE.md with actual line numbers + 1:1 component-to-file mapping + Integration Surface table + explicit scope boundaries (ADR-002, ADR-005) = zero rework across 3 gates. |

### How #192/#193 Shipped Without Being Caught (Root Cause)
- **#192 (MCP prefix):** col-020 tests used bare tool names (context_search) but production observations contain MCP-prefixed names (mcp__unimatrix__context_search). No test used realistic MCP-prefixed input data. col-020b added MCP-prefixed test inputs as regression tests.
- **#193 (2+ sessions filter):** col-020 tested knowledge reuse with multi-session data only. No test used single-session data, so the "2+ sessions" filter appeared correct. col-020b added single-session regression tests (AC-07, AC-15).
- **Lesson:** When testing observation/metric pipelines, test with realistic input data formats (MCP-prefixed names) AND edge-case cardinalities (single session, zero sessions).

## 5. Retrospective Findings

### Notable Observations
1. **Zero rework is achievable for bugfix features** when the scope is tightly bounded with explicit scope boundaries (ADR-002 deferred integration tests, ADR-005 deferred Store investigation). Both boundaries held during implementation.

2. **File-aligned parallelism eliminated coordination overhead.** 4 agents worked simultaneously with no communication needed between them. The architecture's Integration Surface table served as the shared contract.

3. **The col-020b ADRs were NOT stored in Unimatrix before this retrospective.** They existed only as files in the architecture/ directory. This retrospective stored all 5 (#918-#922). The design protocol should ensure ADR storage happens during design, not retroactively.

4. **Existing pattern #884 (Server-Side Cross-Table Computation) was validated across two features** (col-020 and col-020b). Its "scoped exception" framing held — the computation stayed server-side without pressure to move it.

5. **The serde(alias) pattern (#923) fills a gap** between #646 (serde(default) for additive config fields with persistence) and #364 (retain-and-rename for transitional types). The new pattern covers the specific case of field renames on non-persisted types.
