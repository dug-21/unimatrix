# Pseudocode: log-downgrade (Item 2)

## Purpose

Change `tracing::warn!` to `tracing::debug!` at exactly two sites inside
`run_cosine_supports_path` where a `category_map` lookup returns `None` due to an entry
being deprecated between Phase 2 DB read and the Path C loop. These misses are an expected
race condition in the HNSW-plus-DB architecture — not anomalies — and must not pollute the
warn log with expected degraded-mode events.

## File

`crates/unimatrix-server/src/services/nli_detection_tick.rs`

## Scope

Exactly two `tracing::warn!` calls changed to `tracing::debug!`. No other changes to
`run_cosine_supports_path`. The non-finite cosine `warn!` at line 766 is NOT changed.

---

## Modified Function: `run_cosine_supports_path`

### Three Log Sites in This Function (from source, verified)

| Line | Site | Current Level | After Change |
|------|------|---------------|--------------|
| 765–770 | Non-finite cosine guard: `if !cosine.is_finite()` | `warn!` | `warn!` — UNCHANGED |
| 796–800 | `category_map.get(src_id)` None arm | `warn!` | `debug!` — CHANGED |
| 806–810 | `category_map.get(tgt_id)` None arm | `warn!` | `debug!` — CHANGED |

### Before / After: Site 1 — `src_id` category_map miss (line 796)

BEFORE:
```rust
None => {
    tracing::warn!(
        src_id,
        "Path C: source entry not found in category_map (deprecated mid-tick?) — skipping"
    );
    continue;
}
```

AFTER:
```rust
None => {
    tracing::debug!(
        src_id,
        "Path C: source entry not found in category_map (deprecated mid-tick?) — skipping"
    );
    continue;
}
```

Only the macro name changes (`warn!` to `debug!`). Message text, field names, and `continue`
are unchanged.

### Before / After: Site 2 — `tgt_id` category_map miss (line 806)

BEFORE:
```rust
None => {
    tracing::warn!(
        tgt_id,
        "Path C: target entry not found in category_map (deprecated mid-tick?) — skipping"
    );
    continue;
}
```

AFTER:
```rust
None => {
    tracing::debug!(
        tgt_id,
        "Path C: target entry not found in category_map (deprecated mid-tick?) — skipping"
    );
    continue;
}
```

Only the macro name changes. Message text, field names, and `continue` are unchanged.

### Non-Finite Cosine Site (line 765): Must Not Change

```rust
// THIS SITE IS UNCHANGED — do NOT downgrade
if !cosine.is_finite() {
    tracing::warn!(    // ← stays warn!
        src_id,
        tgt_id,
        "Path C: non-finite cosine for candidate pair — skipping"
    );
    continue;
}
```

Rationale: A NaN/Inf cosine from HNSW is a structural anomaly indicating potential data
integrity issues in the vector index. This is an operational warning, not expected degraded
behavior. Log level semantic contract (entry #3467): operational anomalies use `warn!`;
expected degraded-mode behavior uses `debug!`.

---

## Log Level Semantic Contract

From Unimatrix entry #3467:
- `warn!` — operational anomaly; indicates a condition that should not occur in correct
  operation and may require investigation.
- `debug!` — expected degraded-mode behavior; a known, documented race condition or
  transient state that is handled gracefully.

Category_map misses fit the second category: the comment at Gate 3 (lines 790–792) already
documents "If an entry was deprecated between Phase 2 DB read and this point, it will be
absent." The warn level at these two sites contradicts the existing comment.

The non-finite cosine fits the first category: HNSW should never produce NaN/Inf embeddings
in correct operation. It does not fit the "expected degraded-mode" pattern.

---

## Error Handling

No error handling changes. Both None arms continue the loop (skip the pair). The function
is infallible (returns `()`). The only observable change is log level at the two sites.

---

## Key Test Scenarios

Log level is NOT asserted in tests per ADR-001(c) (entry #4143). Coverage is behavioral-only.

### AC-04a: `run_cosine_supports_path` skips pair when src_id absent from category_map

```
GIVEN: candidate_pairs contains (src_id=99, tgt_id=1, cosine=0.75)
AND:   category_map does NOT contain src_id=99
AND:   cosine passes threshold and all other gates
WHEN:  run_cosine_supports_path() is called
THEN:  no Supports edge is written for (99, 1)
AND:   function returns without panic
AND:   function does not propagate an error
NOTE:  log level verified by code review only (ADR-001(c) / entry #4143)
```

Test function name: `test_cosine_supports_path_skips_missing_category_map_src`

### AC-04b: `run_cosine_supports_path` skips pair when tgt_id absent from category_map

```
GIVEN: candidate_pairs contains (src_id=1, tgt_id=99, cosine=0.75)
AND:   category_map contains src_id=1 but NOT tgt_id=99
AND:   cosine passes threshold and all other gates
WHEN:  run_cosine_supports_path() is called
THEN:  no Supports edge is written for (1, 99)
AND:   function returns without panic
NOTE:  log level verified by code review only (ADR-001(c) / entry #4143)
```

Test function name: `test_cosine_supports_path_skips_missing_category_map_tgt`

### AC-05: Non-finite cosine pair is skipped without panic

```
GIVEN: candidate_pairs contains (src_id=1, tgt_id=2, cosine=f32::NAN)
AND:   both entries are present in category_map
WHEN:  run_cosine_supports_path() is called
THEN:  no Supports edge is written for (1, 2)
AND:   function returns without panic
NOTE:  the non-finite cosine site (line 765) remains tracing::warn! — verified by
       code review only, not by test assertion (ADR-001(c) / entry #4143)
```

Test function name: `test_cosine_supports_path_nonfinite_cosine_handled`

---

## Gate Report Requirement (R-11)

The gate report for AC-04 and AC-05 must include the following statement verbatim:

> "AC-04 and AC-05 log-level assertions are behavioral-only per ADR-001(c) (Unimatrix
> entry #4143). Log level verified by code review. No `tracing-test` harness used."

---

## Risks Addressed

- R-05: The non-finite cosine site remaining `warn!` is confirmed by code inspection of
  lines 765–770. The two category_map miss sites at lines 796 and 806 are the only changes.
  Implementor must not accidentally change line 766.
- R-11: Behavioral-only coverage is the explicit architectural decision. Gate report must
  document it.

## Knowledge Stewardship

- Entry #3467 (log level semantic contract): `warn!` for anomalies, `debug!` for expected
  degraded mode — directly governs this change.
- ADR-001(c) (entry #4143): behavioral-only test strategy explicitly chosen; no `tracing-test`.
- Deviations from established patterns: none.
