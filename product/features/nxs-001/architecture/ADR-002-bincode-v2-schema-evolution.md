# ADR-002: bincode v2 Serialization with serde default for Schema Evolution

## Status

Accepted

## Context

EntryRecord is stored as `&[u8]` in redb's ENTRIES table. We need a serialization format that:

1. **Compact** — entries are 500-2000 bytes of content plus metadata; binary format minimizes storage overhead
2. **Fast** — serialize/deserialize on every read/write; must be sub-microsecond
3. **Schema-evolvable** — future milestones (crt-001, col-001, dsn-001) add fields to EntryRecord. Old data must deserialize with new fields defaulting to their type defaults, without any migration step
4. **Rust-native** — integrate cleanly with serde derive macros

Alternatives considered:

- **JSON (serde_json)** — Human-readable but 3-5x larger and 5-10x slower than binary formats. Schema evolution works well with serde, but storage bloat is unnecessary for a non-human-inspected format.
- **MessagePack (rmp-serde)** — Compact binary, good schema evolution. Slightly larger than bincode due to field tagging. A reasonable alternative but less idiomatic in the Rust ecosystem.
- **postcard** — Compact, fast, no_std friendly. However, its schema evolution story is weaker — it uses a positional format that breaks when fields are reordered or inserted in the middle.
- **bincode v1** — Well-known but uses a fixed positional layout that does not handle `#[serde(default)]` correctly for schema evolution.
- **bincode v2** — Improved API, supports `#[serde(default)]` when using serde-compatible configuration, compact binary format.

## Decision

Use **bincode v2** with serde `Encode`/`Decode` derives and `#[serde(default)]` on all fields that may be added in future milestones.

The serialization configuration uses bincode's serde-compatible mode to ensure `#[serde(default)]` attributes are respected during deserialization. This means:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct EntryRecord {
    pub id: u64,
    pub title: String,
    pub content: String,
    // ... required fields ...
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub last_accessed_at: u64,
    #[serde(default)]
    pub access_count: u32,
    #[serde(default)]
    pub supersedes: Option<u64>,
    // Future fields always added with #[serde(default)]
}
```

When a new field is added to EntryRecord (e.g., `project_id: Option<u64>` for dsn-001), old serialized records that lack this field will deserialize successfully with the field set to `None`. No migration, no version checking, no data rewriting.

## Consequences

**Positive:**
- **Zero-migration schema evolution.** Adding `#[serde(default)]` fields to EntryRecord is a source-only change. All existing serialized data continues to deserialize correctly.
- **Compact binary format.** ~2-4x smaller than JSON for typical entries.
- **Fast.** Sub-microsecond encode/decode for our entry sizes.
- **Type-safe.** Compile-time derive macros prevent accidental schema mismatches.

**Negative:**
- **Not human-readable.** Cannot inspect stored entries without deserialization. Mitigated by: the `context_get` API provides human-readable access; we can add a debug/export tool if needed.
- **Field removal is breaking.** Removing a field from EntryRecord would break deserialization of records that contain it. Mitigated by: we never remove fields, only add them. Deprecated fields remain in the struct with `#[serde(default)]`.
- **Field reordering is breaking.** bincode's positional encoding means field order matters. Mitigated by: fields are always appended at the end, never reordered.
- **bincode v2 API differences from v1.** The spike research (ASS-003) used v1 patterns. Mitigated by: v2's API is well-documented and this is a greenfield crate with no migration from v1.

**Rules for future schema changes:**
1. New fields are always appended at the end of EntryRecord
2. New fields always have `#[serde(default)]`
3. Fields are never removed or reordered
4. Field types are never changed (add a new field instead)
