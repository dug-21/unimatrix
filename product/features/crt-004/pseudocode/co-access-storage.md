# Pseudocode: C1 — Co-Access Storage

## Crate: unimatrix-store

### schema.rs additions

```rust
// Table definition
pub const CO_ACCESS: TableDefinition<(u64, u64), &[u8]> =
    TableDefinition::new("co_access");

// Record type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoAccessRecord {
    pub count: u32,
    pub last_updated: u64,
}

// Key helper — canonical ordered pair
pub fn co_access_key(a: u64, b: u64) -> (u64, u64) {
    if a <= b { (a, b) } else { (b, a) }
}

// Serialization (same pattern as EntryRecord)
pub fn serialize_co_access(record: &CoAccessRecord) -> Result<Vec<u8>> {
    bincode::serde::encode_to_vec(record, bincode::config::standard())
        .map_err(|e| StoreError::Serialization(e.to_string()))
}

pub fn deserialize_co_access(bytes: &[u8]) -> Result<CoAccessRecord> {
    let (record, _) = bincode::serde::decode_from_slice::<CoAccessRecord, _>(
        bytes, bincode::config::standard()
    ).map_err(|e| StoreError::Deserialization(e.to_string()))?;
    Ok(record)
}
```

### db.rs changes

```rust
// In Store::open_with_config, add after FEATURE_ENTRIES:
txn.open_table(CO_ACCESS).map_err(StoreError::Table)?;
```

### write.rs additions

```rust
impl Store {
    /// Record co-access for a set of entry IDs.
    /// Generates pairs from first min(entry_ids.len(), max_pairs_from) entries.
    pub fn record_co_access(&self, entry_ids: &[u64], max_pairs_from: usize) -> Result<()> {
        let effective_len = entry_ids.len().min(max_pairs_from);
        if effective_len < 2 {
            return Ok(());
        }

        let now = unix_timestamp();
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(CO_ACCESS)?;
            for i in 0..effective_len {
                for j in (i+1)..effective_len {
                    let key = co_access_key(entry_ids[i], entry_ids[j]);
                    let record = match table.get(key)? {
                        Some(existing) => {
                            let mut rec = deserialize_co_access(existing.value())?;
                            rec.count = rec.count.saturating_add(1);
                            rec.last_updated = now;
                            rec
                        }
                        None => CoAccessRecord { count: 1, last_updated: now },
                    };
                    let bytes = serialize_co_access(&record)?;
                    table.insert(key, bytes.as_slice())?;
                }
            }
        }
        txn.commit()?;
        Ok(())
    }

    /// Record pre-computed co-access pairs (after dedup).
    pub fn record_co_access_pairs(&self, pairs: &[(u64, u64)]) -> Result<()> {
        if pairs.is_empty() {
            return Ok(());
        }

        let now = unix_timestamp();
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(CO_ACCESS)?;
            for &(a, b) in pairs {
                let key = co_access_key(a, b);
                let record = match table.get(key)? {
                    Some(existing) => {
                        let mut rec = deserialize_co_access(existing.value())?;
                        rec.count = rec.count.saturating_add(1);
                        rec.last_updated = now;
                        rec
                    }
                    None => CoAccessRecord { count: 1, last_updated: now },
                };
                let bytes = serialize_co_access(&record)?;
                table.insert(key, bytes.as_slice())?;
            }
        }
        txn.commit()?;
        Ok(())
    }

    /// Remove co-access pairs with last_updated < cutoff.
    /// Returns count of removed pairs.
    pub fn cleanup_stale_co_access(&self, cutoff_timestamp: u64) -> Result<u64> {
        let txn = self.db.begin_write()?;
        let mut removed = 0u64;
        {
            let mut table = txn.open_table(CO_ACCESS)?;
            // Collect stale keys first (can't modify while iterating)
            let stale_keys: Vec<(u64, u64)> = {
                let read_table = txn.open_table(CO_ACCESS)?;
                let mut keys = Vec::new();
                for result in read_table.iter()? {
                    let (key, value) = result?;
                    let record = deserialize_co_access(value.value())?;
                    if record.last_updated < cutoff_timestamp {
                        keys.push(key.value());
                    }
                }
                keys
            };
            // Note: need to re-open mutable table after read iteration ends
            // Actually: collect keys first, then delete in separate loop
            for key in &stale_keys {
                table.remove(*key)?;
                removed += 1;
            }
        }
        txn.commit()?;
        Ok(removed)
    }
}
```

Note: The cleanup method needs careful handling because redb doesn't allow modification during iteration. Collect stale keys first, then delete. The implementation may need adjustment -- the pseudocode shows the intent. The actual implementation should use a single write transaction with: (1) open read-only view to collect keys, (2) delete collected keys.

### read.rs additions

```rust
impl Store {
    /// Get all co-access partners for an entry, filtering by staleness.
    /// Per ADR-001: prefix scan for (entry, *) + full scan for (*, entry).
    pub fn get_co_access_partners(
        &self,
        entry_id: u64,
        staleness_cutoff: u64,
    ) -> Result<Vec<(u64, CoAccessRecord)>> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(CO_ACCESS)?;
        let mut partners = Vec::new();

        // Scan 1: pairs where entry_id is the min (prefix scan)
        for result in table.range((entry_id, 0u64)..=(entry_id, u64::MAX))? {
            let (key, value) = result?;
            let (_, partner_id) = key.value();
            let record = deserialize_co_access(value.value())?;
            if record.last_updated >= staleness_cutoff {
                partners.push((partner_id, record));
            }
        }

        // Scan 2: pairs where entry_id is the max (full table scan)
        for result in table.iter()? {
            let (key, value) = result?;
            let (min_id, max_id) = key.value();
            if max_id == entry_id && min_id != entry_id {
                let record = deserialize_co_access(value.value())?;
                if record.last_updated >= staleness_cutoff {
                    partners.push((min_id, record));
                }
            }
        }

        Ok(partners)
    }

    /// Get co-access statistics.
    /// Returns (total_pairs, active_pairs_after_staleness).
    pub fn co_access_stats(&self, staleness_cutoff: u64) -> Result<(u64, u64)> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(CO_ACCESS)?;
        let mut total = 0u64;
        let mut active = 0u64;

        for result in table.iter()? {
            let (_, value) = result?;
            total += 1;
            let record = deserialize_co_access(value.value())?;
            if record.last_updated >= staleness_cutoff {
                active += 1;
            }
        }

        Ok((total, active))
    }

    /// Get top N co-access pairs by count (non-stale only).
    pub fn top_co_access_pairs(
        &self,
        n: usize,
        staleness_cutoff: u64,
    ) -> Result<Vec<((u64, u64), CoAccessRecord)>> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(CO_ACCESS)?;
        let mut pairs = Vec::new();

        for result in table.iter()? {
            let (key, value) = result?;
            let record = deserialize_co_access(value.value())?;
            if record.last_updated >= staleness_cutoff {
                pairs.push((key.value(), record));
            }
        }

        pairs.sort_by(|a, b| b.1.count.cmp(&a.1.count));
        pairs.truncate(n);
        Ok(pairs)
    }
}
```

### lib.rs additions

```rust
pub use schema::{CO_ACCESS, CoAccessRecord, co_access_key, serialize_co_access, deserialize_co_access};
```
