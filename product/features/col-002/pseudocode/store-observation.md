# Pseudocode: store-observation

## Purpose

Add OBSERVATION_METRICS table to unimatrix-store with store/get/list methods. 14th table.

## File: `crates/unimatrix-store/src/schema.rs` (additions)

### Table Definition

```
/// Observation metric storage: feature_cycle -> bincode-serialized MetricVector.
/// Populated by context_retrospective tool.
pub const OBSERVATION_METRICS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("observation_metrics");
```

Add after OUTCOME_INDEX definition. Update comment from "13 total" to "14 total".

## File: `crates/unimatrix-store/src/db.rs` (modifications)

### Store::open_with_config

Add to the table initialization block:
```
txn.open_table(OBSERVATION_METRICS).map_err(StoreError::Table)?;
```

Update comments from "13 tables" to "14 tables".

### Import

Add `OBSERVATION_METRICS` to the schema imports.

## File: `crates/unimatrix-store/src/write.rs` (additions)

### store_metrics

```
/// Store observation metrics for a feature cycle.
///
/// Overwrites any previously stored metrics for the same feature cycle.
/// The `data` parameter is opaque bincode bytes from unimatrix-observe.
pub fn store_metrics(&self, feature_cycle: &str, data: &[u8]) -> Result<()> {
    let txn = self.db.begin_write()?;
    {
        let mut table = txn.open_table(OBSERVATION_METRICS)?;
        table.insert(feature_cycle, data)?;
    }
    txn.commit()?;
    Ok(())
}
```

Add `OBSERVATION_METRICS` to write.rs imports.

## File: `crates/unimatrix-store/src/read.rs` (additions)

### get_metrics

```
/// Retrieve stored observation metrics for a feature cycle.
///
/// Returns None if no metrics have been stored for this feature.
pub fn get_metrics(&self, feature_cycle: &str) -> Result<Option<Vec<u8>>> {
    let txn = self.db.begin_read()?;
    let table = txn.open_table(OBSERVATION_METRICS)?;

    match table.get(feature_cycle)? {
        Some(guard) => Ok(Some(guard.value().to_vec())),
        None => Ok(None),
    }
}
```

### list_all_metrics

```
/// List all stored observation metrics.
///
/// Returns pairs of (feature_cycle, bincode bytes).
pub fn list_all_metrics(&self) -> Result<Vec<(String, Vec<u8>)>> {
    let txn = self.db.begin_read()?;
    let table = txn.open_table(OBSERVATION_METRICS)?;

    let mut results = Vec::new();
    for entry in table.iter()? {
        let (key, value) = entry?;
        results.push((key.value().to_string(), value.value().to_vec()));
    }

    Ok(results)
}
```

Add `OBSERVATION_METRICS` to read.rs imports.

## Error Handling

- All methods propagate StoreError (existing pattern)
- Table open errors handled by StoreError::Table
- Transaction errors handled by StoreError::Transaction

## Key Test Scenarios

- Store::open creates 14 tables including OBSERVATION_METRICS (AC-29)
- OBSERVATION_METRICS accessible after open (AC-28)
- store_metrics + get_metrics roundtrip (AC-30)
- get_metrics for nonexistent key -> None
- list_all_metrics with 0 entries -> empty vec
- list_all_metrics with 3 entries -> 3 results
- store_metrics overwrites previous value
- Schema version remains 3 (AC-31)
- All existing tests still pass (R-06)
