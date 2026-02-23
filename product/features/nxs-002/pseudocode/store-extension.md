# C7: Store Extension -- Pseudocode

## Purpose

Add `Store::iter_vector_mappings()` to unimatrix-store for VECTOR_MAP iteration. Required by the persistence load path (C6) to rebuild the bidirectional IdMap.

## File Modified: `crates/unimatrix-store/src/read.rs`

```
IMPL Store:
    /// Iterate all entries in the VECTOR_MAP table.
    /// Returns Vec<(entry_id, hnsw_data_id)>.
    /// Used by unimatrix-vector to rebuild IdMap on load.
    pub fn iter_vector_mappings(&self) -> Result<Vec<(u64, u64)>>:
        txn = self.db.begin_read()?
        table = txn.open_table(VECTOR_MAP)?

        mappings = Vec::new()
        for result in table.iter()?:
            (key_guard, value_guard) = result?
            entry_id = key_guard.value()
            data_id = value_guard.value()
            mappings.push((entry_id, data_id))

        Ok(mappings)
```

## Design Notes

- Read-only operation. Uses `ReadTransaction`.
- Returns owned `Vec<(u64, u64)>` -- the transaction is closed when the method returns.
- Consistent with existing Store API patterns (e.g., `query_by_topic` returns owned data).
- The `VECTOR_MAP` table definition already exists in `schema.rs`: `TableDefinition<u64, u64>`.
- W1 alignment: this is the approved minor extension to nxs-001.

## Tests Required (in unimatrix-store)

Per W1 approval, tests must be added in the unimatrix-store crate:
1. Empty VECTOR_MAP returns empty vec.
2. Populated table returns all mappings.
3. Consistency after inserts -- insert entries with vector mappings, verify iteration matches individual lookups.
