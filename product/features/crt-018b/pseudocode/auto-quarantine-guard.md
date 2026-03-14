# Component: Auto-Quarantine Guard

**File**: `crates/unimatrix-server/src/background.rs` (modified, same file as tick writer)

**Purpose**: After `EffectivenessState` is updated in `maintenance_tick`, scan
`consecutive_bad_cycles` for entries that have reached the `AUTO_QUARANTINE_CYCLES` threshold
and trigger synchronous quarantine for each via `store.quarantine_entry()`. Operates after the
write lock is released (NFR-02, R-13). Each quarantine is independent — failure of one does
not abort the remaining candidates.

This component is implemented as a function called from `maintenance_tick` after the write
lock block exits. The `to_quarantine` list is collected inside the write lock and passed to
this function outside it.

---

## Function: `process_auto_quarantine`

Called from `maintenance_tick` after the `EffectivenessState` write lock has been dropped.

```
async function process_auto_quarantine(
    to_quarantine: Vec<(u64, u32, EffectivenessCategory)>,
    // (entry_id, cycle_count, category) for each entry at threshold

    effectiveness_state: &EffectivenessStateHandle,
    // Used to reset counters for successfully quarantined entries

    effectiveness_report: &EffectivenessReport,
    // Source of entry title/topic for audit event detail

    store: &Arc<Store>,
    // Synchronous quarantine path

    audit_log: &Arc<AuditLog>,
    // Audit event emission

    auto_quarantine_cycles: u32,
    // Threshold value — used in audit event "threshold" field
):
    if to_quarantine.is_empty() or auto_quarantine_cycles == 0:
        return

    for (entry_id, cycle_count, category) in to_quarantine:

        // Defense in depth: verify category is still Ineffective or Noisy (AC-14, R-11)
        // This check re-reads the in-memory state (no SQL). The write lock was released
        // but no other writer can have changed state since (background tick is sole writer
        // and we are still within the same tick invocation).
        // The check is redundant but defensive given the RISK-TEST-STRATEGY R-11 concern.
        match category:
            Noisy | Ineffective => () // proceed
            _ => continue             // stale entry in to_quarantine — skip

        // Fetch entry metadata for audit event (title, topic, entry category field)
        // Source: look up in effectiveness_report.all_entries by entry_id
        // See OPEN QUESTION 1 in background-tick-writer.md
        let (title, topic, entry_category) =
            find_entry_metadata_in_report(effectiveness_report, entry_id)
            .unwrap_or_else(|| (
                format!("(id={})", entry_id),   // fallback if not found
                "(unknown)".to_string(),
                "(unknown)".to_string(),
            ))

        // Quarantine via synchronous store path, inside spawn_blocking (NFR-05)
        // store.quarantine_entry() takes: entry_id, pre_quarantine_status, reason, agent_id
        // Implementation agent: verify exact signature of Store::quarantine_entry()
        let reason = format!(
            "auto-quarantine: {} consecutive {:?} classifications in background maintenance tick",
            cycle_count, category
        )
        let store_clone = Arc::clone(store)
        let quarantine_result = tokio::task::spawn_blocking(move ||:
            store_clone.quarantine_entry(entry_id, reason)
            // Implementation agent: pass agent_id = "system" if the API accepts it
            // If quarantine_entry only takes (entry_id, reason): caller attribution in
            // audit event is sufficient for traceability
        ).await

        match quarantine_result:
            Ok(Ok(())):
                // Quarantine succeeded
                // Reset consecutive_bad_cycles counter for this entry (idempotent)
                // Re-acquire write lock for counter reset
                {
                    let mut state = effectiveness_state.write()
                        .unwrap_or_else(|e| e.into_inner())
                    state.consecutive_bad_cycles.remove(&entry_id)
                    // do NOT increment generation here — categories are unchanged
                    // Counter reset does not need to be observed by search paths
                }
                // write lock drops

                // Emit audit event (Component 6)
                emit_auto_quarantine_audit(
                    audit_log,
                    entry_id,
                    title,
                    topic,
                    entry_category,
                    category,
                    cycle_count,
                    auto_quarantine_cycles,
                )

            Ok(Err(store_error)):
                // quarantine_entry returned an error (e.g., entry already quarantined,
                // or entry was deleted between tick computation and now)
                tracing::warn!(
                    entry_id = entry_id,
                    error = %store_error,
                    "auto-quarantine: quarantine_entry failed, skipping entry"
                )
                // Do NOT reset counter — entry may still qualify next tick
                // Do NOT abort loop — continue to next candidate (R-03 mitigation)

            Err(join_error):
                // spawn_blocking panicked
                tracing::warn!(
                    entry_id = entry_id,
                    error = %join_error,
                    "auto-quarantine: spawn_blocking join error, skipping entry"
                )
                // Same: skip and continue
```

---

## Helper: `find_entry_metadata_in_report`

