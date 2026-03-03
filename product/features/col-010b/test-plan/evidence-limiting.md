# Component 1: Evidence-Limiting — Test Plan

## Unit Tests (tools.rs)

### T-EL-01: RetrospectiveParams evidence_limit deserialization
- JSON `{"feature_cycle": "col-010b"}` -> evidence_limit is None
- JSON `{"feature_cycle": "col-010b", "evidence_limit": 5}` -> evidence_limit is Some(5)
- JSON `{"feature_cycle": "col-010b", "evidence_limit": 0}` -> evidence_limit is Some(0)
- Verifies backward compatibility of the wire type.

### T-EL-02: evidence_limit default is 3
- Verify `params.evidence_limit.unwrap_or(3) == 3` when None.

### T-EL-03: Clone-and-truncate preserves original
- Build a RetrospectiveReport with 2 hotspots, each with 10 evidence items.
- Clone the report, truncate each hotspot.evidence to 3.
- Assert truncated report has 3 evidence items per hotspot.
- Assert original report still has 10 evidence items per hotspot.
- This directly verifies ADR-001 (R-01 coverage).

### T-EL-04: evidence_limit = 0 skips truncation
- Build a report with 10 evidence items per hotspot.
- With evidence_limit = 0, no truncation applied.
- Verify all 10 items preserved in output.

## Integration Tests

### AC-01: Default evidence_limit payload size
- Build synthetic report with 13 hotspots, each with 10+ evidence items.
- Apply truncation with default limit (3).
- Serialize to JSON.
- Assert each hotspot.evidence.len() <= 3.
- Assert total JSON byte length <= 10240.

### AC-02: evidence_limit = 0 backward compatibility
- Build a report, serialize with evidence_limit = 0.
- Verify output is structurally identical to pre-col-010b format.
- All evidence arrays complete, no new fields when empty.

### AC-10: Existing tests pass
- `cargo test --workspace` passes.
- R-09 audit: no existing tests assert on evidence array lengths.
- Cached report path includes new fields with default values.
