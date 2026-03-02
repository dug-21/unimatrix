# Pseudocode: tiered-output

Component: Evidence-Limited Retrospective Output (P1)
Files:
  - `crates/unimatrix-server/src/wire.rs` (evidence_limit parameter)
  - `crates/unimatrix-server/src/tools.rs` (server-side truncation)

---

## Purpose

Add `evidence_limit: Option<usize>` parameter to the `context_retrospective` tool request. When set (default = 3), cap the number of evidence items per hotspot before returning the report. `evidence_limit = 0` means no cap — full arrays returned (backward compatible). Resolves issue #65 (~87KB payload).

---

## Prerequisite (R-09 audit)

BEFORE implementing this component, audit all existing integration tests for `context_retrospective` that assert on exact `hotspots[].evidence` array lengths. Update those tests to either:
- Pass `evidence_limit = 0` to restore full arrays, OR
- Update expected count to `<= 3` items.

See `product/test/infra-001/suites/test_tools.py` and `test_lifecycle.py` for assertions on `hotspots[].evidence`.

---

## 1. wire.rs Changes

Find the `context_retrospective` request type (likely a struct or JSON schema definition). Add `evidence_limit` field:

```
// In the context_retrospective request type
pub struct ContextRetrospectiveRequest {
    pub feature_cycle: String,
    // ... existing fields ...
    pub evidence_limit: Option<usize>,  // NEW (col-010): default 3 when None
}
```

If wire.rs uses a JSON schema / tool definition string, update the schema to add:
```json
"evidence_limit": {
    "type": "integer",
    "description": "Max evidence items per hotspot (0 = unlimited, default = 3)",
    "minimum": 0
}
```

---

## 2. tools.rs — Server-Side Evidence Truncation

In `handle_context_retrospective` (or equivalent), after the report is built:

```
// NEW (col-010): apply evidence_limit
let limit: usize = request.evidence_limit.unwrap_or(3)

let report = if limit > 0:
    apply_evidence_limit(report, limit)
else:
    report  // limit=0: no cap, backward compatible
```

### apply_evidence_limit

```
fn apply_evidence_limit(mut report: RetrospectiveReport, limit: usize) -> RetrospectiveReport:
    for hotspot in &mut report.hotspots:
        if hotspot.evidence.len() > limit:
            hotspot.evidence.truncate(limit)
    report
```

Note: truncation is applied AFTER the report is fully built — both from structured path and JSONL path. The `HotspotFinding.evidence` field type is unchanged.

---

## 3. Default Behavior

When `evidence_limit` is absent from the request:
- Default to 3 (resolves issue #65 payload problem).
- Callers that want full evidence must pass `evidence_limit = 0` explicitly.

Document this default prominently in the tool description string in `tools.rs` (wherever the tool is registered with the MCP server).

---

## Backward Compatibility

| Scenario | Behavior |
|----------|---------|
| `evidence_limit` absent | Defaults to 3; evidence arrays capped at 3 items per hotspot |
| `evidence_limit = 0` | No cap; full evidence arrays (pre-col-010 behavior) |
| `evidence_limit = 5` | Cap at 5 items per hotspot |

Pre-col-010 callers that do not send `evidence_limit` will receive capped arrays (default 3). This is the intended behavioral change — callers must opt-in to full arrays with `evidence_limit = 0`. Tests asserting on exact evidence counts must be updated to use `evidence_limit = 0`.

---

## Error Handling

| Case | Handling |
|------|---------|
| `evidence_limit` is negative integer | JSON schema `minimum: 0` rejects at parse time |
| `evidence_limit` > actual evidence count | `truncate(limit)` is a no-op; safe |
| Report has no hotspots | `apply_evidence_limit` iterates empty vec; safe |

---

## Key Test Scenarios

1. `context_retrospective` with no `evidence_limit` → each hotspot has ≤ 3 evidence items.
2. `context_retrospective` with `evidence_limit = 0` → full evidence arrays returned.
3. `context_retrospective` with `evidence_limit = 5` → each hotspot has ≤ 5 evidence items.
4. Total payload for 13-hotspot report with `evidence_limit = 3` → ≤ 10KB.
5. Hotspot with 2 evidence items, `evidence_limit = 3` → all 2 items returned (no truncation needed).
6. Hotspot with 10 evidence items, `evidence_limit = 3` → exactly 3 items returned.
7. All pre-col-010 integration tests updated to pass `evidence_limit = 0` → they continue to pass.
