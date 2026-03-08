# crt-011: Pseudocode — consumer-dedup

## Component: Consumer Dedup Fixes

### Change 1: run_confidence_consumer — Step 4 session dedup

**Location:** `crates/unimatrix-server/src/uds/listener.rs`, function `run_confidence_consumer`

```pseudocode
// BEFORE Step 4 (after Step 3, before the three-pass structure):
let mut session_counted: HashSet<(String, u64)> = HashSet::new()

// Pass 1 (under lock) — existing loop, modified:
for signal in signals:
    for entry_id in signal.entry_ids:
        if entry exists in pending_guard.entries:
            // NEW: only increment if (session_id, entry_id) pair not yet counted
            if session_counted.insert((signal.session_id.clone(), entry_id)):
                existing.success_session_count += 1
        else:
            needing_fetch.push(entry_id)

// Pass 2 (outside lock) — UNCHANGED: fetch metadata for unknown entries

// Pass 3 (under lock) — existing loop, modified:
for (entry_id, (title, category)) in fetched:
    if entry exists in pending_guard.entries:
        // Entry was added between passes — check dedup
        // NOTE: we need to check all signals that referenced this entry_id
        // Since we're iterating fetched entries (not signals), we need to
        // find which signals referenced this entry_id and check each.
        // However, the simpler approach: since Pass 1 already inserted
        // all (session_id, entry_id) pairs for entries that EXISTED,
        // for entries that needed fetch, we now iterate signals again:
        for signal in signals:
            if signal.entry_ids.contains(entry_id):
                if session_counted.insert((signal.session_id.clone(), entry_id)):
                    existing.success_session_count += 1
    else:
        // New entry — create with initial counts
        // Count unique sessions that reference this entry
        let mut count = 0u64
        for signal in signals:
            if signal.entry_ids.contains(entry_id):
                if session_counted.insert((signal.session_id.clone(), entry_id)):
                    count += 1
        analysis = EntryAnalysis { ..., success_session_count: count, ... }
        pending_guard.upsert(analysis)
```

**WAIT — Simpler approach for Pass 3:**

The current Pass 3 code only iterates `fetched` entries (not signals). It does one increment per fetched entry. But there could be multiple signals referencing the same entry_id. The fix needs to handle this correctly.

**Revised Pass 3 pseudocode (simpler, matches current loop structure):**

Since Pass 1 already added `needing_fetch` entries (potentially duplicated across signals), and Pass 3 iterates the deduplicated `fetched` HashMap, we need to go back to iterating signals for entries that were fetched:

```pseudocode
// Pass 3 (under lock):
if fetched is not empty:
    let mut pending_guard = lock pending
    for signal in signals:
        for entry_id in signal.entry_ids:
            if entry_id not in fetched:
                continue  // already handled in Pass 1
            if let Some(existing) = pending_guard.entries.get_mut(entry_id):
                // Entry exists (either added between passes, or by earlier iteration)
                if session_counted.insert((signal.session_id.clone(), entry_id)):
                    existing.success_session_count += 1
            else:
                // First time seeing this entry — create it
                let (title, category) = fetched[entry_id]
                let should_count = session_counted.insert((signal.session_id.clone(), entry_id))
                let analysis = EntryAnalysis {
                    entry_id,
                    title, category,
                    success_session_count: if should_count { 1 } else { 0 },
                    ...zeros...
                }
                pending_guard.upsert(analysis)
```

**ISSUE:** This changes the loop structure from iterating `fetched` to iterating `signals`. Need to preserve the original structure as much as possible.

**FINAL APPROACH (minimal change):**

Keep the existing loop structure. The key insight: Pass 3 currently does ONE increment per fetched entry_id. After the fix, it should increment once per unique (session_id, entry_id) pair. Since we need session_id, we must bring the signal context into Pass 3.

The cleanest minimal change:
1. In Pass 1, when an entry needs fetch, also record which signals reference it
2. In Pass 3, use that mapping to apply session-aware dedup

But even simpler: just change Pass 3 to iterate signals for fetched entries:

```rust
// Pass 3 (back under lock):
if !fetched.is_empty() {
    let mut pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
    for signal in &signals {
        for &entry_id in &signal.entry_ids {
            // Only process entries that were in the fetched set
            if let Some((title, category)) = fetched.get(&entry_id) {
                if let Some(existing) = pending_guard.entries.get_mut(&entry_id) {
                    // Entry exists (added between passes or by earlier signal)
                    if session_counted.insert((signal.session_id.clone(), entry_id)) {
                        existing.success_session_count += 1;
                    }
                } else {
                    // New entry — insert it
                    let should_count = session_counted.insert((signal.session_id.clone(), entry_id));
                    let analysis = EntryAnalysis {
                        entry_id,
                        title: title.clone(),
                        category: category.clone(),
                        rework_flag_count: 0,
                        injection_count: 0,
                        success_session_count: if should_count { 1 } else { 0 },
                        rework_session_count: 0,
                    };
                    pending_guard.upsert(analysis);
                }
            }
        }
    }
}
```

### Change 2: run_retrospective_consumer — session dedup

**Location:** Same file, function `run_retrospective_consumer`

```pseudocode
// Before Step 4:
let mut session_counted: HashSet<(String, u64)> = HashSet::new()

// Step 4 (under lock) — modified:
for signal in signals:
    for entry_id in signal.entry_ids:
        if entry exists in pending_guard.entries:
            // rework_flag_count: always increment (event counter, ADR-002)
            existing.rework_flag_count += 1
            // rework_session_count: dedup per (session_id, entry_id)
            if session_counted.insert((signal.session_id.clone(), entry_id)):
                existing.rework_session_count += 1
        else:
            // New entry
            let session_is_new = session_counted.insert((signal.session_id.clone(), entry_id))
            let analysis = EntryAnalysis {
                entry_id,
                title, category,
                rework_flag_count: 1,  // always count the event
                injection_count: 0,
                success_session_count: 0,
                rework_session_count: if session_is_new { 1 } else { 0 },
            }
            pending_guard.upsert(analysis)
```

## Key Constraints

- HashSet must be declared BEFORE the three-pass structure in run_confidence_consumer so it persists across all passes
- session_id is cloned for HashSet keys (acceptable: bounded by 10K signal queue cap)
- The existing HashSet<u64> for helpful_count (Step 2) is NOT modified
- Code comments must explain rework_flag_count vs rework_session_count distinction
