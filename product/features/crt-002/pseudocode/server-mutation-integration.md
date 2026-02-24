# Pseudocode: server-mutation-integration (C4)

## File: `crates/unimatrix-server/src/server.rs` + `tools.rs`

### Insert Path (context_store in tools.rs)

After the existing insert logic (which includes the spawn_blocking for combined write), add confidence seeding:

```
// Existing: entry is inserted, id is returned
// After the insert spawn_blocking completes and result is obtained:

// NEW: Seed initial confidence (fire-and-forget)
let store_for_conf = Arc::clone(&self.store);
let _ = tokio::task::spawn_blocking(move || {
    // Read the just-inserted entry
    match store_for_conf.get(new_id) {
        Ok(entry) => {
            let now = current_unix_timestamp_secs();
            let conf = confidence::compute_confidence(&entry, now);
            if let Err(e) = store_for_conf.update_confidence(new_id, conf) {
                warn!("confidence seed failed for entry {new_id}: {e}");
            }
        }
        Err(e) => {
            warn!("confidence seed: failed to read entry {new_id}: {e}");
        }
    }
}).await;
```

This runs after the insert is committed. The confidence update is in a separate transaction. If it fails, the entry exists with confidence=0.0 and will be corrected on first retrieval.

### Correction Path (context_correct in server.rs)

The correction already happens in a combined write transaction in `correct_with_audit()`. After that method returns (with both the deprecated original and new correction), add confidence updates:

```
// Existing: correct_with_audit() returns (deprecated_original, new_correction)
// After HNSW insert for correction:

// NEW: Confidence for new correction + recompute for deprecated original
let store_for_conf = Arc::clone(&self.store);
let new_correction_id = new_correction.id;
let original_id = deprecated_original.id;

let _ = tokio::task::spawn_blocking(move || {
    let now = current_unix_timestamp_secs();

    // Confidence for new correction entry
    match store_for_conf.get(new_correction_id) {
        Ok(entry) => {
            let conf = confidence::compute_confidence(&entry, now);
            if let Err(e) = store_for_conf.update_confidence(new_correction_id, conf) {
                warn!("confidence for correction {new_correction_id}: {e}");
            }
        }
        Err(e) => warn!("confidence: read correction {new_correction_id}: {e}"),
    }

    // Recompute confidence for deprecated original (base_score now 0.2)
    match store_for_conf.get(original_id) {
        Ok(entry) => {
            let conf = confidence::compute_confidence(&entry, now);
            if let Err(e) = store_for_conf.update_confidence(original_id, conf) {
                warn!("confidence for deprecated {original_id}: {e}");
            }
        }
        Err(e) => warn!("confidence: read deprecated {original_id}: {e}"),
    }
}).await;
```

### Deprecation Path (context_deprecate in tools.rs)

After `deprecate_with_audit()` returns the deprecated entry:

```
// Existing: deprecate_with_audit() returns deprecated_entry
// After audit and response formatting:

// NEW: Recompute confidence for deprecated entry
let store_for_conf = Arc::clone(&self.store);
let dep_id = deprecated_entry.id;

let _ = tokio::task::spawn_blocking(move || {
    let now = current_unix_timestamp_secs();
    match store_for_conf.get(dep_id) {
        Ok(entry) => {
            let conf = confidence::compute_confidence(&entry, now);
            if let Err(e) = store_for_conf.update_confidence(dep_id, conf) {
                warn!("confidence for deprecated {dep_id}: {e}");
            }
        }
        Err(e) => warn!("confidence: read deprecated {dep_id}: {e}"),
    }
}).await;
```

## Pattern

All three mutation paths follow the same pattern:
1. Existing mutation completes (insert/correct/deprecate)
2. Fire-and-forget spawn_blocking:
   a. Read entry via store.get()
   b. compute_confidence(entry, now)
   c. update_confidence(id, confidence)
3. Errors logged, never propagated

## Import Additions

```
use crate::confidence;
use unimatrix_store::current_unix_timestamp_secs;  // if not already imported
```

## Dependencies

- `crate::confidence::compute_confidence` (from C1)
- `Store::get()`, `Store::update_confidence()` (from C2)
- `current_unix_timestamp_secs` from unimatrix_store
