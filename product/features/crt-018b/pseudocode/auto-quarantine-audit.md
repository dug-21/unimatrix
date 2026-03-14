# Component: Auto-Quarantine Audit Event

**Files**:
- `crates/unimatrix-server/src/background.rs` (audit event emission)
- `crates/unimatrix-engine/src/effectiveness/mod.rs` (EffectivenessReport field addition)

**Purpose**: Emit structured audit events for every auto-quarantine action and tick-skip event.
Add the `auto_quarantined_this_cycle: Vec<u64>` field to `EffectivenessReport` to surface
auto-quarantine actions in `context_status` output.

---

## Part 1: `EffectivenessReport` Modification

**File**: `crates/unimatrix-engine/src/effectiveness/mod.rs`

### Add Constants

```
/// Additive utility boost for Effective-classified entries at query time.
/// Applied inside the status_penalty multiplication (ADR-003).
pub const UTILITY_BOOST: f64 = 0.05;

/// Additive utility boost for Settled-classified entries at query time.
/// Must be strictly less than co-access boost maximum (0.03) per Constraint 5.
pub const SETTLED_BOOST: f64 = 0.01;

/// Additive utility penalty magnitude for Ineffective and Noisy entries.
/// Applied as `-UTILITY_PENALTY` at query time.
pub const UTILITY_PENALTY: f64 = 0.05;
```

Place these after the existing constants block (after `NOISY_TRUST_SOURCES`).

### Add Field to `EffectivenessReport`

```
pub struct EffectivenessReport {
    pub by_category: Vec<(EffectivenessCategory, u32)>,
    pub by_source: Vec<SourceEffectiveness>,
    pub calibration: Vec<CalibrationBucket>,
    pub top_ineffective: Vec<EntryEffectiveness>,
    pub noisy_entries: Vec<EntryEffectiveness>,
    pub unmatched_entries: Vec<EntryEffectiveness>,
    pub data_window: DataWindow,

    /// Entry IDs quarantined by the most recent background maintenance tick.
    /// Populated by maintenance_tick() after auto-quarantine SQL writes complete.
    /// Empty when auto-quarantine is disabled or no entries crossed the threshold.
    /// Surfaced in context_status output (FR-14).
    #[serde(default)]
    pub auto_quarantined_this_cycle: Vec<u64>,   // NEW
}
```

The `#[serde(default)]` attribute ensures backward compatibility if any code constructs
`EffectivenessReport` with struct literal syntax (existing callers will fail to compile
without `auto_quarantined_this_cycle` if it lacks a default — `serde(default)` handles the
deserialization path but not construction). Implementation agent must add `..Default::default()`
or explicitly initialize the field at all construction sites.

Alternatively, define `Default` for `EffectivenessReport` so struct updates compile cleanly.
Or add the field with a default initializer pattern consistent with how the existing struct is
constructed in `status.rs`.

---

## Part 2: Audit Event Emission in `background.rs`

### Audit Event Constants

```
const SYSTEM_AGENT_ID: &str = "system";
const OP_AUTO_QUARANTINE: &str = "auto_quarantine";
const OP_TICK_SKIPPED: &str = "tick_skipped";
```

### Helper: `emit_auto_quarantine_audit`

```
function emit_auto_quarantine_audit(
    audit_log: &Arc<AuditLog>,
    entry_id: u64,
    title: String,
    topic: String,
    entry_category: String,   // knowledge category (decision, convention, etc.)
    classification: EffectivenessCategory,
    consecutive_cycles: u32,
    threshold: u32,
):
    let reason = format!(
        "auto-quarantine: entry '{}' (id={}, category={:?}, \
         consecutive_bad_cycles={}, topic={}) quarantined after {} consecutive \
         background maintenance ticks classified as {:?}",
        title, entry_id, classification, consecutive_cycles, topic,
        consecutive_cycles, classification
    )

    let event = AuditEvent {
        event_id: 0,          // assigned by AuditLog.log_event()
        timestamp: 0,         // assigned by AuditLog.log_event()
        session_id: String::new(),
        agent_id: SYSTEM_AGENT_ID.to_string(),
        operation: OP_AUTO_QUARANTINE.to_string(),
        target_ids: vec![entry_id],
        outcome: Outcome::Success,
        detail: format!(
            "entry_title={:?} entry_category={:?} classification={:?} \
             consecutive_cycles={} threshold={} reason={}",
            title, entry_category, classification,
            consecutive_cycles, threshold, reason
        ),
    }

    if let Err(e) = audit_log.log_event(event):
        tracing::warn!(
            entry_id = entry_id,
            error = %e,
            "auto-quarantine: failed to write audit event"
        )
        // Do not escalate — quarantine succeeded even if audit fails
```

### Audit Event Schema (FR-11)

The `detail` field encodes all required FR-11 fields in a structured string. The `AuditEvent`
type has a single `detail: String` field; all per-event metadata goes there:

