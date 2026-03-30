# Pseudocode: tools.rs Handler Modifications

## Purpose

Modify the `context_cycle_review` handler in
`crates/unimatrix-server/src/mcp/tools.rs` to:
1. Add `force: Option<bool>` to `RetrospectiveParams`
2. Insert step 2.5 — memoization check (after three-path load, before step 4)
3. Insert step 8a — memoization store (after full pipeline, before audit)
4. Handle force=true with purged signals path
5. Extract helper functions to stay within 500-line file guideline (C-10, NFR-08)

Imports needed in tools.rs:
```
use unimatrix_store::cycle_review_index::{CycleReviewRecord, SUMMARY_SCHEMA_VERSION};
use unimatrix_observe::RetrospectiveReport;
// serde_json already used in the file for other purposes
```

---

## Modified: RetrospectiveParams

```
/// Parameters for the context_cycle_review tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RetrospectiveParams {
    /// Feature cycle to analyze (e.g., "col-002").
    pub feature_cycle: String,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Maximum evidence items per hotspot (default: 3, JSON path only). (col-010b)
    pub evidence_limit: Option<usize>,
    /// Output format: "markdown" (default) or "json". (vnc-011)
    pub format: Option<String>,
    /// Force recomputation even if a stored review exists. (crt-033)
    /// Absent or None is equivalent to false.
    pub force: Option<bool>,
}
```

No validation needed for `force` — `Option<bool>` with absent = None = false.
The existing `validate_retrospective_params` validates `feature_cycle` length
and emptiness; `force` is not subject to those checks.

---

## Handler Skeleton with New Steps

The existing handler steps are referenced by their current labels. Only the
new steps are written in full below. Existing code is unchanged except for
the positional insertion of steps 2.5 and 8a.

