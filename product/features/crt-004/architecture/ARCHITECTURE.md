# Architecture: crt-004 Co-Access Boosting

## System Overview

crt-004 adds co-access intelligence to Unimatrix -- tracking which entries are retrieved together and using that signal to improve search and briefing results. The feature introduces 1 new redb table, 1 new server module, extends the existing confidence module, and modifies 3 tool handlers.

The architecture spans 2 crates:
- **unimatrix-store**: New CO_ACCESS table definition, co-access record serialization, and co-access read/write methods on Store.
- **unimatrix-server**: New `coaccess.rs` module for boost computation, extensions to `confidence.rs` for the seventh factor, extensions to `usage_dedup.rs` for pair dedup, and modifications to `tools.rs` and `server.rs` for recording and boosting.

Design principles:
1. **Relational data stays relational** -- Co-access is a pairwise relationship. It lives in CO_ACCESS, not on EntryRecord. No schema migration.
2. **Query-time computation** -- Co-access affinity is computed at search/briefing time from CO_ACCESS lookups. Not pre-computed, not cached. Keeps the data model clean.
3. **Existing patterns, extended** -- Co-access recording plugs into `record_usage_for_entries`. Session dedup extends `UsageDedup`. Boost extends the existing rerank step. No new execution patterns.

```
Claude Code / MCP Client
        |
        | stdio (JSON-RPC 2.0)
        v
unimatrix-server (binary)
  |-- coaccess.rs (NEW)          -- Co-access boost computation, pair generation
  |-- confidence.rs              -- EXTENDED: seventh factor (co-access affinity)
  |-- usage_dedup.rs             -- EXTENDED: co-access pair dedup
  |-- tools.rs                   -- EXTENDED: search + briefing co-access boost
  |-- server.rs                  -- EXTENDED: co-access recording in usage pipeline
  |     |
  |     | record_usage_for_entries() records co-access pairs
  |     v
  v
unimatrix-store
  |-- schema.rs                  -- EXTENDED: CO_ACCESS table definition + CoAccessRecord
  |-- write.rs                   -- EXTENDED: record_co_access(), cleanup_stale_co_access()
  |-- read.rs                    -- EXTENDED: get_co_access_partners(), co_access_stats()
  |-- db.rs                      -- EXTENDED: open CO_ACCESS table in Store::open()
```

## Component Breakdown

### C1: CO_ACCESS Table and Storage (`unimatrix-store`)

**Scope**: New redb table, record type, and read/write methods.

**Table definition** in `schema.rs`:
```rust
/// Co-access pair tracking: (min_entry_id, max_entry_id) -> bincode bytes.
/// Keys are ordered (smaller ID first) to deduplicate symmetric pairs.
pub const CO_ACCESS: TableDefinition<(u64, u64), &[u8]> =
    TableDefinition::new("co_access");
```

**Record type** in `schema.rs`:
```rust
/// Co-access pair metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoAccessRecord {
    /// Number of times this pair was co-retrieved.
    pub count: u32,
    /// Unix timestamp of most recent co-retrieval.
    pub last_updated: u64,
}
```

Serialization: `bincode::serde::encode_to_vec` / `bincode::serde::decode_from_slice` with `standard()` config, matching the EntryRecord pattern.

**Helper functions** in `schema.rs`:
```rust
pub fn serialize_co_access(record: &CoAccessRecord) -> Result<Vec<u8>>
pub fn deserialize_co_access(bytes: &[u8]) -> Result<CoAccessRecord>

/// Create an ordered pair key: (min, max).
pub fn co_access_key(a: u64, b: u64) -> (u64, u64) {
    if a <= b { (a, b) } else { (b, a) }
}
```

**Store::open()** in `db.rs`: Add `txn.open_table(CO_ACCESS)` alongside existing 12 tables (becomes 13th table).

