# Security Review: crt-018-security-reviewer

## Risk Level: low

## Summary

crt-018 adds read-only effectiveness analysis to `context_status`. The feature is well-scoped: no new MCP tool parameters, no external input surfaces, no schema migration, no writes, and no new dependencies. All SQL queries use parameterized statements. The code follows established patterns in the codebase with proper error handling and graceful degradation. Two minor non-blocking findings identified.

## Findings

### Finding 1: Markdown Table Injection via Entry Titles (Mitigated)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/response/status.rs:583,596,606`
- **Description**: Entry titles rendered in markdown tables could contain pipe characters or newlines that break table formatting. The code already sanitizes titles via `.replace('|', "/").replace('\n', " ")` in all three entry list sections (ineffective, noisy, unmatched).
- **Recommendation**: No action needed. The mitigation is already in place and covers all rendering paths.
- **Blocking**: no

### Finding 2: Two Separate lock_conn() Calls in Phase 8

- **Severity**: low
- **Location**: `crates/unimatrix-store/src/read.rs:871,978` and `crates/unimatrix-server/src/services/status.rs:528-529`
- **Description**: `compute_effectiveness_aggregates()` and `load_entry_classification_meta()` each acquire their own connection lock. Between the two calls, an entry could theoretically be deleted, producing injection stats for an entry_id not present in the metadata. The integration code at `status.rs:551` handles this gracefully: entries not found in `stats_map` default to zero injection counts. However, the reverse case (entry deleted between calls, orphan in injection stats) means a classified entry could reference a now-deleted entry. This is a data freshness issue, not a security issue.
- **Recommendation**: Acknowledged in RISK-TEST-STRATEGY.md (integration risk section). The impact is cosmetic at worst (a stale entry appears in the report for one cycle). No action needed.
- **Blocking**: no

### Finding 3: NaN/Infinity Guard in Utility and Aggregate Computation

- **Severity**: low
- **Location**: `crates/unimatrix-engine/src/effectiveness/mod.rs:112-121,223-228`
- **Description**: `utility_score()` guards against division by zero (returns 0.0 when total=0). `aggregate_by_source()` guards against empty injection set (returns 0.0 when no injected entries). Both are correct. No NaN or infinity values can reach the JSON serializer (serde_json rejects NaN).
- **Recommendation**: No action needed. Guards are in place.
- **Blocking**: no

### Finding 4: Case-Sensitive Noisy Trust Source Matching

- **Severity**: low
- **Location**: `crates/unimatrix-engine/src/effectiveness/mod.rs:159`
- **Description**: `noisy_trust_sources.contains(&trust_source)` is a case-sensitive comparison. If `trust_source` is stored as "Auto" or "AUTO", it would bypass Noisy classification. The RISK-TEST-STRATEGY.md (R-10) acknowledges this. The store's SQL uses `COALESCE(trust_source, '')` without case normalization.
- **Recommendation**: Verify that trust_source values in the database are consistently lowercase (the store layer should enforce this on write). Low risk given this is an internal analytical feature. Not blocking.
- **Blocking**: no

## Blast Radius Assessment

**Worst case**: If `compute_effectiveness_aggregates()` returns corrupted data (e.g., inflated injection counts), agents could receive misleading effectiveness classifications in `context_status` output. This is informational only -- no automated actions are triggered by effectiveness data (SR-04 explicitly forbids it). The feature is purely diagnostic.

**Failure mode**: If Phase 8 panics or errors, `effectiveness = None` and the rest of the StatusReport is unaffected. This matches the existing graceful degradation pattern used by the contradiction scan (Phase 4). The worst outcome of a bug is missing or incorrect informational text in status output.

**No data corruption risk**: All code paths are SELECT-only. No writes to any table.

## Regression Risk

- **StatusReport JSON serialization**: The new `effectiveness` field uses `#[serde(skip_serializing_if = "Option::is_none")]`, so existing JSON consumers see no change when effectiveness is None. When present, it adds a new top-level key. Consumers using strict schemas would need updating, but the skip_serializing_if annotation means backwards compatibility is preserved for the common case (no injection data yet).
- **context_status latency**: Phase 8 adds SQL queries. The queries are contained within a single `spawn_blocking` and do not block other phases. At scale (500 entries, 10K injection rows), the risk strategy identifies a 500ms budget. This is additive to existing status computation time.
- **Existing test fixtures**: 8 existing test files set `effectiveness: None` in StatusReport construction, confirming the field integrates cleanly with existing code.

## PR Comments

- Posted 1 comment on PR #207
- Blocking findings: no
