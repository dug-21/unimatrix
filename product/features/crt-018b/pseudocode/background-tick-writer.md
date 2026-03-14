# Component: Background Tick Writer

**File**: `crates/unimatrix-server/src/background.rs` (modified)

**Purpose**: After `compute_report()` succeeds in `maintenance_tick()`, extract the
`EffectivenessReport` and write the new classification map and consecutive-bad-cycle counters
to `EffectivenessState` under a write lock. On `compute_report()` error, emit a `tick_skipped`
audit event without modifying `EffectivenessState`.

This component encompasses only the tick writer logic. Auto-quarantine guard logic (the scan
and SQL calls that follow the write) is documented separately in `auto-quarantine-guard.md`,
but implementation agents must understand both live in the same `maintenance_tick` function.

---

## Imports Added to `background.rs`

```
use crate::services::effectiveness::EffectivenessStateHandle;
use unimatrix_engine::effectiveness::EffectivenessCategory;
use crate::infra::audit::{AuditEvent, AuditLog, Outcome};
use std::sync::Arc;
```

---

## Modified: `spawn_background_tick` Signature

Add `effectiveness_state: EffectivenessStateHandle` as final parameter:

```
pub fn spawn_background_tick(
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    session_registry: Arc<SessionRegistry>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    pending_entries: Arc<Mutex<PendingEntriesAnalysis>>,
    tick_metadata: Arc<Mutex<TickMetadata>>,
    training_service: Option<Arc<TrainingService>>,
    confidence_state: ConfidenceStateHandle,
    effectiveness_state: EffectivenessStateHandle,   // NEW
) -> JoinHandle<()>
```

Propagate through `background_tick_loop` and `run_single_tick` in the same manner as
`confidence_state`. The `effectiveness_state` parameter passes by reference (`&EffectivenessStateHandle`)
into `maintenance_tick`.

---

## Modified: `maintenance_tick` Signature

```
async fn maintenance_tick(
    status_svc: &StatusService,
    session_registry: &SessionRegistry,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    pending_entries: &Arc<Mutex<PendingEntriesAnalysis>>,
    effectiveness_state: &EffectivenessStateHandle,  // NEW
    audit_log: &Arc<AuditLog>,                       // NEW — for tick_skipped event
    auto_quarantine_cycles: u32,                     // NEW — threshold (0 = disabled)
    store: &Arc<Store>,                              // NEW — for quarantine_entry SQL
) -> Result<(), ServiceError>
```

NOTE: `audit_log` and `store` are already present in the broader tick context and can be threaded
through from `run_single_tick`. The implementation agent should verify existing parameter lists
and avoid duplication.

---

## Modified: `maintenance_tick` Body

### Pseudocode

