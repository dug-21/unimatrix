# Pseudocode: C3 Quarantine Tool

## File: crates/unimatrix-server/src/tools.rs

### QuarantineParams

```
struct QuarantineParams {
    id: i64,                   // required
    reason: Option<String>,
    action: Option<String>,    // "quarantine" (default) or "restore"
    agent_id: Option<String>,
    format: Option<String>,
}
```

### context_quarantine handler

```
fn context_quarantine(params):
    // 1. Identity
    identity = resolve_agent(params.agent_id)

    // 2. Capability check (Admin required)
    require_capability(identity.agent_id, Admin)

    // 3. Validate params
    validate_quarantine_params(params)

    // 4. Parse format
    format = parse_format(params.format)

    // 5. Parse action (default: "quarantine")
    action = parse_action(params.action)  // -> Quarantine | Restore

    // 6. Fetch entry
    entry_id = validated_id(params.id)
    entry = store.get(entry_id)  // -> Err(EntryNotFound) if not found

    // 7. Action dispatch
    match action:
        Quarantine:
            if entry.status == Status::Quarantined:
                // Idempotent: already quarantined
                return format_quarantine_success(entry, "already quarantined", format)

            if entry.status != Status::Active:
                return Err("only active entries can be quarantined")

            // Atomic quarantine + audit
            updated = quarantine_with_audit(entry_id, identity.agent_id, params.reason)

            // Recompute confidence (fire-and-forget)
            spawn_blocking:
                recompute and update_confidence(entry_id)

            return format_quarantine_success(updated, params.reason, format)

        Restore:
            if entry.status != Status::Quarantined:
                return Err("entry is not quarantined")

            // Atomic restore + audit
            updated = restore_with_audit(entry_id, identity.agent_id, params.reason)

            // Recompute confidence (fire-and-forget)
            spawn_blocking:
                recompute and update_confidence(entry_id)

            return format_restore_success(updated, params.reason, format)
```

## File: crates/unimatrix-server/src/server.rs

### quarantine_with_audit

```
fn quarantine_with_audit(entry_id, agent_id, reason):
    // Single write transaction
    txn = store.begin_write()

    // Read entry
    entries_table = txn.open_table(ENTRIES)
    bytes = entries_table.get(entry_id)
    entry = deserialize_entry(bytes)
    old_status = entry.status

    // Update entry status
    entry.status = Status::Quarantined
    entry.modified_by = agent_id
    entry.updated_at = now()
    // Note: NOT bumping version/hash (same as deprecate - metadata-only)
    serialized = serialize_entry(entry)
    entries_table.insert(entry_id, serialized)

    // Update STATUS_INDEX: remove old, add new
    status_table = txn.open_table(STATUS_INDEX)
    status_table.remove((old_status as u8, entry_id))
    status_table.insert((Status::Quarantined as u8, entry_id), ())

    // Update COUNTERS
    decrement_counter(txn, status_counter_key(old_status), 1)
    increment_counter(txn, status_counter_key(Status::Quarantined), 1)

    // Write audit event
    audit.write_in_txn(txn, AuditEvent {
        operation: "context_quarantine",
        target_ids: [entry_id],
        detail: format("quarantined: {reason}"),
        outcome: Success,
        agent_id: agent_id,
    })

    txn.commit()
    return entry
```

### restore_with_audit

```
fn restore_with_audit(entry_id, agent_id, reason):
    // Same pattern as quarantine_with_audit but:
    // - old_status = Quarantined
    // - new_status = Active
    // - detail = "restored: {reason}"
    // Everything else identical
```

## File: crates/unimatrix-server/src/validation.rs

### validate_quarantine_params

```
fn validate_quarantine_params(params):
    if params.id is missing or <= 0:
        return Err("id must be a positive integer")
    if params.action is Some:
        if params.action not in ["quarantine", "restore"]:
            return Err("action must be 'quarantine' or 'restore'")
    if params.reason is Some and params.reason.len() > 1000:
        return Err("reason exceeds maximum length")
```

### parse_action

```
fn parse_action(action: Option<String>) -> QuarantineAction:
    match action:
        None => QuarantineAction::Quarantine  // default
        Some(s) => match s.to_lowercase():
            "quarantine" => QuarantineAction::Quarantine
            "restore" => QuarantineAction::Restore
            _ => Err("invalid action")

enum QuarantineAction { Quarantine, Restore }
```

## File: crates/unimatrix-server/src/response.rs

### format_quarantine_success / format_restore_success

```
fn format_quarantine_success(entry, reason, format):
    match format:
        Summary => "Quarantined #{entry.id} | {entry.title}"
        Markdown =>
            "## Entry Quarantined\n\n"
            "**Entry:** #{entry.id} - {entry.title}\n"
            "**Status:** Quarantined\n"
            if reason: "**Reason:** {reason}\n"
        Json => { "quarantined": true, "entry": entry_to_json(entry), "reason": reason }

fn format_restore_success(entry, reason, format):
    // Same structure but "Restored" instead of "Quarantined"
    // status shows "Active"
```
