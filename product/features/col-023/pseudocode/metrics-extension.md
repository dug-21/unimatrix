# Pseudocode: metrics-extension

**Wave**: 3 (parallel with detection-rules and schema-migration)
**Crate**: `unimatrix-store` and `unimatrix-observe`
**Files modified**:
- `crates/unimatrix-store/src/metrics.rs` — MetricVector, store_metrics, get_metrics
- `crates/unimatrix-observe/src/metrics.rs` — compute_universal, compute_metric_vector

## Purpose

Extend `MetricVector` with `domain_metrics: HashMap<String, f64>` for non-claude-code
session metrics. Update `store_metrics()` and `get_metrics()` to read/write the
`domain_metrics_json` column added in schema v14. Guard `compute_universal()` in
`unimatrix-observe/src/metrics.rs` to operate only on `source_domain == "claude-code"`
records (IR-03). Update `UNIVERSAL_METRICS_FIELDS` to include `domain_metrics_json`
as the 22nd entry (FR-05.5).

## unimatrix-store/src/metrics.rs Changes

### MetricVector struct extension

Current:
```
pub struct MetricVector:
    pub computed_at: u64
    pub universal: UniversalMetrics
    pub phases: BTreeMap<String, PhaseMetrics>
```

After:
```
pub struct MetricVector:
    pub computed_at: u64
    pub universal: UniversalMetrics
    pub phases: BTreeMap<String, PhaseMetrics>
    #[serde(default)]
    pub domain_metrics: HashMap<String, f64>    -- NEW; empty for claude-code sessions
```

The `#[serde(default)]` ensures v13 rows (without `domain_metrics_json`) deserialize
without error, producing `HashMap::new()` (FR-05.4).

Add `use std::collections::HashMap;` if not already present.

### UNIVERSAL_METRICS_FIELDS const update (FR-05.5)

Current: 21 entries.
After: 22 entries. Add `"domain_metrics_json"` as the last entry:

```
pub const UNIVERSAL_METRICS_FIELDS: &[&str] = &[
    -- ... existing 21 entries unchanged ...
    "domain_metrics_json",    -- NEW 22nd entry
];
```

The order matters for the structural test (R-11). The 22nd entry must be last.

### store_metrics() update

Current signature (unchanged): `fn store_metrics(conn: &Connection, mv: &MetricVector, feature_cycle: &str) -> Result<()>`

The function currently builds an INSERT with named parameters for each of the 21
`UniversalMetrics` fields. After this change:

```
fn store_metrics(conn: &Connection, mv: &MetricVector, feature_cycle: &str) -> Result<()>:
    -- Serialize domain_metrics to JSON string (NULL if empty, for claude-code sessions)
    let domain_metrics_json: Option<String> = if mv.domain_metrics.is_empty():
        None
    else:
        Some(serde_json::to_string(&mv.domain_metrics)
            .map_err(|e| StoreError::Serialize(e.to_string()))?)

    -- Build INSERT statement including domain_metrics_json column
    -- The INSERT must include domain_metrics_json as a named parameter after the 21 existing ones
    -- Existing named parameters for UniversalMetrics fields are unchanged
    sqlx::query("
        INSERT OR REPLACE INTO OBSERVATION_METRICS (
            feature_cycle, computed_at,
            -- ... 21 UniversalMetrics column names ... ,
            domain_metrics_json
        ) VALUES (
            :feature_cycle, :computed_at,
            -- ... 21 UniversalMetrics binding parameters ... ,
            :domain_metrics_json
        )")
        .bind(...)   -- existing 21 bindings unchanged
        .bind(domain_metrics_json)  -- NEW binding for domain_metrics_json
        ...
```

Implementation note: do not change the existing 21 named column bindings. Add
`domain_metrics_json` at the end of both the column list and the VALUES list.

### get_metrics() update

Current: reads 21 named columns from `OBSERVATION_METRICS`. After:

```
fn get_metrics(conn: &Connection, feature_cycle: &str) -> Result<Option<MetricVector>>:
    let row = sqlx::query("
        SELECT computed_at,
               -- ... 21 UniversalMetrics column names ... ,
               domain_metrics_json
        FROM OBSERVATION_METRICS
        WHERE feature_cycle = ?1")
        ...

    -- Read domain_metrics_json column
    let domain_metrics_json: Option<String> = row.get("domain_metrics_json")

    let domain_metrics: HashMap<String, f64> = match domain_metrics_json:
        None => HashMap::new()           -- NULL in DB: empty map (v13 rows, FR-05.4)
        Some(json_str) =>
            serde_json::from_str(&json_str)
                .unwrap_or_else(|_| HashMap::new())  -- Malformed JSON: treat as empty

    -- Build MetricVector with domain_metrics
    Ok(Some(MetricVector {
        computed_at,
        universal,   -- existing 21-field construction unchanged
        phases,
        domain_metrics,
    }))
```

For v13 rows: the `domain_metrics_json` column does not exist in the DB, but since
it is `NULL` (no column → no row in the result set, or NULL if query includes it),
it deserializes to `None` via `row.get()`. The `HashMap::new()` fallback handles
this case (FR-05.4). The named column approach is safe after the `ALTER TABLE ADD COLUMN`
migration (R-05) — named column reads are not positional.

