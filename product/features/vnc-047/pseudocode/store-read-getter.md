# C3 — Store read getter `get_cycle_tags`

**File:** `crates/unimatrix-store/src/db.rs` (NEW; parity `get_cycle_start_goal` :371)
**ADR:** ADR-004. **AC:** AC-05. **Consumed by:** C8 (review handler).

## Purpose

Read the frozen tag set for a `feature_cycle` at review time, deterministically ordered. `cycle_tags`
is the source of truth read fresh each review (NOT the `summary_json` mirror — that would be a
source-of-truth inversion).

## Signature

```rust
pub async fn get_cycle_tags(&self, feature_cycle: &str) -> Result<Vec<String>>;
```

## Pseudocode body

```
FUNCTION get_cycle_tags(feature_cycle):
    rows: Vec<String> = query_scalar_all(write_pool,
        "SELECT tag FROM cycle_tags WHERE feature_cycle = ?1 ORDER BY tag",
        binds: [feature_cycle])                 # PARAMETERIZED; ORDER BY tag → deterministic
        map_err → StoreError::Database
    return Ok(rows)                             # empty Vec when no rows (no tags supplied)
```

- `ORDER BY tag` gives a stable, deterministic order for both JSON and markdown output (no reliance
  on insert order or rowid). Uses `idx_cycle_tags_tag`.
- Reads via `write_pool` (parity with `get_cycle_start_goal` :379, which reads the write pool for
  read-your-writes consistency on this data class).
- Value-opaque: returns tags byte-for-byte; no filtering, no interpretation.

## Error handling

- DB error → `Err(StoreError::Database)`. The caller (C8) maps this to `report.tags = []` +
  `tracing::warn` and continues — review NEVER fails on tag read (parity `get_cycle_start_goal`
  degrade arm, tools.rs:3425).

## Key test scenarios (hints)

1. Insert `{B, A, C}` for a FC → `get_cycle_tags` returns `["A","B","C"]` (sorted, deterministic).
2. No rows for a FC → returns `[]` (not an error).
3. Verbatim round-trip: colon-prefixed and unicode tags returned byte-identical.
4. (Assembled) tags land via the hook path, then `get_cycle_tags` reads exactly the frozen set — but
   AC-05 itself must be proven via `context_cycle_review`, not this getter alone (SR-08).
