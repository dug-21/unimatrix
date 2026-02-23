# C5: Filter Module -- Pseudocode

## Purpose

Implement hnsw_rs `FilterT` trait for metadata-filtered search. Translates entry ID allow-lists to hnsw data ID allow-lists.

## File: `crates/unimatrix-vector/src/filter.rs`

```
use hnsw_rs::prelude::FilterT;

/// A filter that restricts hnsw_rs search to a pre-computed allow-list.
/// Used by VectorIndex::search_filtered.
pub(crate) STRUCT EntryIdFilter:
    allowed_data_ids: Vec<usize>   // sorted for binary search

IMPL FilterT for EntryIdFilter:
    fn hnsw_filter(&self, id: &usize) -> bool:
        self.allowed_data_ids.binary_search(id).is_ok()

IMPL EntryIdFilter:
    /// Construct from a list of allowed hnsw data IDs.
    /// The input is sorted internally.
    pub(crate) fn new(mut allowed_data_ids: Vec<usize>) -> Self:
        allowed_data_ids.sort_unstable()
        allowed_data_ids.dedup()
        EntryIdFilter { allowed_data_ids }
```

## Design Notes

- `pub(crate)` visibility -- only used by `index.rs`.
- The translation from entry IDs to data IDs happens in `VectorIndex::search_filtered`, not here. This struct only deals with data IDs (usize).
- `sort_unstable` + `dedup` ensures correct binary search and handles duplicate entry IDs in the caller's allow-list.
- hnsw_rs `FilterT` trait has a single method: `hnsw_filter(&self, id: &DataId) -> bool` where `DataId = usize`.
- Using a custom struct rather than `Vec<usize>` directly for type safety and future extensibility.
