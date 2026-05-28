## ADR-003: f64 NaN Safety for graph_edges.weight Export

### Context

`graph_edges.weight` is a REAL (f64) column with DEFAULT 1.0. When serializing f64 values to JSON, `serde_json::Number::from_f64()` returns `None` for NaN and Infinity because JSON has no representation for these values. The existing `entries.confidence` export already handles this with `Number::from_f64(v).unwrap_or(Number::from(0))` (nan-001 ADR-002).

Risk SR-01 from the scope risk assessment identifies this as a medium-severity risk. Unimatrix pattern #4133 and #4533 document NaN safety requirements across the codebase.

The question is what fallback value to use when `weight` is NaN:
- 0 (matches entries.confidence fallback): semantically wrong for weight -- a 0 weight means "no relationship"
- 1.0 (the column DEFAULT): semantically correct -- the default weight means "standard strength relationship"

### Decision

Use `Number::from_f64(weight).unwrap_or_else(|| serde_json::Number::from_f64(1.0).unwrap())` for `graph_edges.weight`. The fallback value is 1.0 (the column's DEFAULT), not 0.

This differs from `entries.confidence` which falls back to 0 because confidence 0 means "lowest confidence" (a safe default), whereas weight 0 would mean "no relationship" (a destructive default that would effectively delete edge significance).

The same `Number::from_f64` call is used -- the pattern is identical to nan-001, only the fallback value differs to match the column's semantic default.

### Consequences

- NaN weights (which should never occur in practice -- they would indicate a bug in edge creation) are silently coerced to 1.0 rather than causing a serialization panic
- The fallback preserves the edge's structural significance rather than nullifying it
- If NaN weights are observed in production, the export still succeeds -- the bug is in edge creation, not export
- Tests must explicitly verify NaN fallback behavior for weight (AC-11)
