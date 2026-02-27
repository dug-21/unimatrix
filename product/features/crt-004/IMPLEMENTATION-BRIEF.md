# Implementation Brief: crt-004 Co-Access Boosting

## Feature Summary

crt-004 adds co-access intelligence to Unimatrix -- tracking which entries are retrieved together and using that signal to improve search and briefing results. It introduces 1 new redb table (CO_ACCESS), 1 new server module (coaccess.rs), extends the confidence formula with a seventh factor, and modifies the search and briefing tool handlers with co-access boosting.

## Source Documents

| Document | Path |
|----------|------|
| SCOPE.md | product/features/crt-004/SCOPE.md |
| Scope Risk Assessment | product/features/crt-004/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-004/architecture/ARCHITECTURE.md |
| ADR-001 | product/features/crt-004/architecture/ADR-001-co-access-table-design.md |
| ADR-002 | product/features/crt-004/architecture/ADR-002-co-access-boost-formula.md |
| ADR-003 | product/features/crt-004/architecture/ADR-003-confidence-weight-redistribution.md |
| Specification | product/features/crt-004/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-004/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-004/ALIGNMENT-REPORT.md |

## Component Map

| Component | Description | Crate | Pseudocode | Test Plan |
|-----------|-------------|-------|-----------|-----------|
| C1: co-access-storage | CO_ACCESS table, CoAccessRecord, read/write methods | unimatrix-store | pseudocode/co-access-storage.md | test-plan/co-access-storage.md |
| C2: session-dedup | Extend UsageDedup with co-access pair tracking | unimatrix-server | pseudocode/session-dedup.md | test-plan/session-dedup.md |
| C3: co-access-recording | Record co-access pairs in usage pipeline | unimatrix-server | pseudocode/co-access-recording.md | test-plan/co-access-recording.md |
| C4: co-access-boost | Boost computation for search and briefing | unimatrix-server | pseudocode/co-access-boost.md | test-plan/co-access-boost.md |
| C5: confidence-extension | Seventh confidence factor, weight redistribution | unimatrix-server | pseudocode/confidence-extension.md | test-plan/confidence-extension.md |
| C6: tool-integration | Modify context_search, context_briefing, context_status | unimatrix-server | pseudocode/tool-integration.md | test-plan/tool-integration.md |

## Implementation Order

```
C1 (co-access-storage) + C2 (session-dedup) -- parallel, no dependencies
 |
 +---> C3 (co-access-recording) + C4 (co-access-boost) -- parallel, depend on C1/C2
 |
 +---> C5 (confidence-extension) -- depends on C4
 |
 +---> C6 (tool-integration) -- depends on C3, C4, C5
```

**Wave 1**: C1 + C2 in parallel (storage + dedup -- foundation)
**Wave 2**: C3 + C4 in parallel (recording + boost computation)
**Wave 3**: C5 (confidence weight redistribution)
**Wave 4**: C6 (tool handler modifications + status reporting)

## Key Design Decisions

1. **Full table scan for partner lookup** (ADR-001): CO_ACCESS uses ordered `(min, max)` keys. Finding all partners of entry X requires a prefix scan + full table scan. At Unimatrix scale (1K-10K pairs), this is < 5ms per lookup. Interface hides implementation for future optimization.

2. **Log-transform + cap for boost formula** (ADR-002): `boost = min(ln(1 + count) / ln(1 + 20), 1.0) * 0.03`. Consistent with crt-002's usage_score pattern. Max boost 0.03 is roughly equivalent to a 3.5% similarity gap.

3. **Split confidence integration** (ADR-003): Six existing factors reduced proportionally to sum to 0.92. Co-access affinity (max 0.08) added at query time. `compute_confidence()` signature unchanged. Function pointer in `record_usage_with_confidence` continues to work.

4. **Agent-independent co-access dedup**: Co-access pairs are global (not per-agent). Dedup prevents any agent from inflating counts for the same pair within a session, but different agents reinforce the same pair.

5. **Briefing boost with very small weight**: MAX_BRIEFING_CO_ACCESS_BOOST = 0.01 (vs 0.03 for search). Deliberately small to prevent co-access from changing which entries appear in agent orientation.