## unimatrix-observe/src/metrics.rs Changes

### Remove HookType import

```
-- DELETE: use crate::types::{..., HookType, ...}
```

### compute_universal() — source_domain guard (IR-03)

The current implementation matches on `r.hook == HookType::PreToolUse` etc.
After the change, every event-type check in `compute_universal()` must be guarded with
`source_domain == "claude-code"`.

The simplest approach: pre-filter the records slice at the top of `compute_universal()`:

```
fn compute_universal(records: &[ObservationRecord], hotspots: &[HotspotFinding]) -> UniversalMetrics:
    -- source_domain guard (MANDATORY — IR-03)
    let records: Vec<&ObservationRecord> = records.iter()
        .filter(|r| r.source_domain == "claude-code")
        .collect()

    let mut m = UniversalMetrics::default()

    -- Count tool calls (PreToolUse events)
    m.total_tool_calls = records.iter()
        .filter(|r| r.event_type == "PreToolUse")
        .count() as u64

    -- ... all other field computations unchanged in logic, but:
    --     REPLACE: r.hook == HookType::PreToolUse
    --     WITH:    r.event_type == "PreToolUse"
    --
    --     REPLACE: r.hook == HookType::PostToolUse
    --     WITH:    r.event_type == "PostToolUse"
    --
    --     REPLACE: match r.hook { HookType::PreToolUse => ..., HookType::PostToolUse => ... }
    --     WITH:    if r.event_type == "PreToolUse" ... else if r.event_type == "PostToolUse" ...

    m
```

The 21 `UniversalMetrics` fields all compute from claude-code-specific events
(PreToolUse, PostToolUse). With the pre-filter guard, non-claude-code records never
contribute to any field. This ensures IR-03: a session with only `source_domain = "sre"`
records produces zero-value `UniversalMetrics` (all fields = 0).

### compute_metric_vector() extension

Current:
```
pub fn compute_metric_vector(records, hotspots, computed_at) -> MetricVector:
    MetricVector { computed_at, universal: compute_universal(records, hotspots), phases: compute_phases(records) }
```

After:
```
pub fn compute_metric_vector(records, hotspots, computed_at) -> MetricVector:
    let universal = compute_universal(records, hotspots)
    let phases = compute_phases(records)
    let domain_metrics = compute_domain_metrics(records)
    MetricVector { computed_at, universal, phases, domain_metrics }
```

### compute_domain_metrics() — new function

```
fn compute_domain_metrics(records: &[ObservationRecord]) -> HashMap<String, f64>:
    -- For W1-5, the only domain in production is "claude-code".
    -- Non-claude-code records pass through with domain_metrics = empty.
    --
    -- Extension point for W3-1: domain-specific aggregations can be added here
    -- keyed by "<source_domain>.<metric_name>".
    --
    -- For W1-5: return empty map for all sessions.
    -- This satisfies FR-05.3 (domain_metrics_json NULL for claude-code sessions).
    HashMap::new()
```

Note: `compute_domain_metrics` returns `HashMap::new()` in W1-5. This is correct.
The extension hook is provided for W3-1 without requiring a schema migration.

## Error Handling

- `store_metrics()`: JSON serialization of `domain_metrics` can fail; return
  `Err(StoreError::Serialize(...))` on failure.
- `get_metrics()`: malformed `domain_metrics_json` in the DB is treated as an empty map
  (`.unwrap_or_else(|_| HashMap::new())`). This is a best-effort degradation.
- Empty `domain_metrics` writes `NULL` to `domain_metrics_json` column, not `"{}"`.
  This avoids storing unnecessary JSON for the majority of rows (claude-code sessions).

## Key Test Scenarios

1. **IR-03 source_domain guard in compute_universal()**: supply a session with only
   `source_domain = "sre"` records where one has `event_type = "PostToolUse"`.
   Assert all 21 `UniversalMetrics` fields are zero (not 1 for `total_tool_calls`).

2. **R-02 backward compatibility**: supply a fixed claude-code session fixture with the
   new `event_type` + `source_domain` fields. Assert all 21 `UniversalMetrics` fields
   produce identical values to the pre-refactor baseline.

3. **domain_metrics empty for claude-code**: `compute_metric_vector()` for a claude-code
   session produces `MetricVector.domain_metrics == HashMap::new()`.

4. **MetricVector round-trip via store_metrics/get_metrics**:
   - Write a `MetricVector` with `domain_metrics = HashMap::new()`. Assert `domain_metrics_json` column is NULL.
   - Write a `MetricVector` with `domain_metrics = {"sre_incident_count": 3.0}`. Assert
     `get_metrics` returns `domain_metrics == {"sre_incident_count": 3.0}`.

5. **v13 row read-back (FR-05.4)**: simulate a v13 row (no `domain_metrics_json` column
   in query result). Assert `MetricVector.domain_metrics` is `HashMap::new()`.

6. **R-11 structural test**: `UNIVERSAL_METRICS_FIELDS.len() == 22`; the 22nd entry
   is `"domain_metrics_json"`; all 21 original field names are present and in order.

7. **R-05 positional read safety**: after the v14 migration, read back all 21 original
   fields by name. Assert no field value is offset by the new column addition.