```
async fn context_cycle_review(...) -> Result<CallToolResult, ErrorData>:

    // Step 1: identity resolution (UNCHANGED)
    // Step 2: validate params (UNCHANGED)

    // Step 3: three-path observation load → attributed: Vec<ObservationRecord>
    // (UNCHANGED — full spawn_blocking_with_timeout block as-is)
    let (attributed, obs_path_label) = ...

    attribution_path_label = Some(obs_path_label)

    // -----------------------------------------------------------------------
    // Step 2.5 (NEW): Memoization check / force=true purged-signals gate
    // Executes AFTER step 3, BEFORE step 4 (is_empty check on attributed).
    // -----------------------------------------------------------------------
    let force = params.force.unwrap_or(false)

    if !force {
        // Normal path: check for a stored review before any computation.
        match store.get_cycle_review(&feature_cycle).await {
            Ok(Some(record)) => {
                // Memoization hit — return stored record (skip steps 4–8a).
                return handle_memoization_hit(record, &params)
                    .map_err(rmcp::ErrorData::from)
            }
            Ok(None) => {
                // No stored review — fall through to full pipeline.
            }
            Err(e) => {
                // Read error — treat as a cache miss (ADR-003, RISK-TEST-STRATEGY).
                // Do NOT abort; log and proceed to full pipeline.
                tracing::warn!(
                    "crt-033: get_cycle_review read error for {}: {} — treating as miss",
                    feature_cycle, e
                )
                // Fall through to full pipeline.
            }
        }
    } else if attributed.is_empty() {
        // force=true AND attributed is empty:
        // Sole discriminator is get_cycle_review() return value (OQ-01, FR-05/FR-06).
        // No cycle_events COUNT query.
        match store.get_cycle_review(&feature_cycle).await {
            Ok(Some(record)) => {
                // Stored record exists: signals were purged after review was written.
                // Return stored record with purged-signals note.
                return handle_purged_signals_hit(record, &params)
                    .map_err(rmcp::ErrorData::from)
            }
            Ok(None) => {
                // No stored record: return ERROR_NO_OBSERVATION_DATA (FR-06).
                return Err(rmcp::model::ErrorData::new(
                    ERROR_NO_OBSERVATION_DATA,
                    format!(
                        "No observation data found for feature '{}'. \
                         Ensure hook scripts are installed and sessions have been run.",
                        feature_cycle
                    ),
                    None,
                ))
            }
            Err(e) => {
                // Read error with force=true and empty attributed:
                // Cannot distinguish purged from never-existed.
                // Return ERROR_NO_OBSERVATION_DATA (safest response).
                tracing::warn!(
                    "crt-033: get_cycle_review read error (force=true, empty attributed) \
                     for {}: {}", feature_cycle, e
                )
                return Err(rmcp::model::ErrorData::new(
                    ERROR_NO_OBSERVATION_DATA,
                    format!(
                        "No observation data found for feature '{}'. \
                         Ensure hook scripts are installed and sessions have been run.",
                        feature_cycle
                    ),
                    None,
                ))
            }
        }
    }
    // If force=true AND attributed is non-empty: skip step 2.5 check entirely,
    // fall through to step 4 and full pipeline (FR-04).

    // Step 4: if attributed.is_empty() → existing MetricVector cache path (UNCHANGED)
    // NOTE: This branch is only reached when force=false AND no stored record exists,
    // OR when force=true AND attributed is non-empty.
    // The force=true + empty attributed path has already returned above.
    if attributed.is_empty() {
        // ... existing get_metrics / ERROR_NO_OBSERVATION_DATA path (UNCHANGED) ...
    }

    // Steps 5–8: full computation pipeline (UNCHANGED)
    // 7a. list_all_metrics for baseline
    // 7b. history slice
    // 7c. detect_hotspots, compute_metric_vector
    // 8. store_metrics (analytics queue — UNCHANGED)
    // 9. Cleanup expired observations
    // 10a. compute_baselines
    // 10b. drain entries_analysis
    // 10c. build_report
    // 10d. recommendations
    // 10e. narratives
    // 10f. lesson-learned spawn
    // 10g. phase narrative
    // 10h. PhaseStats
    // 10i. goal, cycle_type, is_in_progress, attribution_path

    // -----------------------------------------------------------------------
    // Step 8a (NEW): Serialize and store the computed review.
    // Executes AFTER the full pipeline (after step 10i), BEFORE step 11 (audit).
    // evidence_limit truncation is NOT applied here — the full report is stored.
    // -----------------------------------------------------------------------
    match build_cycle_review_record(&feature_cycle, &report) {
        Ok(record) => {
            // Direct await in the handler's async context.
            // MUST NOT use spawn_blocking (ADR-001, entries #2266, #2249).
            if let Err(e) = store.store_cycle_review(&record).await {
                // Write failure: propagate as tool error (NFR-03, failure modes table).
                // Do not panic; do not swallow silently (GH #409 gate depends on the row).
                return Err(rmcp::model::ErrorData::new(
                    crate::error::ERROR_INTERNAL,
                    format!(
                        "Failed to store cycle review for '{}': {}",
                        feature_cycle, e
                    ),
                    None,
                ))
            }
        }
        Err(e) => {
            // serde_json::to_string failed (non-serializable field — should not occur
            // after serde audit, but propagate as tool error rather than panic).
            return Err(rmcp::model::ErrorData::new(
                crate::error::ERROR_INTERNAL,
                format!(
                    "Failed to serialize cycle review for '{}': {}",
                    feature_cycle, e
                ),
                None,
            ))
        }
    }

    // Step 11: Audit (UNCHANGED)
    // Step 12 / format dispatch (UNCHANGED — evidence_limit applied here at render time)
```

---

## Helper Function: handle_memoization_hit

```
/// Handle a memoization hit (stored record exists, force=false).
///
/// Deserializes the stored summary_json into a RetrospectiveReport.
/// On deserialization failure: falls through to full recomputation with a
/// tracing warning (ADR-003 defense-in-depth).
///
/// Appends schema version advisory to the response when stored schema_version
/// differs from SUMMARY_SCHEMA_VERSION (FR-02, C-05).
///
/// evidence_limit truncation is applied here at render time, AFTER deserialization,
/// NEVER before storage (C-03, FR-08).
fn handle_memoization_hit(
    record: CycleReviewRecord,
    params: &RetrospectiveParams,
) -> Result<CallToolResult, ServerError>

BODY:
    // Check for schema version mismatch.
    let advisory: Option<String> = if record.schema_version != SUMMARY_SCHEMA_VERSION {
        Some(format!(
            "computed with schema_version {}, current is {} — use force=true to recompute.",
            record.schema_version, SUMMARY_SCHEMA_VERSION
        ))
    } else {
        None
    }

    // Deserialize stored report.
    let report: RetrospectiveReport = match serde_json::from_str(&record.summary_json) {
        Ok(r) => r,
        Err(e) => {
            // ADR-003: deserialization failure falls through to full pipeline.
            // Caller (the handler) must catch this error and continue computation
            // rather than returning an error to the MCP client.
            tracing::warn!(
                "crt-033: deserialization of stored summary_json failed for {}: {} \
                 — falling through to full recomputation",
                record.feature_cycle, e
            )
            // Signal to the caller to fall through by returning a specific error.
            // Implementation: return Err(ServerError::MemoizationDeserError)
            // which the handler catches and treats as a miss.
            return Err(ServerError::MemoizationDeserError(e.to_string()))
        }
    }

    // Apply evidence_limit at render time (C-03).
    // This mirrors the existing step 12 format dispatch logic.
    let format = params.format.as_deref().unwrap_or("markdown")
    let result = dispatch_format_with_advisory(report, format, params.evidence_limit, advisory)?

    Ok(result)
```