```
async function maintenance_tick(...):

    // Step 1: Attempt to compute the status report
    let result = status_svc.compute_report(None, None, false).await

    match result:
        Err(error):
            // ADR-002: hold semantics — do NOT modify EffectivenessState
            // Emit tick_skipped audit event (FR-13, SR-07)
            emit_tick_skipped_audit(audit_log, error.to_string())
            // Fall through to existing maintenance path with no state update
            // (compute_report error means no active_entries either; return early or
            //  skip maintenance — implementation agent should verify existing behavior
            //  to determine whether to return Err or continue with partial state)
            return Err(error)

        Ok((mut report, active_entries)):

            // Step 2: Extract EffectivenessReport from the StatusReport
            // report.effectiveness is Option<EffectivenessReport>
            // Per SPECIFICATION FR-03: write occurs if effectiveness data is present
            let to_quarantine: Vec<(u64, u32, EffectivenessCategory)> = []
            // to_quarantine: Vec of (entry_id, cycle_count, category) to quarantine after lock drop

            if let Some(effectiveness_report) = report.effectiveness.as_ref():

                // Step 3: Acquire write lock on EffectivenessState
                // Lock ordering: write lock is held only for in-memory updates (NFR-02)
                {
                    let mut state = effectiveness_state.write()
                        .unwrap_or_else(|e| e.into_inner())

                    // Step 4: Build new categories map from report
                    // The report contains a flat list of EntryEffectiveness records
                    // Source: effectiveness_report contains per-entry classifications
                    // Implementation agent: verify field name in EffectivenessReport that
                    // provides the per-entry Vec<EntryEffectiveness>
                    // Based on existing struct: likely reconstruct from top_ineffective,
                    // noisy_entries, unmatched_entries, and the by_category summary
                    // FLAG: The architecture references "per-entry classification map" but
                    // EffectivenessReport.by_category is aggregate counts, not per-entry.
                    // The full per-entry list is split across top_ineffective, noisy_entries,
                    // unmatched_entries. StatusService.compute_report() must expose the full
                    // per-entry slice. See OPEN QUESTION 1 below.

                    // Assuming a per-entry Vec<EntryEffectiveness> is accessible as
                    // `effectiveness_report.all_entries` (see open question):
                    let new_categories: HashMap<u64, EffectivenessCategory> = HashMap::new()
                    for entry_effectiveness in effectiveness_report.all_entries:
                        new_categories.insert(entry_effectiveness.entry_id,
                                              entry_effectiveness.category)

                    // Step 5: Replace categories map
                    state.categories = new_categories

                    // Step 6: Update consecutive_bad_cycles
                    // Remove entries that are no longer in the active classification set
                    // (quarantined, deprecated, or deleted since last tick)
                    let active_ids: HashSet<u64> = state.categories.keys().copied().collect()
                    state.consecutive_bad_cycles.retain(|id, _| active_ids.contains(id))

                    // Increment or reset per entry
                    for (entry_id, category) in &state.categories:
                        match category:
                            Ineffective | Noisy =>
                                // Increment counter (FR-09)
                                let counter = state.consecutive_bad_cycles.entry(entry_id).or_insert(0)
                                *counter += 1
                            Effective | Settled | Unmatched =>
                                // Reset counter on recovery (FR-09)
                                state.consecutive_bad_cycles.remove(entry_id)
                                // or: state.consecutive_bad_cycles.insert(entry_id, 0)
                                // Prefer .remove() to keep the map sparse

                    // Step 7: Collect entries that cross the auto-quarantine threshold
                    // This scan happens INSIDE the write lock (counters already updated)
                    // The actual SQL calls happen AFTER the lock is released (NFR-02, R-13)
                    if auto_quarantine_cycles > 0:
                        for (entry_id, &count) in &state.consecutive_bad_cycles:
                            if count >= auto_quarantine_cycles:
                                let category = state.categories.get(entry_id).copied()
                                    .unwrap_or(EffectivenessCategory::Ineffective)
                                // Only quarantine Ineffective or Noisy (AC-14, R-11)
                                match category:
                                    Ineffective | Noisy =>
                                        to_quarantine.push((entry_id, count, category))
                                    _ => () // should not occur — counter resets on non-bad
                                            // but defensive check prevents R-11

                    // Step 8: Increment generation counter
                    state.generation += 1

                    // write lock drops here (end of scope)
                    // CRITICAL: Do NOT hold write lock past this point (NFR-02, R-13)
                }
                // state write guard is now dropped

            // Step 9: Auto-quarantine SQL writes (write lock is NOT held)
            // Delegated to auto-quarantine-guard.md logic
            // to_quarantine is populated; pass to auto-quarantine function
            process_auto_quarantine(
                to_quarantine,
                effectiveness_state,
                effectiveness_report,
                store,
                audit_log,
                auto_quarantine_cycles,
            ).await

    // Step 10: Run existing maintenance logic (unchanged)
    status_svc.run_maintenance(
        &active_entries,
        &mut report,
        session_registry,
        entry_store,
        pending_entries,
    ).await?

    Ok(())
```

---

## Helper: `emit_tick_skipped_audit`

```
function emit_tick_skipped_audit(audit_log: &Arc<AuditLog>, error_reason: String):
    let event = AuditEvent {
        event_id: 0,          // assigned by AuditLog.log_event()
        timestamp: 0,         // assigned by AuditLog.log_event()
        session_id: String::new(),
        agent_id: "system".to_string(),
        operation: "tick_skipped".to_string(),
        target_ids: vec![],
        outcome: Outcome::Failure,
        detail: format!("background tick compute_report failed: {}", error_reason),
    }
    if let Err(log_err) = audit_log.log_event(event):
        tracing::warn!("failed to emit tick_skipped audit event: {}", log_err)
```

