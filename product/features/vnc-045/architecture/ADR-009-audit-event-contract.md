## ADR-009: The complete, generic `context_tag` audit-event shape is a retrofit-hard contract shipped in full now

### Context
The audit log is append-only (`AuditEvent`, schema.rs:360; `metadata: String` JSON, default `"{}"`, no hash chain). Reshaping a record's meaning after rows exist is the painful, retrofit-HARD class of change — unlike additive config, which retrofits cheaply. vnc-045 ships the `context_tag` MECHANISM and DEFERS the `protected_tags` value-hygiene policy. The human's ruling: because a future `protected_tags` feature must add **ZERO** audit surface, the complete generic event shape is specified and emitted in full NOW, even though nothing enforces tag hygiene yet. The mechanism's audit contract is the single genuinely retrofit-hard piece kept in scope; getting its shape right now is what lets the deferred policy bolt on for free later.

### Decision
Every `context_tag` mutation emits exactly one `AuditEvent`, fire-and-forget after commit (mirror store_correct.rs:98-102), with this **complete, generic** shape:

```
AuditEvent {
  operation      = "context_tag",
  target_ids     = [id],
  agent_id,                       // self-declared, audit-only (ADR-008)
  capability_used,                // Capability::Write
  timestamp, session_id, credential_type ("none"), outcome,
  metadata = {                    // JSON in the existing metadata: String column
    action,        // "add" | "remove" | "replace"
    namespace,     // substring of `tag` before the first ':' — else null. DERIVED, never validated.
    tag,           // the full tag string as written
    prior_value,   // MANDATORY on remove and replace; null on add
    new_value      // the value written; null on remove
  }
}
```

Binding rules:
- **`namespace` is derived from the tag prefix** (substring before the first `:`, else `null`) and is **recorded but NEVER validated** — value-opacity (ADR-008 point 4). No allow-list, no vocabulary check gates the write; the namespace is a forensic breadcrumb only.
- **`prior_value` is always emitted on `remove` and `replace`** (the record must be sufficient to reconstruct what changed): on `remove` it is the removed tag's value (always non-null — the client names the exact tag); on `replace` it is the value evicted from the derived namespace, non-null whenever a prior existed and `null` only in the degenerate no-prior case (a colon-less tag, or a namespace that held nothing — `replace` then degrades to `add`, ADR-004). On `add` it is `null`. The invariant a future policy may rely on: an `action:"replace"` row carries a non-null `prior_value` exactly when a prior was evicted.
- **`new_value` is null on `remove`**, the written value otherwise.
- **No schema change:** the shape lives entirely in the existing `metadata: String` JSON column (vnc-014 provenance fields carry the rest). No `AuditEvent` column is added.
- **`'context_tag'` is added to the `audit_write_count_since` op-list** (audit.rs:84) as a latent (non-enforcing) signal.

The contract is **retrofit-locked**: a future `protected_tags` value-hygiene feature reuses this record verbatim and adds **no field** to it. A rejected-by-hygiene write simply never reaches this emit point (it fails at the value-opacity seam, ADR-008 point 4); the audit shape does not change to describe rejection.

### Consequences
- Easier: the future `protected_tags` feature inherits a forensic record that already carries `action`, `namespace`, and prior/new values — the append-only substrate is shaped correctly on day one, avoiding a painful record migration.
- Easier: a reader can reconstruct what changed, when, and by whom from one row; `replace` is one legible record with both prior and new values (ADR-004).
- Accepted: attribution is declarative (`agent_id` self-declared, `credential_type="none"`) — the record's forensic value is capped until credentialed transport lands (ADR-008). This is the shape's known bound, not a defect in the shape.
- Cross-references ADR-001 (the write that precedes emit), ADR-004 (replace → one event with prior_value), ADR-008 (audit is the primary control; value-opacity seam gates before emit).
