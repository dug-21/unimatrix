# Security Review: col-020b-security-reviewer

## Risk Level: low

## Summary

This change is a low-risk bug fix and refactoring of internal retrospective metrics computation. No new external input surfaces, no new dependencies, no secrets, no injection vectors. All data flows are internal (Store records and Claude hook events). The changes are minimal and well-scoped to their stated purpose.

## Findings

### Finding 1: No Input Validation Concerns
- **Severity**: informational
- **Location**: crates/unimatrix-observe/src/session_metrics.rs:207-209
- **Description**: `normalize_tool_name` performs a simple `strip_prefix` on trusted internal data (tool names from Claude's hook system). No user-controlled input reaches this function. The function handles edge cases correctly (empty string, double prefix, unknown prefix -- all pass through without panic).
- **Recommendation**: None needed.
- **Blocking**: no

### Finding 2: Serde Backward Compatibility is Unidirectional
- **Severity**: low
- **Location**: crates/unimatrix-observe/src/types.rs:188-195, 207-215
- **Description**: `serde(alias)` only works for deserialization. Serialized output uses new field names (`knowledge_served`, `delivery_count`, `feature_knowledge_reuse`). Any downstream consumer parsing serialized JSON with old field names would fail silently (field defaults to 0). However, the architecture documents confirm `RetrospectiveReport` is ephemeral MCP tool output with no persistence, making this acceptable. Tests verify both directions.
- **Recommendation**: Documented and tested. No action needed.
- **Blocking**: no

### Finding 3: Error Handling in Data Flow is Adequate
- **Severity**: low
- **Location**: crates/unimatrix-server/src/mcp/tools.rs:1335
- **Description**: `compute_knowledge_reuse_for_sessions` failures are caught with `tracing::warn` and result in `feature_knowledge_reuse: None` in the report. This is a graceful degradation pattern -- the report is incomplete but not invalid, and no panic occurs. The `??` chains on `spawn_blocking` calls correctly propagate JoinErrors and Store errors.
- **Recommendation**: None needed. Debug tracing added at data flow boundaries aids future diagnosis.
- **Blocking**: no

### Finding 4: No New Dependencies
- **Severity**: informational
- **Location**: Cargo.toml (unchanged)
- **Description**: No new crate dependencies are introduced. All changes use existing `serde`, `tracing`, and `std` library features.
- **Recommendation**: None needed.
- **Blocking**: no

### Finding 5: No Hardcoded Secrets
- **Severity**: informational
- **Location**: Full diff (all 52 files)
- **Description**: No API keys, tokens, credentials, or secrets are present in the diff. All new files are documentation, pseudocode, test plans, and agent reports.
- **Recommendation**: None needed.
- **Blocking**: no

### Finding 6: Formatting-Only Changes in tools.rs are Benign
- **Severity**: low
- **Location**: crates/unimatrix-server/src/mcp/tools.rs (multiple locations)
- **Description**: A significant portion of the tools.rs diff is import reordering and code formatting (line wrapping of `.await?` chains, `if` expressions, `format!` macros). These are style changes, likely from rustfmt, with zero behavioral impact. While these expand the diff surface, they introduce no functional risk.
- **Recommendation**: None needed. These are standard formatter output.
- **Blocking**: no

## OWASP Assessment

| Check | Result |
|-------|--------|
| Input validation | No new external inputs. Internal data only. |
| Path traversal | No file path operations added. |
| Injection | No shell commands, SQL, or format strings with untrusted input. |
| Deserialization | Serde deserialization of trusted internal JSON. Aliases and defaults handle backward compat safely. |
| Error handling | Errors logged with tracing, graceful degradation to None. No panics in production paths. |
| Access control | No trust boundary changes. Existing capability checks unchanged. |
| Dependencies | No new dependencies. |
| Secrets | No hardcoded secrets. |

## Blast Radius Assessment

**Worst case**: If `normalize_tool_name` has a subtle bug (e.g., strips too much or too little), the impact is limited to incorrect tool classification and knowledge metric counters in retrospective reports. Specifically:
- `knowledge_served`, `knowledge_stored`, `knowledge_curated` could be incorrect (too high or too low)
- `tool_distribution` could miscategorize tools
- `FeatureKnowledgeReuse.delivery_count` could be wrong

**Impact severity**: Low. These are observability/analytics metrics only. No code execution, no data mutation, no privilege escalation, no denial of service. The retrospective pipeline is a read-only analytics feature that does not affect the core knowledge storage or retrieval paths.

**Failure mode**: Safe. Incorrect metrics produce misleading reports but do not corrupt data or affect system availability.

## Regression Risk

**Low**. The changes are well-contained:
1. `session_metrics.rs` changes are additive (new normalization function, new counter, new category). Existing bare-name matching continues to work because `normalize_tool_name` passes through non-prefixed names unchanged.
2. `types.rs` changes use serde aliases and defaults, preserving backward compatibility with pre-col-020b serialized data.
3. `knowledge_reuse.rs` semantic change (all delivery vs 2+ sessions) is an intentional fix, not a regression. The old behavior was the bug.
4. `tools.rs` changes are formatting + debug tracing + field renames. No behavioral changes to the data flow.
5. Test coverage is comprehensive: 8 normalize edge cases, exhaustive classify_tool coverage, MCP-prefixed integration tests, serde backward compat tests, delivery vs cross-session semantic tests.

## PR Comments
- Posted 1 review comment on PR #195
- Blocking findings: no
