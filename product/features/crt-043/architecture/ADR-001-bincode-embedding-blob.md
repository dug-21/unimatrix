## ADR-001: SQLite Embedding Blob Serialization — bincode Vec<f32>

### Context

crt-043 introduces the first SQLite BLOB column containing an embedding vector:
`cycle_events.goal_embedding`. This is a novel pattern in the codebase. The existing VECTOR_MAP
table does NOT store embedding bytes — it stores only `entry_id → hnsw_data_id` integer pairs.
Primary entry embeddings live exclusively in the HNSW in-memory index and on-disk binary files.
There is therefore no existing SQLite embedding blob convention to follow; this ADR creates it.

Three serialization options were considered:

1. **Raw f32 bytes** — write `vec.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>()`
2. **bincode** — use the `bincode` crate already present in the workspace
3. **serde_json** — encode as a JSON array of floats

Group 6 will need additional embedding blobs (e.g., `goal_cluster` embeddings). Whatever is
decided here becomes the codebase pattern that Group 6 must follow or explicitly supersede.

SR-02 from SCOPE-RISK-ASSESSMENT.md: without a specified serialization contract, each downstream
read site independently reverse-engineers the format. Silent divergence is undetectable at
compile time — a read site using the wrong bincode config will deserialize into garbage floats
with no error.

### Decision

Use `bincode` v2 with `serde` for all SQLite embedding blob columns, with the following exact
API:

**Encode (write path):**
```rust
bincode::serde::encode_to_vec(vec: Vec<f32>, bincode::config::standard())
```

**Decode (read path):**
```rust
let (vec, _): (Vec<f32>, _) =
    bincode::serde::decode_from_slice(bytes, bincode::config::standard())?;
```

**Canonical helper signatures** (ship with the write path in `unimatrix-store`):
```rust
pub(crate) fn encode_goal_embedding(vec: Vec<f32>) -> Result<Vec<u8>, bincode::error::EncodeError>
pub(crate) fn decode_goal_embedding(bytes: &[u8]) -> Result<Vec<f32>, bincode::error::DecodeError>
```

Every new embedding BLOB column introduced after crt-043 must have analogous paired helpers
(`encode_X_embedding` / `decode_X_embedding`) defined in the same PR as the write path. The
helpers are `pub(crate)` — they are not part of the public `unimatrix-store` API but must be
co-located with the write path so Group 6 agents find them without independent research.

Rationale for bincode over raw f32 bytes:
1. **Self-describing length** — bincode length-prefixes `Vec<f32>`, so read sites do not need
   to know the embedding dimension out-of-band. Raw bytes require every read site to hard-code
   384 dimensions (or whatever the current model produces).
2. **Already in workspace** — no new dependency. bincode v2 with serde is used in `crt-006`
   and `nxs-001` (ADR-002) for adaptation state and EntryRecord persistence respectively.
3. **Model upgrade path** — if the embedding dimension changes (e.g., 384 → 768 with a model
   upgrade), bincode blobs are distinguishable by byte length. Raw f32 bytes would silently
   deserialize into the wrong number of floats with no error; a 768-dim blob read as 384-dim
   would truncate without warning.
4. **Precedent for Group 6** — Group 6 needs a `goal_cluster` table with centroid embeddings.
   This ADR gives them a pattern to cite rather than making a second independent decision.

Rationale for bincode over serde_json:
- JSON float arrays are 10–15× larger than binary representations for equivalent precision.
  A 384-dim f32 vector is ~1.5KB in bincode; ~6KB in JSON. Multiplied across many cycle_events
  rows, the storage difference is non-trivial.

**Primary embedding follow-up (out of scope for crt-043):**
If primary entry embeddings were stored as bincode BLOB columns in the `entries` table (keyed
by `entry_id`), `get_embedding()` would become O(1) instead of the current O(N) HNSW scan,
directly addressing SR-01 in crt-042 (PPR expander latency). Storage cost is trivial
(~7,000 entries × 1,536 bytes ≈ 10MB). The HNSW index would remain authoritative for ANN
search; the SQLite blob would be a parallel random-access path. This ADR establishes the
encoding pattern that such a feature would follow. Evaluate alongside crt-042 delivery.
Note: this evaluation requires its own ADR and migration; it is not implied by crt-043.

**Cold-start behavior:** Pre-v21 `cycle_events` rows have `goal_embedding = NULL`. Read sites
in Group 6/7 must handle NULL explicitly. The NULL baseline is documented and accepted in scope.

### Consequences

Easier:
- Group 6 agents have a defined, tested encode/decode API to call — no format research required
- Model dimension changes are forward-compatible without migration
- bincode decode errors surface immediately (no silent garbage floats)

Harder:
- bincode blobs are not human-readable in SQLite CLI inspection; use the decode helper
- Every future embedding BLOB column requires the paired helper discipline to be maintained;
  architectural review should verify the pattern is followed