**Write methods** in `write.rs`:
```rust
impl Store {
    /// Record co-access for a set of entry IDs.
    /// Generates k*(k-1)/2 pairs from the input IDs (capped at max_pairs_from entries).
    /// For each pair: read existing record, increment count, update last_updated.
    /// If no record exists, create with count=1.
    /// All pairs written in a single transaction.
    pub fn record_co_access(&self, entry_ids: &[u64], max_pairs_from: usize) -> Result<()>

    /// Remove co-access pairs with last_updated older than cutoff timestamp.
    /// Returns count of removed pairs.
    pub fn cleanup_stale_co_access(&self, cutoff_timestamp: u64) -> Result<u64>
}
```

**Read methods** in `read.rs`:
```rust
impl Store {
    /// Get all co-access partners for a given entry ID with their co-access counts.
    /// Returns pairs where the given entry is either the min or max ID.
    /// Filters out pairs with last_updated older than staleness_cutoff.
    /// Returns Vec<(partner_entry_id, CoAccessRecord)>.
    pub fn get_co_access_partners(
        &self,
        entry_id: u64,
        staleness_cutoff: u64,
    ) -> Result<Vec<(u64, CoAccessRecord)>>

    /// Get co-access statistics for status reporting.
    /// Returns (total_pairs, active_pairs_after_staleness_filter).
    pub fn co_access_stats(&self, staleness_cutoff: u64) -> Result<(u64, u64)>

    /// Get top-N co-access pairs by count (for status report clusters).
    /// Filters by staleness.
    pub fn top_co_access_pairs(
        &self,
        n: usize,
        staleness_cutoff: u64,
    ) -> Result<Vec<((u64, u64), CoAccessRecord)>>
}
```

**get_co_access_partners scan strategy**: CO_ACCESS keys are `(u64, u64)`. To find all partners of entry X, we need pairs where X is either the first or second element. Two range scans:
1. `(X, 0)..=(X, u64::MAX)` -- pairs where X is the min ID
2. Full table scan for pairs where X is the max ID is too expensive.

**ADR-001** addresses this: we use a supplementary approach. See ADR-001 for the decision.

### C2: Session Dedup Extension (`unimatrix-server`)

**Scope**: Extend `UsageDedup` to track co-access pairs per agent per session.

**Changes to `usage_dedup.rs`**:

Add to `DedupState`:
```rust
struct DedupState {
    access_counted: HashSet<(String, u64)>,
    vote_recorded: HashMap<(String, u64), bool>,
    co_access_recorded: HashSet<(u64, u64)>,  // NEW: ordered pairs, agent-independent
}
```

New method on `UsageDedup`:
```rust
/// Filter co-access pairs to only those not yet recorded this session.
/// Returns subset of input pairs not yet seen.
/// Marks returned pairs as recorded.
/// Pairs are agent-independent (co-access is global, not per-agent).
pub fn filter_co_access_pairs(&self, pairs: &[(u64, u64)]) -> Vec<(u64, u64)>
```

Co-access dedup is agent-independent because co-access is a global signal (per SCOPE non-goal: "no per-agent co-access profiles"). If agent A retrieves entries {1,2,3} and agent B also retrieves {1,2,3}, both contribute to the co-access count -- but only on first retrieval per session. The dedup key is the ordered pair itself, not (agent, pair).

### C3: Co-Access Recording (`unimatrix-server`)

**Scope**: Extend `record_usage_for_entries` to record co-access pairs.

**Changes to `server.rs`**:

After existing usage recording (Step 3) and feature entry recording (Step 4), add Step 5:

```rust
// Step 5: Co-access recording (fire-and-forget)
if entry_ids.len() >= 2 {
    // Generate ordered pairs from entry_ids (capped at MAX_CO_ACCESS_ENTRIES)
    let pairs = coaccess::generate_pairs(entry_ids, MAX_CO_ACCESS_ENTRIES);
    // Filter through session dedup
    let new_pairs = self.usage_dedup.filter_co_access_pairs(&pairs);
    if !new_pairs.is_empty() {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || {
            store.record_co_access_pairs(&new_pairs)
        }).await;
    }
}
```

