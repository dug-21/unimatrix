# Component: mcp/response/mod.rs
# crt-051 Pseudocode

## Purpose

`response/mod.rs` handles serialization and formatting of `StatusReport` into MCP
response text. It contains test helper functions that construct `StatusReport` structs
with hardcoded field values for formatting tests.

This component has exactly one change: the `make_coherence_status_report()` helper
function must have `contradiction_count` updated from `0` to `15`. No other fields in
this fixture change. No other fixtures in the file change.

---

## Modified Fixture: make_coherence_status_report() (~line 1397)

### Location

`crates/unimatrix-server/src/mcp/response/mod.rs`, function `make_coherence_status_report()`

### Exact Field Change

| Field | Old Value | New Value | Reason |
|---|---|---|---|
| `contradiction_count` | `0` | `15` | Make fixture consistent with non-trivial score |
| `contradiction_density_score` | `0.7000` | `0.7000` (unchanged) | `1.0 - 15/50 = 0.70` — now mathematically coherent |
| `total_active` | `50` | `50` (unchanged) | Provides normalization denominator |

### Fixture Field (single line to change)

```
Old:   contradiction_count: 0,
New:   contradiction_count: 15,
```

### Why 15

With `total_active: 50` and `contradiction_count: 15`:
`1.0 - (15 as f64 / 50 as f64) = 1.0 - 0.30 = 0.7000` exactly.

This gives the `contradiction_density_score: 0.7000` field its first coherent meaning.
Previously, the fixture had `contradiction_count: 0` with `contradiction_density_score:
0.7000` — a contradiction (no pairs detected, yet sub-1.0 score). The old value 0.7000
was also inconsistent with the old formula: `1.0 - (3/50) = 0.940 != 0.70`.

### Surrounding Fields (must not change)

```
total_active: 50,                      // unchanged — denominator for the formula
total_quarantined: 3,                  // unchanged — quarantine recs, not Lambda
coherence: 0.7450,                     // unchanged — independently hardcoded
graph_quality_score: 0.6500,           // unchanged
embedding_consistency_score: 0.9000,   // unchanged
contradiction_density_score: 0.7000,   // unchanged — now coherent with contradiction_count: 15
```

---

## Other Fixtures: No Change Required

Seven other fixtures in `response/mod.rs` also construct `StatusReport` objects. All
seven have `contradiction_count: 0` and `contradiction_density_score: 1.0`. These are
fully consistent with the new semantics (zero pairs -> score 1.0) and must not be
changed.

Delivery must confirm these seven fixtures are untouched after the edit.

---

## Function Pseudocode (informational — only one field changes)

```
FUNCTION make_coherence_status_report() -> StatusReport:
    RETURN StatusReport {
        total_active: 50,
        total_deprecated: 5,
        total_proposed: 2,
        total_quarantined: 3,
        // ... (other fields unchanged) ...
        contradiction_count: 15,             // CHANGED from 0
        contradiction_scan_performed: false,
        // ... (other fields unchanged) ...
        coherence: 0.7450,
        graph_quality_score: 0.6500,
        embedding_consistency_score: 0.9000,
        contradiction_density_score: 0.7000, // unchanged — now coherent
        // ... (remaining fields unchanged) ...
    }
END FUNCTION
```

---

## Error Handling

This is a test-only helper function. No error handling applies. The struct literal will
not compile if any required field is missing or has the wrong type. Adding a `usize`
value (`15`) where a `usize` field is expected has no risk of type mismatch.

---

## Key Test Scenarios

| Scenario | Verification | AC/Risk |
|---|---|---|
| `contradiction_count` is 15 in fixture | Read `make_coherence_status_report()` | AC-15, R-02 |
| `contradiction_density_score` is still 0.7000 | Read same fixture | AC-15 |
| `total_active` is still 50 | Read same fixture | formula verification |
| Seven other fixtures have `contradiction_count: 0` | Read/search other fixture helpers | R-02 |
| `cargo test -p unimatrix-server mcp::response` passes | cargo test | R-02 |

The formatting test `test_coherence_markdown_section` (around line 1457) asserts
`coherence: 0.7450`. That field is unchanged. This test must continue to pass without
modification.

No test in `response/mod.rs` directly asserts the value `0.7000` for
`contradiction_density_score` — the field exists only in the fixture struct. Changing
`contradiction_count` from `0` to `15` will not break any existing assertion.
