# Pseudocode: signal-dispatch

## Purpose

Extend `uds_listener.rs` to handle the full signal generation and processing pipeline on `SessionClose`. Adds `process_session_close` helper, `run_confidence_consumer`, `run_retrospective_consumer`, and a `RecordEvent` dispatch arm for `post_tool_use_rework_candidate` events.

## Files

- MODIFY `crates/unimatrix-server/src/uds_listener.rs`
- MODIFY `crates/unimatrix-server/src/server.rs` — add `pending_entries_analysis` field

## Context: Existing `dispatch_request` structure

The existing `dispatch_request` function pattern-matches on `HookRequest` variants. We add:
1. New arm for `RecordEvent { event }` where `event.event_type == "post_tool_use_rework_candidate"`
2. Modification to `SessionClose` arm to call `process_session_close`

## New: `PendingEntriesAnalysis` struct (in server.rs)

```rust
// In crates/unimatrix-server/src/server.rs

use unimatrix_observe::types::EntryAnalysis;

pub struct PendingEntriesAnalysis {
    pub entries: HashMap<u64, EntryAnalysis>,  // entry_id -> analysis
    pub created_at: u64,
}

impl PendingEntriesAnalysis {
    pub fn new() -> Self {
        PendingEntriesAnalysis {
            entries: HashMap::new(),
            created_at: now_secs(),
        }
    }

    /// Insert or update an EntryAnalysis, enforcing the 1000-entry cap.
    /// When cap reached, drops the entry with lowest rework_flag_count.
    pub fn upsert(&mut self, analysis: EntryAnalysis) {
        if self.entries.contains_key(&analysis.entry_id) {
            // Update existing
            let existing = self.entries.get_mut(&analysis.entry_id).unwrap();
            existing.rework_flag_count += analysis.rework_flag_count;
            existing.rework_session_count += analysis.rework_session_count;
            existing.success_session_count += analysis.success_session_count;
        } else {
            // Enforce cap before insert
            if self.entries.len() >= 1000 {
                // Drop entry with lowest rework_flag_count
                let min_key = self.entries.iter()
                    .min_by_key(|(_, v)| v.rework_flag_count)
                    .map(|(k, _)| *k);
                if let Some(k) = min_key {
                    self.entries.remove(&k);
                }
            }
            self.entries.insert(analysis.entry_id, analysis);
        }
    }

    /// Drain all entries and clear the map. Returns the drained entries.
    pub fn drain_all(&mut self) -> Vec<EntryAnalysis> {
        let entries: Vec<EntryAnalysis> = self.entries.values().cloned().collect();
        self.entries.clear();
        entries
    }
}
```

## Modified: `McpServer` struct (server.rs)

Add field to the struct:

```rust
pub struct McpServer {
    // existing fields...
    pub pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>,
}

// In McpServer::new() or constructor:
pending_entries_analysis: Arc::new(Mutex::new(PendingEntriesAnalysis::new())),
```

Pass `Arc<Mutex<PendingEntriesAnalysis>>` into UDS listener `start()` call — or share via `Arc<McpServer>`.

## Modified: `dispatch_request` in `uds_listener.rs`

### New arm: RecordEvent for rework candidate

```rust
HookRequest::RecordEvent { ref event }
    if event.event_type == "post_tool_use_rework_candidate" =>
{
    // Parse payload fields
    let tool_name = event.payload["tool_name"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let file_path = event.payload["file_path"]
        .as_str()
        .map(|s| s.to_string());
    let had_failure = event.payload["had_failure"]
        .as_bool()
        .unwrap_or(false);

    let rework_event = ReworkEvent {
        tool_name,
        file_path,
        had_failure,
        timestamp: event.timestamp,
    };

    session_registry.record_rework_event(&event.session_id, rework_event);
    HookResponse::Ack
}
```

Note: The existing `RecordEvent` arm (generic fallthrough) must remain for all other event types. Order the new arm BEFORE the generic one so pattern matching works correctly.

### Modified arm: SessionClose

```rust
HookRequest::SessionClose { ref session_id, ref outcome } => {
    let hook_outcome = outcome.as_deref().unwrap_or("");
    let response = process_session_close(
        session_id,
        hook_outcome,
        &store,
        &session_registry,
        &entry_store,
        &pending_entries_analysis,
    ).await;
    response
}
```

## New: `process_session_close` async helper

```rust
async fn process_session_close(
    session_id: &str,
    hook_outcome: &str,
    store: &Store,
    session_registry: &SessionRegistry,
    entry_store: &AsyncEntryStore<StoreAdapter>,
    pending: &Arc<Mutex<PendingEntriesAnalysis>>,
) -> HookResponse {
    // Step 1: Sweep stale sessions first (FR-09.1)
    let stale_outputs = session_registry.sweep_stale_sessions();
    for (_, stale_output) in stale_outputs {
        write_signals_to_queue(&stale_output, store).await;
    }

    // Step 2: Generate signals for the closing session (atomic — ADR-003)
    let maybe_output = session_registry.drain_and_signal_session(session_id, hook_outcome);

    if let Some(ref output) = maybe_output {
        // Step 3: Write signals to SIGNAL_QUEUE
        write_signals_to_queue(output, store).await;
        // Note: session is already removed from registry by drain_and_signal_session

        // Step 4: Run consumers (after queue is written)
        run_confidence_consumer(store, entry_store, pending).await;
        run_retrospective_consumer(store, pending, entry_store).await;
    } else {
        // Session was already cleared (idempotent) or not found — no-op
    }

    HookResponse::Ack
}
```