Where `record_co_access_pairs` is a lower-level Store method that takes pre-computed pairs (different from `record_co_access` which generates pairs internally). Both methods exist:
- `record_co_access(&self, entry_ids, max_pairs_from)` -- generates pairs, for direct use
- `record_co_access_pairs(&self, pairs: &[(u64, u64)])` -- records pre-computed pairs, for use after dedup

**Constants** in `coaccess.rs`:
```rust
/// Maximum entries to consider for co-access pair generation.
/// 10 entries = 45 pairs. Beyond this, pairs are not generated.
pub const MAX_CO_ACCESS_ENTRIES: usize = 10;

/// Default staleness threshold for co-access pairs: 30 days in seconds.
pub const CO_ACCESS_STALENESS_SECONDS: u64 = 30 * 24 * 3600;
```

### C4: Co-Access Boost Module (`unimatrix-server`)

**Scope**: New module `coaccess.rs` containing pair generation, boost computation, and briefing integration.

**Pair generation**:
```rust
/// Generate ordered pairs from entry IDs, capped at max_entries.
/// Returns Vec<(min_id, max_id)>.
pub fn generate_pairs(entry_ids: &[u64], max_entries: usize) -> Vec<(u64, u64)>
```

Takes the first `max_entries` IDs from the slice, generates all pairwise combinations.

**Search boost computation**:
```rust
/// Compute co-access boost scores for search results.
///
/// Takes the current ranked results and returns a map of entry_id -> boost.
/// Uses the top `anchor_count` results as anchors.
/// For each result that is a co-access partner of any anchor, computes a boost
/// based on co-access count (log-transformed, capped).
///
/// Returns HashMap<u64, f32> where values are additive boosts in [0.0, MAX_CO_ACCESS_BOOST].
pub fn compute_search_boost(
    anchor_ids: &[u64],
    result_ids: &[u64],
    store: &Store,
    staleness_cutoff: u64,
) -> HashMap<u64, f32>
```

**Boost formula** (see ADR-002):
```
raw_boost = ln(1 + co_access_count) / ln(1 + MAX_MEANINGFUL_CO_ACCESS)
capped_boost = min(raw_boost, 1.0) * MAX_CO_ACCESS_BOOST
```

Where `MAX_CO_ACCESS_BOOST = 0.03` and `MAX_MEANINGFUL_CO_ACCESS = 20.0`. The log-transform matches the anti-gaming pattern from crt-002's `usage_score`. The cap prevents any co-access signal from dominating similarity.

**Briefing boost**:
```rust
/// Compute co-access boost for briefing results.
/// Same algorithm as search boost but with a smaller multiplier.
pub fn compute_briefing_boost(
    anchor_ids: &[u64],
    result_ids: &[u64],
    store: &Store,
    staleness_cutoff: u64,
) -> HashMap<u64, f32>
```

Uses `MAX_BRIEFING_CO_ACCESS_BOOST = 0.01` (very small influence per scope decision).

### C5: Confidence Factor Extension (`unimatrix-server`)

**Scope**: Add seventh factor to the confidence composite formula.

**Changes to `confidence.rs`**:

New weight distribution (see ADR-003 for rationale):
```rust
pub const W_BASE: f32 = 0.20;
pub const W_USAGE: f32 = 0.15;
pub const W_FRESH: f32 = 0.18;  // reduced from 0.20
pub const W_HELP: f32 = 0.15;
pub const W_CORR: f32 = 0.12;   // reduced from 0.15
pub const W_TRUST: f32 = 0.12;  // reduced from 0.15
pub const W_COAC: f32 = 0.08;   // NEW: co-access affinity
// Sum: 0.20 + 0.15 + 0.18 + 0.15 + 0.12 + 0.12 + 0.08 = 1.00
```