```
function find_entry_metadata_in_report(
    report: &EffectivenessReport,
    entry_id: u64,
) -> Option<(String, String, String)>:
    // Search across top_ineffective, noisy_entries, unmatched_entries
    // See OPEN QUESTION 1: if a full per-entry list is added, search that instead
    for entry_eff in report.top_ineffective.iter()
        .chain(report.noisy_entries.iter()):
        if entry_eff.entry_id == entry_id:
            return Some((
                entry_eff.title.clone(),
                entry_eff.topic.clone(),
                // EntryEffectiveness does not have a knowledge category field
                // (it has trust_source). Implementation agent must fetch the
                // knowledge category from the store or use a separate lookup.
                // See OPEN QUESTION 2.
                entry_eff.trust_source.clone(),  // fallback pending open question
            ))
    None
```

---

## Counter Reset Constraint

The write lock is re-acquired briefly for counter removal after a successful quarantine. This
is acceptable because:
- The counter reset is a single `HashMap::remove()` operation (< 1 microsecond)
- `generation` is NOT incremented — search/briefing paths do not need to re-clone categories
  for a counter change
- The lock is held only for in-memory mutation, not during any SQL call (NFR-02 preserved)

If re-acquiring the write lock per entry is considered too expensive for bulk quarantine, the
implementation agent may batch the counter removals into a single write lock acquisition after
the entire loop completes. Either approach is correct.

---

## `AUTO_QUARANTINE_CYCLES` Guard

The top-level guard `if auto_quarantine_cycles == 0: return` is the single enforcement point
for the disabled-quarantine configuration (AC-12). The `to_quarantine` list may have been
built (entries at threshold) but this guard prevents any SQL call from being issued.

The threshold check `consecutive_bad_cycles[id] >= auto_quarantine_cycles` was performed inside
the write lock in `maintenance_tick` (background-tick-writer.md). The `to_quarantine` list
therefore only contains entries that legitimately crossed the threshold at the time of the tick.

---

## Fire-and-Forget Confidence Recompute

After each successful quarantine, trigger a fire-and-forget confidence recompute via
`ConfidenceService`:

```
// After successful quarantine, before emitting audit event:
confidence_service.recompute(&[entry_id])
// This is non-blocking (spawn_blocking internally); no await needed
```

The `confidence_service` reference must be threaded through `maintenance_tick` from
`run_single_tick`. Implementation agent should verify whether `ConfidenceService` is already
accessible in `maintenance_tick` or if it needs to be added.

Note from RISK-TEST-STRATEGY Integration Risk 2: the fire-and-forget confidence recompute may
race with `run_maintenance()` confidence batch refresh that follows in the same tick. This is
an accepted risk — the fire-and-forget result may be overwritten by the batch refresh. The
batch refresh is idempotent and will incorporate the quarantined entry's exclusion.

---

## Error Handling

| Error | Behavior |
|-------|----------|
| `quarantine_entry()` returns `Err` | Log warning; skip that entry; continue loop (R-03) |
| `spawn_blocking` panics | `JoinError` caught; log warning; skip that entry; continue loop (R-03) |
| `find_entry_metadata_in_report` returns None | Use fallback strings in audit event; do not abort quarantine |
| Counter reset write lock poisoned | `.unwrap_or_else(|e| e.into_inner())` — proceed with potentially stale counter |
| `to_quarantine` is empty | Early return; no spawn_blocking, no lock acquisition |

---

## Key Test Scenarios

**Scenario 1 — Auto-quarantine fires after N consecutive bad ticks (AC-10)**
- Seed entry 1 as Ineffective, consecutive_bad_cycles[1] = auto_quarantine_cycles (e.g., 3)
- Call process_auto_quarantine with entry 1 in to_quarantine
- Assert store.quarantine_entry(1, ...) was called
- Assert counter[1] removed from state
- Assert audit event written

**Scenario 2 — Bulk quarantine: failure on one does not abort others (R-03)**
- to_quarantine contains entries [1, 2, 3]
- Entry 2's quarantine_entry() returns Err
- Assert entries 1 and 3 are quarantined
- Assert entry 2 is NOT quarantined and counter is NOT reset for entry 2
- Assert audit events written for entries 1 and 3 only

**Scenario 3 — Auto-quarantine disabled (AC-12)**
- Set auto_quarantine_cycles = 0
- to_quarantine has 5 entries all at high counter values
- Assert no call to quarantine_entry()
- Assert no counter resets

**Scenario 4 — Category restriction: only Ineffective/Noisy quarantined (AC-14, R-11)**
- to_quarantine contains entry 1 with category=Settled (stale, defensive check)
- Assert quarantine_entry NOT called for entry 1
- (This tests the defensive category re-check at top of loop)

**Scenario 5 — Write lock released before SQL call (R-13)**
- Code review: assert write lock guard goes out of scope before quarantine_entry() call
- Concurrency test: while quarantine is running (simulated slow SQL), issue search()
  and assert search completes within 10ms (not blocked by quarantine write path)

**Scenario 6 — Counter increment only after successful quarantine (R-03)**
- Entry 1 in to_quarantine with cycle_count=3
- quarantine_entry returns Ok
- Assert counter[1] is removed
- Entry 2 in to_quarantine; quarantine_entry returns Err
- Assert counter[2] is NOT removed
