# Pseudocode: type-migration (Wave 1)

## 1. Move Types to unimatrix-core

### unimatrix-core/Cargo.toml
```
Add serde_json = { workspace = true } to [dependencies]
```

### unimatrix-core/src/lib.rs
```
Add module: pub mod observation;
Add re-exports:
  pub use observation::{ObservationRecord, HookType, ParsedSession, ObservationStats};
```

### unimatrix-core/src/observation.rs (NEW)
```
// Move these types from unimatrix-observe/src/types.rs:
// - HookType enum (Debug, Clone, PartialEq, Eq, Serialize, Deserialize)
// - ObservationRecord struct (Debug, Clone, Serialize, Deserialize)
// - ParsedSession struct (Debug, Clone)
// - ObservationStats struct (Debug, Clone)

// Keep exact same field names, derives, and doc comments.
// The serde_json::Value field on ObservationRecord.input requires serde_json dep.
```

## 2. Re-export from unimatrix-observe

### unimatrix-observe/src/types.rs
```
// Remove the 4 type definitions (HookType, ObservationRecord, ParsedSession, ObservationStats)
// Replace with re-exports from unimatrix-core:
pub use unimatrix_core::{HookType, ObservationRecord, ParsedSession, ObservationStats};

// Keep all other types (HotspotCategory, Severity, EvidenceRecord, etc.) in place
// Keep all test code that uses these types (tests will use the re-exported versions)
```

### unimatrix-observe/Cargo.toml
```
Add to [dependencies]:
  unimatrix-store = { path = "../unimatrix-store" }
  unimatrix-core = { path = "../unimatrix-core" }
```

### unimatrix-observe/src/lib.rs
```
// Update the crate doc comment to remove "no dependency on unimatrix-store" claim
// The re-exports in lib.rs already re-export from types:: which now re-exports from core
// No changes to the pub use statements needed
```

## 3. Update Imports

### Files to update (~14 files)
All files that use ObservationRecord, HookType, ParsedSession, or ObservationStats
need their imports verified. Since unimatrix-observe re-exports these types,
most files within unimatrix-observe should continue to work via `crate::types::`.
Server files that import from unimatrix_observe:: will also still work via re-exports.

No import changes should be required if re-exports are correct.

## 4. trust_score("auto") in unimatrix-engine/src/confidence.rs

```rust
// In trust_score() function, change the match:
// FROM:
//   "human" => 1.0,
//   "system" => 0.7,
//   "agent" => 0.5,
//   _ => 0.3,
// TO:
//   "human" => 1.0,
//   "system" => 0.7,
//   "agent" => 0.5,
//   "auto" => 0.35,
//   _ => 0.3,
```

## 5. Extract check_entry_contradiction() from contradiction.rs

```rust
// In unimatrix-server/src/infra/contradiction.rs:

// NEW PUBLIC FUNCTION:
pub fn check_entry_contradiction(
    content: &str,
    title: &str,
    store: &Store,
    vector_store: &dyn VectorStore,
    embed_adapter: &dyn EmbedService,
    config: &ContradictionConfig,
) -> Result<Option<ContradictionPair>, ServerError> {
    // 1. Embed title + content
    let embed_text = format!("{} {}", title, content);
    let embedding = embed_adapter.embed_entry(title, content)
        .map_err(|e| ServerError::Core(e))?;

    // 2. Search HNSW for neighbors
    let neighbors = vector_store.search(&embedding, config.neighbors_per_entry, EF_SEARCH)
        .map_err(|e| ServerError::Core(e))?;

    // 3. For each neighbor above similarity threshold:
    let mut best: Option<ContradictionPair> = None;
    for neighbor in &neighbors {
        if (neighbor.similarity as f32) < config.similarity_threshold {
            continue;
        }
        let neighbor_entry = store.get(neighbor.entry_id)?;
        if neighbor_entry.status != Status::Active {
            continue;
        }

        // 4. Run conflict heuristic
        let (conflict_score, explanation) = conflict_heuristic(
            content,
            &neighbor_entry.content,
            config.conflict_sensitivity,
        );

        if conflict_score > 0.0 {
            let pair = ContradictionPair {
                entry_id_a: 0, // proposed entry has no ID yet
                entry_id_b: neighbor_entry.id,
                title_a: title.to_string(),
                title_b: neighbor_entry.title.clone(),
                similarity: neighbor.similarity as f32,
                conflict_score,
                explanation,
            };
            // Keep the highest conflict score
            if best.as_ref().map_or(true, |b| conflict_score > b.conflict_score) {
                best = Some(pair);
            }
        }
    }

    Ok(best)
}

// REFACTOR scan_contradictions() to use check_entry_contradiction internally:
// The inner loop body (embed -> search -> heuristic per neighbor) can be extracted,
// but scan_contradictions has seen_pairs dedup that check_entry_contradiction doesn't need.
// Keep scan_contradictions as-is for now; just add the new function alongside it.
// This avoids risking regressions in the scan path.
```

## Error Handling
- All existing error types preserved
- check_entry_contradiction propagates ServerError like scan_contradictions
- Type migration is purely mechanical (re-exports)