**Decision on integration path** (SR-04 resolution): Co-access affinity is NOT inside `compute_confidence`. Instead, the confidence formula remains six factors. The co-access "seventh factor" is applied as a separate additive term at query time:

```rust
/// Compute the co-access affinity component for an entry.
/// This is computed at query time and added to the stored confidence value.
///
/// affinity = W_COAC * normalized_partner_score
///
/// Where normalized_partner_score is derived from the entry's co-access partners:
/// - Count of active (non-stale) partners
/// - Average confidence of partners
/// - Log-transformed to prevent gaming
pub fn co_access_affinity(
    partner_count: usize,
    avg_partner_confidence: f32,
) -> f32
```

The existing `compute_confidence` function keeps its six factors but with redistributed weights. The co-access affinity is computed separately and added at query time. This avoids breaking the function pointer signature in `record_usage_with_confidence`.

**Effective confidence at query time**:
```
effective_confidence = stored_confidence_6factor + co_access_affinity
```

Where `stored_confidence_6factor` uses the redistributed weights (base 0.20, usage 0.15, freshness 0.18, helpfulness 0.15, correction 0.12, trust 0.12 = 0.92 total) and `co_access_affinity` contributes up to W_COAC (0.08). The combined value is clamped to [0.0, 1.0].

This means:
- `compute_confidence()` returns a value in [0.0, 0.92] (six factors with redistributed weights)
- `co_access_affinity()` returns a value in [0.0, 0.08]
- At query time, the two are summed and clamped to [0.0, 1.0]
- When there is no co-access data, affinity = 0.0, so entries behave as if confidence is slightly lower (max 0.92 vs previous max 1.0). This is acceptable because the net effect is small.

### C6: Tool Handler Modifications (`unimatrix-server`)

**Scope**: Modify `context_search`, `context_briefing`, and `context_status` handlers.

**context_search changes** in `tools.rs`:

After step 9b (similarity+confidence re-ranking), add step 9c (co-access boost):

```
9c. Co-access boost:
    a. Extract anchor IDs: top min(3, result_count) entry IDs from ranked results
    b. Extract all result IDs
    c. Call coaccess::compute_search_boost(anchors, results, store, staleness_cutoff)
    d. For each result, add the boost to its rerank score
    e. Re-sort by boosted score
    f. Trim to k results (boost may not change order, but if it does, respect k)
```

The rerank integration:
```rust
// Step 9b: Existing rerank (similarity + confidence)
let score = rerank_score(similarity, entry.confidence);

// Step 9c: Co-access boost
let co_access_boost = boost_map.get(&entry.id).copied().unwrap_or(0.0);
let final_score = score + co_access_boost;
```

**context_briefing changes** in `tools.rs`:

Briefing assembles entries from lookup + search. After the existing assembly:
1. Identify anchor entries (top entries from the search portion)
2. Call `coaccess::compute_briefing_boost(anchors, all_briefing_entry_ids, store, staleness_cutoff)`
3. Apply very small boost to re-order briefing entries
4. This changes which entries appear in the final briefing when the token budget is tight

**context_status changes** in `tools.rs`:

After existing status report assembly:
1. Call `store.co_access_stats(staleness_cutoff)` for pair counts
2. Call `store.top_co_access_pairs(5, staleness_cutoff)` for top clusters
3. Call `store.cleanup_stale_co_access(staleness_cutoff)` for piggybacked maintenance
4. Add to `StatusReport`:
   - `total_co_access_pairs: u64`
   - `active_co_access_pairs: u64`
   - `top_co_access_pairs: Vec<CoAccessClusterEntry>`
   - `stale_pairs_cleaned: u64`

### C7: StatusReport Extension (`unimatrix-server`)

**Scope**: Extend `StatusReport` and response formatting.