6. **Staleness configurable, default 30 days**: CO_ACCESS_STALENESS_SECONDS = 2,592,000. Named constant, same pattern as FRESHNESS_HALF_LIFE_HOURS in crt-002.

7. **Piggybacked staleness cleanup**: Stale pair removal runs during context_status (on-demand maintenance, consistent with crt-003's on-demand contradiction scanning).

## Cross-Crate Impact

| Crate | Changes |
|-------|---------|
| unimatrix-store | New table CO_ACCESS, new types CoAccessRecord, new read/write methods, Store::open extended |
| unimatrix-core | No changes (no new traits, no schema changes) |
| unimatrix-vector | No changes |
| unimatrix-embed | No changes |
| unimatrix-server | New module coaccess.rs, modified confidence.rs, usage_dedup.rs, tools.rs, server.rs, response.rs |

## Files to Create/Modify

### New Files
| Path | Description |
|------|-------------|
| `crates/unimatrix-server/src/coaccess.rs` | Co-access pair generation, boost computation, constants |

### Modified Files
| Path | Description |
|------|-------------|
| `crates/unimatrix-store/src/schema.rs` | CO_ACCESS table def, CoAccessRecord, serialization helpers |
| `crates/unimatrix-store/src/db.rs` | Open CO_ACCESS table in Store::open() |
| `crates/unimatrix-store/src/write.rs` | record_co_access, record_co_access_pairs, cleanup_stale_co_access |
| `crates/unimatrix-store/src/read.rs` | get_co_access_partners, co_access_stats, top_co_access_pairs |
| `crates/unimatrix-store/src/lib.rs` | Export CO_ACCESS, CoAccessRecord, helpers |
| `crates/unimatrix-server/src/lib.rs` | Add coaccess module |
| `crates/unimatrix-server/src/confidence.rs` | Weight redistribution (6 weights reduced), co_access_affinity fn |
| `crates/unimatrix-server/src/usage_dedup.rs` | Add co_access_recorded HashSet, filter_co_access_pairs method |
| `crates/unimatrix-server/src/server.rs` | Extend record_usage_for_entries with co-access recording step |
| `crates/unimatrix-server/src/tools.rs` | context_search: co-access boost step; context_briefing: co-access boost; context_status: co-access stats |
| `crates/unimatrix-server/src/response.rs` | StatusReport co-access fields, CoAccessClusterEntry, format_status_report extension |

## Data Structures

```rust
// unimatrix-store/schema.rs
pub const CO_ACCESS: TableDefinition<(u64, u64), &[u8]> = TableDefinition::new("co_access");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoAccessRecord {
    pub count: u32,
    pub last_updated: u64,
}

// unimatrix-server/response.rs
pub struct CoAccessClusterEntry {
    pub entry_id_a: u64,
    pub entry_id_b: u64,
    pub title_a: String,
    pub title_b: String,
    pub count: u32,
    pub last_updated: u64,
}
```

## Function Signatures

```rust
// unimatrix-store/schema.rs
pub fn co_access_key(a: u64, b: u64) -> (u64, u64)
pub fn serialize_co_access(record: &CoAccessRecord) -> Result<Vec<u8>>
pub fn deserialize_co_access(bytes: &[u8]) -> Result<CoAccessRecord>

// unimatrix-store/write.rs
impl Store {
    pub fn record_co_access(&self, entry_ids: &[u64], max_pairs_from: usize) -> Result<()>
    pub fn record_co_access_pairs(&self, pairs: &[(u64, u64)]) -> Result<()>
    pub fn cleanup_stale_co_access(&self, cutoff_timestamp: u64) -> Result<u64>
}

// unimatrix-store/read.rs
impl Store {
    pub fn get_co_access_partners(&self, entry_id: u64, staleness_cutoff: u64) -> Result<Vec<(u64, CoAccessRecord)>>
    pub fn co_access_stats(&self, staleness_cutoff: u64) -> Result<(u64, u64)>
    pub fn top_co_access_pairs(&self, n: usize, staleness_cutoff: u64) -> Result<Vec<((u64, u64), CoAccessRecord)>>
}

// unimatrix-server/coaccess.rs
pub fn generate_pairs(entry_ids: &[u64], max_entries: usize) -> Vec<(u64, u64)>
pub fn compute_search_boost(anchor_ids: &[u64], result_ids: &[u64], store: &Store, staleness_cutoff: u64) -> HashMap<u64, f32>
pub fn compute_briefing_boost(anchor_ids: &[u64], result_ids: &[u64], store: &Store, staleness_cutoff: u64) -> HashMap<u64, f32>

// unimatrix-server/confidence.rs
pub fn co_access_affinity(partner_count: usize, avg_partner_confidence: f32) -> f32

// unimatrix-server/usage_dedup.rs
impl UsageDedup {
    pub fn filter_co_access_pairs(&self, pairs: &[(u64, u64)]) -> Vec<(u64, u64)>
}
```

## Constants

```rust
// unimatrix-server/coaccess.rs
pub const MAX_CO_ACCESS_ENTRIES: usize = 10;
pub const CO_ACCESS_STALENESS_SECONDS: u64 = 2_592_000; // 30 days
pub const MAX_CO_ACCESS_BOOST: f32 = 0.03;
pub const MAX_BRIEFING_CO_ACCESS_BOOST: f32 = 0.01;
pub const MAX_MEANINGFUL_CO_ACCESS: f64 = 20.0;
pub const MAX_MEANINGFUL_PARTNERS: f64 = 10.0;

// unimatrix-server/confidence.rs (updated)
pub const W_BASE: f32 = 0.18;  // was 0.20
pub const W_USAGE: f32 = 0.14; // was 0.15
pub const W_FRESH: f32 = 0.18; // was 0.20
pub const W_HELP: f32 = 0.14;  // was 0.15
pub const W_CORR: f32 = 0.14;  // was 0.15
pub const W_TRUST: f32 = 0.14; // was 0.15
pub const W_COAC: f32 = 0.08;  // NEW (applied at query time, not in compute_confidence)
```

## Constraints

- `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89
- No new crate dependencies
- Object-safe traits maintained
- Fire-and-forget: co-access recording must not block tool responses
- Confidence weights (stored six): sum to 0.92 exactly
- Effective confidence (stored + co-access affinity): clamped to [0.0, 1.0]
- CO_ACCESS table opened in Store::open() alongside existing 12 tables
- Test infrastructure cumulative: build on crt-001/002/003 fixtures
- Quarantined and deprecated entries excluded from co-access partner lookups

## Dependencies

- redb (existing workspace dep)
- bincode (existing workspace dep)
- tokio (existing workspace dep)
- unimatrix-store (existing crate)
- unimatrix-core (existing crate, for EntryRecord/Status re-exports)

## NOT In Scope

- Graph algorithms, PageRank, transitive relationships
- Per-agent co-access profiles
- Cross-session co-access computation
- New MCP tools
- UI for co-access visualization
- Background/scheduled co-access computation
- Schema migration (no EntryRecord changes)
- Co-access influence on context_lookup or context_get

## Risk Hotspots (Test First)

1. **R-01: Confidence weight regression** (C5) -- most impactful risk. Run all crt-002 confidence tests with new weight values first.
2. **R-02: Feedback loop** (C4) -- verify boost cap and log-transform prevent runaway amplification.
3. **R-04: Quadratic pair generation** (C3) -- verify MAX_CO_ACCESS_ENTRIES cap enforced.
4. **R-08: Quarantined partner boost** (C4) -- verify quarantined/deprecated entries excluded from partner lookups.
5. **R-09: CoAccessRecord serialization** (C1) -- verify roundtrip before any other store tests.

## Alignment Status

All checks PASS. One WARN: the split confidence integration (ADR-003) is an architectural addition beyond SCOPE's literal "seventh factor in the formula" framing, but the effective behavior is equivalent and the addition is justified by preserving the function pointer signature (SR-04). No variances requiring human approval.