Note: `ServerError::MemoizationDeserError` is a new variant needed in
`crates/unimatrix-server/src/error.rs`. When the handler receives this error
from `handle_memoization_hit`, it continues to the full pipeline rather than
returning to the caller.

Alternative implementation (to avoid a new error variant): return a
`Result<Option<CallToolResult>, ServerError>` where `None` means "fall through".
Either approach is valid; the implementation agent should choose based on the
existing error convention in tools.rs.

---

## Helper Function: handle_purged_signals_hit

```
/// Handle force=true with empty attributed observations AND a stored record.
///
/// The stored record is returned as-is with an explanatory note.
/// raw_signals_available in the response is set from the stored record's field.
/// The stored record itself is NOT updated by this call (FR-05).
///
/// evidence_limit truncation is applied at render time.
fn handle_purged_signals_hit(
    record: CycleReviewRecord,
    params: &RetrospectiveParams,
) -> Result<CallToolResult, ServerError>

BODY:
    // Format the computed_at timestamp for the note.
    let computed_at_display = format_unix_timestamp(record.computed_at)
    // e.g., "2026-01-15T10:30:00Z" or just the raw seconds value

    let note = format!(
        "Raw signals have been purged; returning stored record from {}.",
        computed_at_display
    )

    // Deserialize stored report.
    let report: RetrospectiveReport = match serde_json::from_str(&record.summary_json) {
        Ok(r) => r,
        Err(e) => {
            // Even on purged path, a corrupt stored record means we cannot help.
            // Return a tool error (not the full pipeline, since signals are gone).
            tracing::warn!(
                "crt-033: deserialization of stored summary_json failed (purged path) \
                 for {}: {}", record.feature_cycle, e
            )
            return Err(ServerError::Core(CoreError::Store(
                StoreError::Deserialization(e.to_string())
            )))
        }
    }

    // Apply evidence_limit at render time.
    let format = params.format.as_deref().unwrap_or("markdown")
    let result = dispatch_format_with_advisory(report, format, params.evidence_limit, Some(note))?

    Ok(result)
```

---

## Helper Function: build_cycle_review_record

```
/// Serialize a RetrospectiveReport into a CycleReviewRecord for storage.
///
/// 4MB ceiling enforcement is delegated to store_cycle_review() (NFR-03).
/// evidence_limit truncation MUST NOT be applied before this call (C-03).
/// SUMMARY_SCHEMA_VERSION is imported from unimatrix_store::cycle_review_index.
fn build_cycle_review_record(
    feature_cycle: &str,
    report: &RetrospectiveReport,
) -> Result<CycleReviewRecord, serde_json::Error>

BODY:
    // Serialize the full report — no evidence_limit truncation here.
    let summary_json = serde_json::to_string(report)?

    let computed_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64

    Ok(CycleReviewRecord {
        feature_cycle:         feature_cycle.to_string(),
        schema_version:        SUMMARY_SCHEMA_VERSION,
        computed_at,
        raw_signals_available: 1i32,   // live signals — full pipeline just ran
        summary_json,
    })
```

---

## Helper Function: dispatch_format_with_advisory