**StatusReport changes** in `response.rs`:
```rust
pub struct StatusReport {
    // existing fields...
    pub total_co_access_pairs: u64,       // new
    pub active_co_access_pairs: u64,      // new
    pub top_co_access_pairs: Vec<CoAccessClusterEntry>,  // new
    pub stale_pairs_cleaned: u64,         // new
}

pub struct CoAccessClusterEntry {
    pub entry_id_a: u64,
    pub entry_id_b: u64,
    pub title_a: String,
    pub title_b: String,
    pub count: u32,
    pub last_updated: u64,
}
```

**Response formatting**:
- Summary: append "Co-access: X pairs (Y active)"
- Markdown: add "## Co-Access Patterns" section with top pairs
- JSON: add `co_access` object with stats and top pairs

## Data Flow

```
context_search / context_lookup / context_briefing / context_get
  |
  | (returns entry_ids to server)
  v
server.rs::record_usage_for_entries(agent_id, trust, entry_ids, helpful, feature)
  |
  |-- Step 1-4: Existing usage recording (access_count, votes, confidence, feature_entries)
  |
  |-- Step 5: Co-access recording (NEW)
  |     |-- coaccess::generate_pairs(entry_ids, MAX_CO_ACCESS_ENTRIES)
  |     |-- usage_dedup.filter_co_access_pairs(pairs)
  |     |-- spawn_blocking: store.record_co_access_pairs(new_pairs)
  |     v
  |   CO_ACCESS table updated: count++, last_updated = now
  v
(tool response already sent -- fire-and-forget)

context_search(query)
  |
  |-- Steps 1-9b: Existing flow (embed, HNSW search, filter, rerank)
  |
  |-- Step 9c: Co-access boost (NEW)
  |     |-- anchor_ids = top 3 results
  |     |-- coaccess::compute_search_boost(anchors, results, store, staleness)
  |     |     |-- store.get_co_access_partners(anchor_id, staleness) for each anchor
  |     |     |-- compute log-transformed boost for each co-accessed result
  |     |     v
  |     |   HashMap<entry_id, boost>
  |     |-- Apply boost, re-sort
  |     v
  |   Final ranked results
  v
Format and return

context_status()
  |
  |-- Existing status report assembly
  |
  |-- Co-access stats (NEW)
  |     |-- store.co_access_stats(staleness)
  |     |-- store.top_co_access_pairs(5, staleness)
  |     |-- store.cleanup_stale_co_access(staleness)  // piggybacked maintenance
  |     v
  |   StatusReport with co-access fields
  v
Format and return
```

## Component Dependencies

```
C1 (CO_ACCESS Table + Storage)
 |
 +-- C2 (Session Dedup Extension) -- independent of C1
 |
 +-- C3 (Co-Access Recording) -- depends on C1, C2
 |
 +-- C4 (Co-Access Boost Module) -- depends on C1
 |
 +-- C5 (Confidence Factor Extension) -- depends on C4
 |
 +-- C6 (Tool Handler Modifications) -- depends on C3, C4, C5
 |
 +-- C7 (StatusReport Extension) -- depends on C1
```

**Implementation order**: C1 + C2 (parallel) -> C3 + C4 (parallel) -> C5 -> C6 + C7 (parallel)

## Integration Points

### With crt-002 (Confidence)
- Weight redistribution: six existing weights reduced proportionally to make room for W_COAC = 0.08
- `compute_confidence()` continues to be called via function pointer in `record_usage_with_confidence` -- returns [0.0, 0.92] range now
- Co-access affinity is added separately at query time, not inside `compute_confidence()`
- `rerank_score()` unchanged -- still takes (similarity, confidence). The "confidence" input will now include co-access affinity at query time

### With crt-001 (Usage Tracking)
- Co-access recording plugs into `record_usage_for_entries` after existing steps
- `UsageDedup` extended with co-access pair tracking
- Fire-and-forget pattern reused