| FR-11 Field | Placement in AuditEvent |
|-------------|------------------------|
| `operation` | `operation: "auto_quarantine"` |
| `agent_id` | `agent_id: "system"` |
| `entry_id` | `target_ids: [entry_id]` |
| `entry_title` | `detail` field as `entry_title=...` |
| `entry_category` | `detail` field as `entry_category=...` |
| `classification` | `detail` field as `classification=...` |
| `consecutive_cycles` | `detail` field as `consecutive_cycles=...` |
| `threshold` | `detail` field as `threshold=...` |
| `reason` | `detail` field as `reason=...` |

The `outcome` is `Outcome::Success` for a successful quarantine. If the quarantine SQL fails,
no audit event is emitted (the entry is skipped, as documented in auto-quarantine-guard.md).

### `tick_skipped` Audit Event (FR-13)

```
function emit_tick_skipped_audit(
    audit_log: &Arc<AuditLog>,
    error_reason: String,
):
    let event = AuditEvent {
        event_id: 0,
        timestamp: 0,
        session_id: String::new(),
        agent_id: SYSTEM_AGENT_ID.to_string(),
        operation: OP_TICK_SKIPPED.to_string(),
        target_ids: vec![],
        outcome: Outcome::Failure,
        detail: format!("background tick compute_report failed: {}", error_reason),
    }

    if let Err(e) = audit_log.log_event(event):
        tracing::warn!(error = %e, "failed to emit tick_skipped audit event")
```

Both `emit_auto_quarantine_audit` and `emit_tick_skipped_audit` use the constant
`SYSTEM_AGENT_ID = "system"`. The agent_id is never sourced from any request parameter or
external input (Security Risk 2 from RISK-TEST-STRATEGY).

---

## Part 3: Populating `auto_quarantined_this_cycle`

The `auto_quarantined_this_cycle` field on `EffectivenessReport` is populated in
`maintenance_tick` after all auto-quarantine SQL writes complete:

```
// After process_auto_quarantine() completes:
// Collect entry IDs that were successfully quarantined this cycle
// (These are the entries where quarantine_entry returned Ok)

// The process_auto_quarantine function returns successfully_quarantined: Vec<u64>
let successfully_quarantined: Vec<u64> = // returned from process_auto_quarantine

if let Some(ref mut eff_report) = report.effectiveness:
    eff_report.auto_quarantined_this_cycle = successfully_quarantined
// If effectiveness is None (unexpected), the field remains empty — no action needed
```

`process_auto_quarantine` must return the list of successfully quarantined entry IDs so
`maintenance_tick` can populate the field. Modify the function signature:

```
async function process_auto_quarantine(
    ...
) -> Vec<u64>:  // returns successfully quarantined entry IDs
    let mut quarantined: Vec<u64> = Vec::new()
    ...
    // On successful quarantine:
    quarantined.push(entry_id)
    ...
    return quarantined
```

---

## Error Handling

| Error | Behavior |
|-------|----------|
| `audit_log.log_event()` fails for auto_quarantine event | `tracing::warn!`; quarantine was already successful; do not rollback |
| `audit_log.log_event()` fails for tick_skipped event | `tracing::warn!`; EffectivenessState not modified; proceed |
| `auto_quarantined_this_cycle` field absent on `EffectivenessReport` construction | `#[serde(default)]` + Default trait ensures empty vec; no panic |
| `report.effectiveness` is None | `auto_quarantined_this_cycle` field not populated; `context_status` shows nothing for this tick |

---

## Key Test Scenarios

**Scenario 1 — Audit event schema correctness (AC-13)**
- Trigger auto-quarantine for entry 1 (title="test-entry", category=Ineffective, cycles=3)
- Read audit log; find event with operation="auto_quarantine"
- Assert agent_id == "system"
- Assert target_ids == [1]
- Assert detail contains: entry_title, entry_category, classification="Ineffective",
  consecutive_cycles=3, threshold=3
- Assert outcome == Success

**Scenario 2 — Tick-skipped event on compute_report error (FR-13, R-08)**
- Inject compute_report() error
- Read audit log; find event with operation="tick_skipped"
- Assert agent_id == "system"
- Assert outcome == Failure
- Assert detail contains the error string

**Scenario 3 — auto_quarantined_this_cycle populated in StatusReport (FR-14, R-12)**
- Trigger auto-quarantine for entry 5
- Call context_status; inspect StatusReport.effectiveness.auto_quarantined_this_cycle
- Assert [5] is present in the field

**Scenario 4 — auto_quarantined_this_cycle is empty when no quarantine fires**
- Tick fires; no entries cross threshold
- Assert auto_quarantined_this_cycle == []

**Scenario 5 — Constants have correct values (AC-03)**
- Assert UTILITY_BOOST == 0.05
- Assert SETTLED_BOOST == 0.01
- Assert UTILITY_PENALTY == 0.05
- Assert SETTLED_BOOST < 0.03 (co-access boost max)

**Scenario 6 — Audit event for quarantine failure is NOT written (R-03)**
- quarantine_entry() returns Err for entry 2
- Read audit log; assert no "auto_quarantine" event for entry 2
- (Audit event written only on success)

**Scenario 7 — agent_id is hardcoded constant, not user input**
- Code review: assert SYSTEM_AGENT_ID is a compile-time constant
- Assert no MCP request parameter influences the agent_id field in these events