## Helper: `write_signals_to_queue`

```rust
async fn write_signals_to_queue(
    output: &SignalOutput,
    store: &Store,
) {
    use unimatrix_store::{SignalRecord, SignalType, SignalSource};
    use std::time::{SystemTime, UNIX_EPOCH};

    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Only write if there are entry_ids (FR-04.6)
    let (entry_ids, signal_type, signal_source) = match output.final_outcome {
        SessionOutcome::Success if !output.helpful_entry_ids.is_empty() => {
            (output.helpful_entry_ids.clone(), SignalType::Helpful, SignalSource::ImplicitOutcome)
        }
        SessionOutcome::Rework if !output.flagged_entry_ids.is_empty() => {
            (output.flagged_entry_ids.clone(), SignalType::Flagged, SignalSource::ImplicitRework)
        }
        _ => return, // No entries to signal
    };

    let record = SignalRecord {
        signal_id: 0, // Allocated by insert_signal
        session_id: output.session_id.clone(),
        created_at,
        entry_ids,
        signal_type,
        signal_source,
    };

    if let Err(e) = store.insert_signal(&record) {
        tracing::warn!(
            session_id = %output.session_id,
            error = %e,
            "write_signals_to_queue: failed to insert signal"
        );
    }
}
```

## New: `run_confidence_consumer`

```rust
/// Drain Helpful signals from SIGNAL_QUEUE and apply helpful_count increments.
/// Also updates success_session_count in PendingEntriesAnalysis (FR-06.2b).
async fn run_confidence_consumer(
    store: &Store,
    entry_store: &AsyncEntryStore<StoreAdapter>,
    pending: &Arc<Mutex<PendingEntriesAnalysis>>,
) {
    // Step 1: Drain Helpful signals (all in one transaction — FR-05.5)
    let signals = match store.drain_signals(SignalType::Helpful) {
        Ok(s) => s,
        Err(e) => {
            // Log warning, skip — unprocessed signals remain in queue (FR-05.3)
            tracing::warn!(error = %e, "run_confidence_consumer: drain_signals failed");
            return;
        }
    };

    if signals.is_empty() {
        return;
    }

    // Step 2: Deduplicate entry_ids across all drained signals
    // (multiple signals may reference same entry — process each entry once)
    let mut all_entry_ids: HashSet<u64> = HashSet::new();
    for signal in &signals {
        for &entry_id in &signal.entry_ids {
            all_entry_ids.insert(entry_id);
        }
    }

    // Step 3: For each unique entry_id, increment helpful_count via crt-002 path
    for entry_id in &all_entry_ids {
        // source = "hook" distinguishes from explicit MCP votes ("mcp")
        if let Err(e) = entry_store.record_helpfulness(*entry_id, true, "hook").await {
            // Entry deleted since injection — skip with warning (FR-05.4)
            tracing::warn!(
                entry_id = entry_id,
                error = %e,
                "run_confidence_consumer: record_helpfulness failed, skipping"
            );
        }
    }

    // Step 4: Update success_session_count in PendingEntriesAnalysis (FR-06.2b)
    // For each entry_id in drained Helpful signals, increment success_session_count
    {
        let mut pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
        for signal in &signals {
            for &entry_id in &signal.entry_ids {
                if let Some(existing) = pending_guard.entries.get_mut(&entry_id) {
                    existing.success_session_count += 1;
                } else {
                    // Fetch title + category from store to create new EntryAnalysis
                    // (async fetch can't happen inside lock — use a collect-then-insert pattern)
                    // Record a placeholder; async fetch happens after lock release
                    // See note below on fetch-then-insert pattern
                }
            }
        }
    }

    // Fetch titles/categories for new entries (not yet in PendingEntriesAnalysis)
    // Do this OUTSIDE the lock to avoid holding it during async I/O
    let entries_needing_fetch: Vec<u64> = {
        let pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
        let mut unique_ids: HashSet<u64> = HashSet::new();
        for signal in &signals {
            for &entry_id in &signal.entry_ids {
                if !pending_guard.entries.contains_key(&entry_id) {
                    unique_ids.insert(entry_id);
                }
            }
        }
        unique_ids.into_iter().collect()
    };

    for entry_id in entries_needing_fetch {
        // Fetch EntryRecord for title + category
        let meta = entry_store.get_entry(entry_id).await;
        let (title, category) = match meta {
            Ok(Some(record)) => (record.title.clone(), record.category.clone()),
            _ => (String::new(), String::new()),
        };
        let analysis = EntryAnalysis {
            entry_id,
            title,
            category,
            rework_flag_count: 0,
            injection_count: 0,
            success_session_count: 1,
            rework_session_count: 0,
        };
        let mut pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
        // Use upsert to handle the case where it was added between our check and now
        if pending_guard.entries.contains_key(&entry_id) {
            pending_guard.entries.get_mut(&entry_id).unwrap().success_session_count += 1;
        } else {
            pending_guard.upsert(analysis);
        }
    }
}
```

