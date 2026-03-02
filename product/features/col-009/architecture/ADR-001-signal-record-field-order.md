## ADR-001: SignalRecord Field Order Locked at Shipping

### Context

`SignalRecord` is bincode v2 serialized (positional encoding — field order determines binary layout). Once records are written to the SIGNAL_QUEUE redb table and the feature ships, reordering or removing fields would produce deserialization failures for any records written by an older binary. This is the same constraint that applies to `EntryRecord` (established in nxs-004 and enforced through three migrations).

However, SIGNAL_QUEUE is ephemeral: records are drained and deleted by consumers after every SessionClose. In practice, no records survive a server restart. This makes the risk lower than for ENTRIES, but it is not zero: a record could be written and not yet consumed if the server crashes between write and drain. A binary upgrade in that window would encounter a field-order mismatch.

SR-01 (Scope Risk Assessment) flagged this risk and recommended locking the field order before any data is written.

### Decision

The `SignalRecord` struct fields are declared in this order and may never be reordered or removed:

```rust
pub struct SignalRecord {
    pub signal_id: u64,        // field 0 — monotonic key, also stored as value
    pub session_id: String,    // field 1 — which session generated this signal
    pub created_at: u64,       // field 2 — Unix seconds
    pub entry_ids: Vec<u64>,   // field 3 — entries receiving this signal
    pub signal_type: SignalType,    // field 4 — Helpful | Flagged
    pub signal_source: SignalSource, // field 5 — ImplicitOutcome | ImplicitRework
}
```

New fields may only be appended at the end with `#[serde(default)]` when a schema migration bumps the version. A comment in the source marks the struct as layout-frozen:

```rust
// LAYOUT FROZEN: bincode v2 positional encoding. Fields may only be APPENDED.
// See ADR-001 (col-009). Do not reorder or remove fields.
```

`SignalType` and `SignalSource` enums use explicit integer discriminants to prevent accidental reordering:
```rust
#[repr(u8)]
pub enum SignalType { Helpful = 0, Flagged = 1 }
#[repr(u8)]
pub enum SignalSource { ImplicitOutcome = 0, ImplicitRework = 1 }
```

### Consequences

- Field order is a maintenance constraint. Any future field addition requires both a schema version bump and an appended field — the usual migration process.
- Explicit discriminants on enums prevent a class of serialization bugs if enum variants are ever reordered.
- The `// LAYOUT FROZEN` comment provides a conspicuous warning to future contributors.
- Easier: deserialization bugs from field reordering are prevented by the comment contract.
- Harder: future extensions require the migration apparatus (schema bump + migrate function), even for minor additions.
