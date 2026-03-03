# Component 1: Evidence-Limited Output — Pseudocode

## Files Modified

- `crates/unimatrix-server/src/tools.rs` — RetrospectiveParams + truncation logic

## 1. Wire Type Change (tools.rs)

```pseudo
struct RetrospectiveParams {
    feature_cycle: String,
    agent_id: Option<String>,
    evidence_limit: Option<usize>,  // NEW: default 3, 0 = unlimited
}
```

## 2. R-09 Blocking Gate Audit

Before implementing truncation, audit all existing tests:
- Search for tests that assert on `hotspot.evidence.len()` or evidence array contents
- Current finding: no existing integration tests assert on evidence array lengths
- Document this finding in the implementation

## 3. Clone-and-Truncate Logic (tools.rs, context_retrospective handler)

Location: After building the full report (step 10c), after lesson-learned spawn (Component 3), before returning.

```pseudo
function context_retrospective(params):
    // ... existing steps 1-10c: parse, attribute, detect, compute, build report ...

    // [Component 2] Populate narratives and recommendations on the report
    // (see evidence-synthesis.md)

    // [Component 3] Spawn lesson-learned write on FULL report
    // (see lesson-learned.md)

    // [Component 1] Clone-and-truncate for serialization
    let evidence_limit = params.evidence_limit.unwrap_or(3)

    if evidence_limit > 0:
        let mut truncated = report.clone()
        for hotspot in &mut truncated.hotspots:
            hotspot.evidence.truncate(evidence_limit)
        return format_retrospective_report(&truncated)
    else:
        return format_retrospective_report(&report)
```

## 4. Ordering Constraint

The truncation MUST happen:
- AFTER narrative synthesis (Component 2 needs full evidence)
- AFTER lesson-learned content extraction (Component 3 needs full evidence)
- BEFORE serialization (response contains truncated evidence)

This is enforced by the clone-and-truncate pattern: the original `report` is never mutated.

## 5. Backward Compatibility

- `evidence_limit = 0` disables truncation entirely
- `evidence_limit = None` defaults to 3
- No change to `hotspots: Vec<HotspotFinding>` type
- New `narratives` and `recommendations` fields use `#[serde(default)]` + `skip_serializing_if`

## 6. Cached Report Path

The cached report path (step 6, when `attributed.is_empty()`) also needs the new fields:
```pseudo
// In the cached report construction:
let report = RetrospectiveReport {
    // existing fields...
    narratives: None,           // NEW: no data for cached
    recommendations: vec![],    // NEW: no data for cached
}
```
