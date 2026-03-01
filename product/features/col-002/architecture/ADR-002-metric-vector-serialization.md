## ADR-002: MetricVector Serialization Boundary

### Context

MetricVector must be stored in the OBSERVATION_METRICS redb table as bytes and later retrieved for the retrospective report (and future col-002b baseline comparison). The MetricVector struct is defined in `unimatrix-observe` (per ADR-001). The store crate handles `&[u8]` values.

Two options exist:
1. MetricVector implements Serialize/Deserialize. The server crate (which depends on both) calls `bincode::serde::encode_to_vec` before storing and `bincode::serde::decode_from_slice` after retrieval.
2. The observe crate provides `serialize_metric_vector` / `deserialize_metric_vector` helper functions, and the server calls those.

### Decision

Option 2: The observe crate provides `serialize_metric_vector(mv: &MetricVector) -> Result<Vec<u8>>` and `deserialize_metric_vector(bytes: &[u8]) -> Result<MetricVector>` functions. This mirrors the pattern established in `unimatrix-store/src/schema.rs` with `serialize_entry` / `deserialize_entry`.

MetricVector uses `#[serde(default)]` on all fields to support forward-compatible deserialization when col-002b adds new metric fields. This follows the same convention as EntryRecord.

### Consequences

- **Easier**: Consistent serialization pattern across the workspace. Clear ownership -- the crate that defines the type owns its serialization. col-002b can add fields to MetricVector without breaking existing stored data (serde defaults).
- **Harder**: Caller must import the helper functions. Bincode config must match (both use `bincode::config::standard()`). Any MetricVector schema change requires considering stored data compatibility (same as EntryRecord).
