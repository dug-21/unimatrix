# Pseudocode: C3 Store Usage Methods

## File: crates/unimatrix-store/src/write.rs

### Store::record_usage

```
impl Store {
    pub fn record_usage(
        &self,
        all_ids: &[u64],
        access_ids: &[u64],
        helpful_ids: &[u64],
        unhelpful_ids: &[u64],
        decrement_helpful_ids: &[u64],
        decrement_unhelpful_ids: &[u64],
    ) -> Result<()> {
        // Early return: nothing to do
        if all_ids.is_empty() {
            return Ok(());
        }

        let now = current_unix_timestamp_secs();
        let txn = self.db.begin_write()?;

        // Build HashSets for O(1) membership checks
        let access_set: HashSet<u64> = access_ids.iter().copied().collect();
        let helpful_set: HashSet<u64> = helpful_ids.iter().copied().collect();
        let unhelpful_set: HashSet<u64> = unhelpful_ids.iter().copied().collect();
        let dec_helpful_set: HashSet<u64> = decrement_helpful_ids.iter().copied().collect();
        let dec_unhelpful_set: HashSet<u64> = decrement_unhelpful_ids.iter().copied().collect();

        // Single pass over all_ids
        for &id in all_ids {
            // Read existing entry
            let old_bytes = {
                let table = txn.open_table(ENTRIES)?;
                match table.get(id)? {
                    Some(guard) => guard.value().to_vec(),
                    None => continue,  // Entry deleted between retrieval and usage recording
                }
            };

            let mut record = deserialize_entry(&old_bytes)?;

            // Always update last_accessed_at (no dedup)
            record.last_accessed_at = now;

            // Conditionally increment access_count (deduped by server layer)
            if access_set.contains(&id) {
                record.access_count += 1;
            }

            // Increment helpful_count (new vote or corrected vote)
            if helpful_set.contains(&id) {
                record.helpful_count += 1;
            }

            // Increment unhelpful_count
            if unhelpful_set.contains(&id) {
                record.unhelpful_count += 1;
            }

            // Decrement helpful_count (vote correction: was helpful, now unhelpful)
            if dec_helpful_set.contains(&id) {
                record.helpful_count = record.helpful_count.saturating_sub(1);
            }

            // Decrement unhelpful_count (vote correction: was unhelpful, now helpful)
            if dec_unhelpful_set.contains(&id) {
                record.unhelpful_count = record.unhelpful_count.saturating_sub(1);
            }

            // Rewrite entry
            let new_bytes = serialize_entry(&record)?;
            let mut table = txn.open_table(ENTRIES)?;
            table.insert(id, new_bytes.as_slice())?;
        }

        txn.commit()?;
        Ok(())
    }
}
```

### Store::record_feature_entries

```
impl Store {
    pub fn record_feature_entries(&self, feature: &str, entry_ids: &[u64]) -> Result<()> {
        if entry_ids.is_empty() {
            return Ok(());
        }

        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_multimap_table(FEATURE_ENTRIES)?;
            for &id in entry_ids {
                table.insert(feature, id)?;  // Idempotent: duplicate is no-op in multimap
            }
        }
        txn.commit()?;
        Ok(())
    }
}
```

### Import Updates

Add to imports at top of write.rs:
```
use crate::schema::FEATURE_ENTRIES;
```
