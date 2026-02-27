# Pseudocode: store-pipeline (server crate)

## Purpose

Extend the context_store pipeline to:
1. Add `feature_cycle: Option<String>` to StoreParams
2. Validate feature_cycle input
3. Call validate_outcome_tags when category == "outcome"
4. Populate OUTCOME_INDEX in insert_with_audit transaction
5. Include warning when outcome has empty feature_cycle

## Changes

### tools.rs: StoreParams

Add field after `format`:

```rust
pub struct StoreParams {
    // ... existing fields ...
    /// Feature cycle or workflow identifier (e.g., "col-001", "bug-42").
    pub feature_cycle: Option<String>,
}
```

### validation.rs: validate_store_params

Add feature_cycle validation at end of validate_store_params:

```
const MAX_FEATURE_CYCLE_LEN: usize = 128;

pub fn validate_store_params(params: &StoreParams) -> Result<(), ServerError> {
    // ... existing validation ...
    if let Some(fc) = &params.feature_cycle {
        validate_string_field("feature_cycle", fc, MAX_FEATURE_CYCLE_LEN, false)?;
    }
    Ok(())
}
```

### tools.rs: context_store handler

After step 5 (category validation), add outcome tag validation:

```
// 5a. Outcome tag validation (only for outcome entries)
if params.category == "outcome" {
    let tags = params.tags.as_deref().unwrap_or(&[]);
    crate::outcome_tags::validate_outcome_tags(tags)
        .map_err(rmcp::ErrorData::from)?;
}
```

In step 9 (build NewEntry), map feature_cycle:

```
let new_entry = NewEntry {
    // ... existing fields ...
    feature_cycle: params.feature_cycle.clone().unwrap_or_default(),
    // ... rest ...
};
```

After step 12 (format response), add orphan outcome warning:

```
// 12a. Warn if outcome without feature_cycle
if params.category == "outcome" && params.feature_cycle.as_ref().map_or(true, |fc| fc.is_empty()) {
    // Modify the response to include a warning note
    // The format_store_success_with_warning function handles this
}
```

### server.rs: insert_with_audit

Add OUTCOME_INDEX import:

```
use unimatrix_store::OUTCOME_INDEX;
```

After VECTOR_MAP insert and before counter increment (within the existing transaction), add:

```
// Write OUTCOME_INDEX (if outcome with non-empty feature_cycle)
if record.category == "outcome" && !record.feature_cycle.is_empty() {
    let mut outcome_table = txn.open_table(OUTCOME_INDEX)
        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
    outcome_table.insert((record.feature_cycle.as_str(), id), ())
        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
}
```

### response.rs: format_store_success

Extend format_store_success to accept an optional warning:

Option A (preferred): Add a `notes` parameter to format_store_success.
Option B: Append warning text in the caller.

Going with Option B for minimal disruption:

In tools.rs context_store, after generating the CallToolResult:

```
let mut result = format_store_success(&record, format);
if record.category == "outcome" && record.feature_cycle.is_empty() {
    // Append warning to the text content
    if let Some(Content::Text(text_content)) = result.content.first_mut() {
        text_content.text.push_str("\nNote: outcome not linked to a workflow (no feature_cycle provided)");
    }
}
```

## Invariants

- feature_cycle is Optional in StoreParams, defaults to empty string in NewEntry
- Outcome tag validation happens BEFORE content scanning/embedding (fail fast)
- OUTCOME_INDEX insert is within the same write transaction as ENTRIES (atomic)
- OUTCOME_INDEX only populated when category == "outcome" AND feature_cycle is non-empty
- Non-outcome entries never trigger outcome tag validation or OUTCOME_INDEX writes
- Warning text appended to response for orphan outcomes, not an error