```
/// Apply evidence_limit truncation and format dispatch, appending an optional
/// advisory string to the response text.
///
/// Called from both handle_memoization_hit and handle_purged_signals_hit.
/// Mirrors the existing step 12 format dispatch in the handler.
fn dispatch_format_with_advisory(
    report: RetrospectiveReport,
    format: &str,
    evidence_limit: Option<usize>,
    advisory: Option<String>,
) -> Result<CallToolResult, ServerError>

BODY:
    match format {
        "markdown" | "summary" => {
            let mut result = format_retrospective_markdown(&report)
            if let Some(note) = advisory {
                // Append advisory to the response text.
                // Access the first Content item's text and append.
                append_text_to_tool_result(&mut result, &format!("\n\n{}", note))
            }
            Ok(result)
        }
        "json" => {
            let evidence_limit = evidence_limit.unwrap_or(3)
            let mut final_report = report
            if evidence_limit > 0 {
                for hotspot in &mut final_report.hotspots {
                    hotspot.evidence.truncate(evidence_limit)
                }
            }
            let mut result = format_retrospective_report(&final_report)
            if let Some(note) = advisory {
                // For JSON format: append as a top-level "advisory" text note
                // or append to the result text. Follow the existing JSON formatter
                // convention for advisory messages in this codebase.
                append_text_to_tool_result(&mut result, &format!("\n\n{}", note))
            }
            Ok(result)
        }
        _ => Err(ServerError::InvalidParams(format!(
            "Unknown format '{}'. Valid values: \"markdown\", \"json\".",
            format
        )))
    }
```

Note: `append_text_to_tool_result` is a small helper (or inline logic) that
appends to the text content of a `CallToolResult`. Check existing helpers in
`mcp/response/mod.rs` for a pattern before adding a new function.

---

## Control Flow Summary

```
context_cycle_review entry
    │
    ├─ step 1–2: identity, validate
    ├─ step 3: three-path obs load → attributed
    │
    ├─ step 2.5 branch:
    │     force=false:
    │       get_cycle_review() → Some → handle_memoization_hit → RETURN
    │                          → Some + deser error → fall through
    │                          → None → fall through
    │                          → Err(read error) → fall through (treat as miss)
    │     force=true + attributed.is_empty():
    │       get_cycle_review() → Some → handle_purged_signals_hit → RETURN
    │                          → None → ERROR_NO_OBSERVATION_DATA → RETURN
    │                          → Err → ERROR_NO_OBSERVATION_DATA → RETURN
    │     force=true + attributed.non_empty():
    │       step 2.5 is skipped entirely → fall through
    │
    ├─ step 4: attributed.is_empty() → MetricVector cache / ERROR_NO_OBSERVATION_DATA
    ├─ steps 5–8/10: full pipeline
    ├─ step 8a: build_cycle_review_record → store_cycle_review → RETURN on Err
    ├─ step 11: audit
    └─ step 12: format dispatch with evidence_limit truncation
```

## Error Handling

| Scenario | Response |
|----------|----------|
| `get_cycle_review` Err (step 2.5, force=false) | Treat as miss, log warn, proceed to full pipeline |
| `get_cycle_review` Err (step 2.5, force=true, empty) | Return ERROR_NO_OBSERVATION_DATA |
| `serde_json::from_str` Err in `handle_memoization_hit` | Return ServerError::MemoizationDeserError; caller falls through to pipeline |
| `serde_json::from_str` Err in `handle_purged_signals_hit` | Return tool error (signals gone, cannot recover) |
| `serde_json::to_string` Err in `build_cycle_review_record` | Return tool error (propagated from step 8a) |
| `store_cycle_review` Err (step 8a) | Return tool error (ERROR_INTERNAL); no panic |
| `store_cycle_review` 4MB exceeded | Return tool error via store error propagation |

## Key Test Scenarios

1. `force` absent: deserializes as `None`; `unwrap_or(false)` = false (AC-12).
2. `force: true` in JSON: deserializes as `Some(true)` (AC-12).
3. Memoization hit path (force=false, stored record): returns stored record, no observation
   load occurs (AC-04, AC-14).
4. Schema version mismatch (stored=0, current=1): advisory in response, stored record
   returned, no recompute (AC-04b, FR-02).
5. Deserialization error of stored record: falls through to full pipeline (R-06 scenario 3).
6. force=true with live signals: overwrites stored record, `INSERT OR REPLACE` (AC-05).
7. force=true with empty attributed + stored record: returns stored + purged note (AC-06, AC-15).
8. force=true with empty attributed + no stored record: ERROR_NO_OBSERVATION_DATA (AC-07).
9. evidence_limit=2 on stored record: stored JSON has full evidence, response has 2 items (AC-08).
10. step 8a not inside spawn_blocking: static grep check (R-09).
11. build_cycle_review_record does not apply evidence_limit before serde_json::to_string (C-03).