## New: `run_retrospective_consumer`

```rust
/// Drain Flagged signals and update PendingEntriesAnalysis.
async fn run_retrospective_consumer(
    store: &Store,
    pending: &Arc<Mutex<PendingEntriesAnalysis>>,
    entry_store: &AsyncEntryStore<StoreAdapter>,
) {
    // Step 1: Drain Flagged signals
    let signals = match store.drain_signals(SignalType::Flagged) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "run_retrospective_consumer: drain_signals failed");
            return;
        }
    };

    if signals.is_empty() {
        return;
    }

    // Step 2: Collect entry_ids needing fetch (not yet in PendingEntriesAnalysis)
    let entries_needing_fetch: Vec<u64> = {
        let pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
        signals.iter()
            .flat_map(|s| s.entry_ids.iter().copied())
            .filter(|id| !pending_guard.entries.contains_key(id))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    };

    // Step 3: Fetch metadata for new entries (outside lock — async I/O)
    let mut fetched: HashMap<u64, (String, String)> = HashMap::new();
    for entry_id in entries_needing_fetch {
        match entry_store.get_entry(entry_id).await {
            Ok(Some(record)) => { fetched.insert(entry_id, (record.title.clone(), record.category.clone())); }
            _ => { fetched.insert(entry_id, (String::new(), String::new())); }
        }
    }

    // Step 4: Apply updates to PendingEntriesAnalysis (under lock)
    {
        let mut pending_guard = pending.lock().unwrap_or_else(|e| e.into_inner());
        for signal in &signals {
            for &entry_id in &signal.entry_ids {
                if pending_guard.entries.contains_key(&entry_id) {
                    // Update existing
                    let existing = pending_guard.entries.get_mut(&entry_id).unwrap();
                    existing.rework_flag_count += 1;
                    existing.rework_session_count += 1;
                } else {
                    // Create new from fetched metadata
                    let (title, category) = fetched
                        .get(&entry_id)
                        .cloned()
                        .unwrap_or_default();
                    let analysis = EntryAnalysis {
                        entry_id,
                        title,
                        category,
                        rework_flag_count: 1,
                        injection_count: 0,
                        success_session_count: 0,
                        rework_session_count: 1,
                    };
                    pending_guard.upsert(analysis);
                }
            }
        }
    }
}
```

## Modified: `context_retrospective` handler in `server.rs`

Before calling `build_report`, drain `pending_entries_analysis`:

```rust
// In the context_retrospective handler:
let entries_analysis = {
    let mut pending = self.pending_entries_analysis.lock().unwrap_or_else(|e| e.into_inner());
    let drained = pending.drain_all();
    if drained.is_empty() { None } else { Some(drained) }  // FR-10.5
};

let report = build_report(
    &feature_cycle,
    &records,
    metrics,
    hotspots,
    baseline,
    entries_analysis,  // NEW 6th parameter
);
```

## Error Handling

- `process_session_close`: never panics; all errors logged as warnings; always returns `HookResponse::Ack`
- `run_confidence_consumer`: drain failure → log + return; per-entry failure → log + continue
- `run_retrospective_consumer`: drain failure → log + return; fetch failure → use empty strings
- `write_signals_to_queue`: insert failure → log + continue (session_id in warning)

## Key Test Scenarios

1. `test_process_session_close_success` — stop hook with "success" → Helpful signal written, helpful_count incremented
2. `test_process_session_close_rework` — rework threshold crossed → Flagged signal, helpful_count NOT incremented
3. `test_process_session_close_empty_session` — no injections → no signal written, no consumer errors
4. `test_stale_sweep_runs_before_close` — stale session cleaned up before current session processed
5. `test_run_confidence_consumer_increments_helpful_count` — drain Helpful signal for 2 entries → both helpful_counts +1
6. `test_run_confidence_consumer_success_session_count` — FR-06.2b: success_session_count updated in PendingEntriesAnalysis
7. `test_run_confidence_consumer_deleted_entry` — entry_id=999 not in store → warning, no panic, continues
8. `test_run_retrospective_consumer_creates_entry_analysis` — Flagged signal → EntryAnalysis created with correct counts
9. `test_pending_entries_analysis_cap` — 1001 entries → cap enforced, lowest rework_flag_count dropped
10. `test_rework_candidate_dispatch` — RecordEvent post_tool_use_rework_candidate → record_rework_event called
