# Test Plan: C6 — Data Flow Debugging

**File:** `crates/unimatrix-server/src/mcp/tools.rs`
**Function:** `compute_knowledge_reuse_for_sessions` (private async)
**Risks:** R-06 (#193 data flow), R-12 (spawn_blocking error swallowing)

## Verification Method

C6 adds `tracing::debug!` instrumentation only. No behavioral change. Verification is by code review, not unit tests.

### Code Review Checklist

1. After loading `session_id_list`: `tracing::debug!("knowledge_reuse: {} session IDs", session_id_list.len())`
2. After loading query_logs: `tracing::debug!("knowledge_reuse: {} query_log records", query_logs.len())`
3. After loading injection_logs: `tracing::debug!("knowledge_reuse: {} injection_log records", injection_logs.len())`
4. After computing result: `tracing::debug!("knowledge_reuse: delivery_count={}, cross_session_count={}", result.delivery_count, result.cross_session_count)`

### Error Handling Review (R-12)

1. Confirm `compute_knowledge_reuse_for_sessions` returns `Result<FeatureKnowledgeReuse>` (not `Option`).
2. Confirm the caller in `tools.rs` handles `Err` with `tracing::warn!` and sets `feature_knowledge_reuse: None` on the report.
3. Confirm no `unwrap()` on `spawn_blocking` JoinHandle.
4. Confirm error path produces `None` (not `Some(FeatureKnowledgeReuse { delivery_count: 0, ... })`), so consumers can distinguish "no data" from "computation failed".

### AC-16 Verification

Grep `crates/unimatrix-server/src/mcp/tools.rs` for `tracing::debug!` calls containing "knowledge_reuse" to confirm all 4 log points are present.

## Risk Coverage

- R-06: Debug tracing enables diagnosis of the #193 data flow issue. The end-to-end path is not unit-testable (requires Store with populated query_log/injection_log). ADR-002 accepts this gap.
- R-12: Code review verifies error propagation path. The `None` vs `Some(zeroed)` distinction is verified by reviewing the caller logic.