### With crt-003 (Contradiction Detection)
- Quarantined entries excluded from co-access partner lookups (SR-08)
- Deprecated entries also excluded from co-access lookups
- Status check on partner entries during boost computation

### With vnc-003 (context_status)
- StatusReport gains co-access fields
- Stale pair cleanup piggybacked on status calls
- Response formatting extended with co-access section

### With nxs-001 (Store)
- New CO_ACCESS table follows existing table patterns
- Store::open() extended for 13th table
- bincode serde path for CoAccessRecord serialization

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `CO_ACCESS` | `TableDefinition<(u64, u64), &[u8]>` | `unimatrix-store/schema.rs` |
| `CoAccessRecord` | `{ count: u32, last_updated: u64 }` | `unimatrix-store/schema.rs` |
| `co_access_key(a, b)` | `fn(u64, u64) -> (u64, u64)` | `unimatrix-store/schema.rs` |
| `serialize_co_access` | `fn(&CoAccessRecord) -> Result<Vec<u8>>` | `unimatrix-store/schema.rs` |
| `deserialize_co_access` | `fn(&[u8]) -> Result<CoAccessRecord>` | `unimatrix-store/schema.rs` |
| `Store::record_co_access` | `fn(&self, &[u64], usize) -> Result<()>` | `unimatrix-store/write.rs` |
| `Store::record_co_access_pairs` | `fn(&self, &[(u64, u64)]) -> Result<()>` | `unimatrix-store/write.rs` |
| `Store::cleanup_stale_co_access` | `fn(&self, u64) -> Result<u64>` | `unimatrix-store/write.rs` |
| `Store::get_co_access_partners` | `fn(&self, u64, u64) -> Result<Vec<(u64, CoAccessRecord)>>` | `unimatrix-store/read.rs` |
| `Store::co_access_stats` | `fn(&self, u64) -> Result<(u64, u64)>` | `unimatrix-store/read.rs` |
| `Store::top_co_access_pairs` | `fn(&self, usize, u64) -> Result<Vec<((u64, u64), CoAccessRecord)>>` | `unimatrix-store/read.rs` |
| `UsageDedup::filter_co_access_pairs` | `fn(&self, &[(u64, u64)]) -> Vec<(u64, u64)>` | `unimatrix-server/usage_dedup.rs` |
| `coaccess::generate_pairs` | `fn(&[u64], usize) -> Vec<(u64, u64)>` | `unimatrix-server/coaccess.rs` |
| `coaccess::compute_search_boost` | `fn(&[u64], &[u64], &Store, u64) -> HashMap<u64, f32>` | `unimatrix-server/coaccess.rs` |
| `coaccess::compute_briefing_boost` | `fn(&[u64], &[u64], &Store, u64) -> HashMap<u64, f32>` | `unimatrix-server/coaccess.rs` |
| `confidence::co_access_affinity` | `fn(usize, f32) -> f32` | `unimatrix-server/confidence.rs` |
| `MAX_CO_ACCESS_ENTRIES` | `const usize = 10` | `unimatrix-server/coaccess.rs` |
| `CO_ACCESS_STALENESS_SECONDS` | `const u64 = 2_592_000` | `unimatrix-server/coaccess.rs` |
| `MAX_CO_ACCESS_BOOST` | `const f32 = 0.03` | `unimatrix-server/coaccess.rs` |
| `MAX_BRIEFING_CO_ACCESS_BOOST` | `const f32 = 0.01` | `unimatrix-server/coaccess.rs` |
| `MAX_MEANINGFUL_CO_ACCESS` | `const f64 = 20.0` | `unimatrix-server/coaccess.rs` |

## Technology Decisions

See ADR documents for detailed rationale:
- ADR-001: CO_ACCESS table key design and partner lookup strategy
- ADR-002: Co-access boost formula (log-transform + cap)
- ADR-003: Confidence weight redistribution strategy
