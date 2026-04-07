# Agent Report: crt-050-agent-3-risk

## Output

Produced: `/workspaces/unimatrix/product/features/crt-050/RISK-TEST-STRATEGY.md`

## SR-01 Conflict Resolution

**Verdict: Architect (ADR-005) is correct. Spec writer C-02/AC-SV-01 is wrong.**

Direct source inspection of `crates/unimatrix-server/src/uds/listener.rs` lines 2686–2697 confirms:

```rust
let input = event.payload
    .get("tool_input")
    .map(|v| serde_json::to_string(v).unwrap_or_default());
```

`event.payload.get("tool_input")` for a `context_get` call returns
`Value::Object({"id": 42})`. `serde_json::to_string(Value::Object{...})` produces
`'{"id":42}'` — stored as a plain JSON object string, not double-encoded.

The spec writer misidentified the two-branch read-path parser in `knowledge_reuse.rs`
lines 76–103 as evidence of double-encoding at write time. That parser handles the
re-wrapping of stored strings as `Value::String(raw_json)` during in-memory loading,
not a write-path encoding issue. The stored SQLite bytes are never double-encoded.

Pure-SQL `json_extract(o.input, '$.id')` is valid for all observations. ADR-005 stands.

**Action required:** Spec C-02, FR-07 option language, and AC-SV-01 must be corrected
or superseded before the implementation agent begins work.

## Risk Summary

| Priority | Count | Risks |
|----------|-------|-------|
| Critical | 1 | R-01 (spec SR-01 conflict) |
| High | 2 | R-02 (outcome vocab drift), R-03 (mixed-weight bucket ordering) |
| Medium | 5 | R-04 (threshold default misalignment), R-05 (MILLIS_PER_DAY), R-06 (rename surface), R-07 (weights formula), R-10/R-11 (column name / index) |
| Low | 4 | R-08 (NULL feature_cycle), R-09 (visibility deferral), R-12 (unknown outcomes) |

## Knowledge Stewardship
- Queried: /uni-knowledge-search for risk patterns -- found #4222 (mandatory SQL constraints), #3685 (rank normalization formula), #4225 (ADR-003 outcome weighting)
- Stored: nothing novel to store -- R-02 outcome vocab drift pattern is crt-050-specific; not yet visible across 2+ features as a recurring pattern