The `"system"` agent_id is a hardcoded constant, never sourced from request parameters
(Security Risk 2 from RISK-TEST-STRATEGY). Use a module-level constant:

```
const SYSTEM_AGENT_ID: &str = "system";
```

---

## `AUTO_QUARANTINE_CYCLES` Configuration

Read and validate at server startup, before `spawn_background_tick` is called. Place validation
in `main.rs` or a dedicated config parsing function:

```
function parse_auto_quarantine_cycles() -> Result<u32, StartupError>:
    let raw = std::env::var("UNIMATRIX_AUTO_QUARANTINE_CYCLES")
        .unwrap_or_else(|_| "3".to_string())

    let value: u32 = raw.parse()
        .map_err(|_| StartupError::config(
            "UNIMATRIX_AUTO_QUARANTINE_CYCLES",
            "must be a non-negative integer",
        ))?

    if value > 1000:
        return Err(StartupError::config(
            "UNIMATRIX_AUTO_QUARANTINE_CYCLES",
            "value > 1000 is implausibly large; set to 0 to disable or use a value in [1, 1000]",
        ))

    // value == 0: auto-quarantine disabled (intentional)
    // value in [1, 1000]: valid
    return Ok(value)
```

This validation enforces Constraint 14 (startup error on > 1000, security DoS mitigation).
A value of `0` is valid and disables auto-quarantine (AC-12).

---

## Error Handling

| Error | Behavior |
|-------|----------|
| `compute_report()` returns `Err` | emit `tick_skipped` audit event; skip EffectivenessState write; return `Err` from `maintenance_tick` (ADR-002) |
| `EffectivenessState` write lock poisoned | `.unwrap_or_else(|e| e.into_inner())` — proceed with stale state |
| Audit log write for `tick_skipped` fails | `tracing::warn!` — do not escalate; this is observability infrastructure |
| `effectiveness_report.all_entries` is empty | Write empty categories map and clear all counters — this is a valid state (all entries unclassified) |

---

## Key Test Scenarios

**Scenario 1 — Successful tick updates categories and counters (AC-09)**
- Seed an `EffectivenessState` with generation=0
- Call maintenance_tick with a mock `compute_report` returning one Ineffective entry (id=1)
- Assert `state.categories[1] == Ineffective`
- Assert `state.consecutive_bad_cycles[1] == 1`
- Assert `state.generation == 1`

**Scenario 2 — Counter increments across multiple bad ticks (AC-09)**
- Tick 1: entry 1 = Ineffective. Assert counter[1] = 1.
- Tick 2: entry 1 = Ineffective. Assert counter[1] = 2.
- Tick 3: entry 1 = Effective (recovery). Assert counter[1] is absent or 0.

**Scenario 3 — Tick error holds counters unchanged (ADR-002, AC-09)**
- Set state: counter[1] = 2
- Call maintenance_tick with mock `compute_report` returning Err
- Assert counter[1] still == 2 (not incremented, not reset)
- Assert generation unchanged
- Assert `tick_skipped` audit event was written

**Scenario 4 — context_status does NOT advance counters (R-04, AC-01)**
- Call `status_svc.compute_report()` directly N=10 times
- Assert `effectiveness_state.consecutive_bad_cycles` remains empty throughout
- (StatusService must not write EffectivenessState)

**Scenario 5 — Removed entry is cleaned from counter map (AC-15)**
- Set state: categories[1] = Ineffective, counter[1] = 2
- Tick where entry 1 is absent from compute_report output
- Assert counter[1] is removed from map

**Scenario 6 — `AUTO_QUARANTINE_CYCLES > 1000` causes startup error**
- Set env var to "1001"
- Assert `parse_auto_quarantine_cycles()` returns `Err`
- Server does not start

**Scenario 7 — `AUTO_QUARANTINE_CYCLES = 0` produces valid config**
- Set env var to "0"
- Assert `parse_auto_quarantine_cycles()` returns `Ok(0)`
- Assert auto-quarantine is disabled (no calls to quarantine_entry in subsequent ticks)
