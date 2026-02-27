# Specification: crt-004 Co-Access Boosting

## Objective

Track which knowledge entries are retrieved together in the same tool call, accumulate pairwise co-retrieval frequency in a dedicated CO_ACCESS table, and use that signal to boost semantically related entries in search and briefing results. The feature adds a seventh confidence factor (co-access affinity) and a post-ranking search boost that promotes entries frequently co-retrieved with top results.

## Functional Requirements

### FR-01: CO_ACCESS Table
- FR-01a: A new redb table `CO_ACCESS` stores co-access pairs with `(u64, u64)` ordered keys and bincode-serialized `CoAccessRecord` values.
- FR-01b: `CoAccessRecord` contains `count: u32` and `last_updated: u64`.
- FR-01c: Keys are ordered as `(min(a, b), max(a, b))` to guarantee symmetric deduplication.
- FR-01d: The table is created during `Store::open()` alongside existing tables.
- FR-01e: Serialization uses the bincode serde path (`bincode::serde::encode_to_vec` / `decode_from_slice` with `standard()` config).

### FR-02: Co-Access Recording
- FR-02a: When `record_usage_for_entries` processes a result set of 2+ entries, it generates co-access pairs and records them.
- FR-02b: Pair generation is capped at `MAX_CO_ACCESS_ENTRIES` (default: 10). Only the first 10 entries in the result set contribute to pairs.
- FR-02c: For a capped result set of k entries, k*(k-1)/2 ordered pairs are generated.
- FR-02d: For each pair: if no record exists, create with `count=1, last_updated=now`. If a record exists, increment `count` and update `last_updated`.
- FR-02e: All pairs from a single result set are written in a single redb write transaction.
- FR-02f: Recording is fire-and-forget (does not block tool response). Uses `spawn_blocking`.

### FR-03: Session Deduplication
- FR-03a: `UsageDedup` tracks co-access pairs recorded this session.
- FR-03b: Dedup is agent-independent: the dedup key is the ordered pair `(min_id, max_id)`, not `(agent_id, pair)`.
- FR-03c: If a pair was already recorded this session (by any agent), it is skipped.
- FR-03d: Dedup state is in-memory, cleared on server restart.

### FR-04: Search Boost
- FR-04a: After existing similarity+confidence re-ranking (step 9b), `context_search` applies a co-access boost step.
- FR-04b: The top 3 results (or fewer if result count < 3) serve as anchor entries.
- FR-04c: For each anchor, look up its co-access partners from CO_ACCESS (filtering by staleness).
- FR-04d: For each result entry that is a co-access partner of any anchor, compute an additive boost.
- FR-04e: Boost formula: `min(ln(1 + count) / ln(1 + MAX_MEANINGFUL_CO_ACCESS), 1.0) * MAX_CO_ACCESS_BOOST`.
- FR-04f: `MAX_CO_ACCESS_BOOST = 0.03`, `MAX_MEANINGFUL_CO_ACCESS = 20.0`.
- FR-04g: If an entry is a co-access partner of multiple anchors, use the maximum boost (not sum).
- FR-04h: Results are re-sorted by boosted score after co-access boost.
- FR-04i: Co-access partners that are quarantined or deprecated are excluded from boost calculations.

### FR-05: Briefing Boost
- FR-05a: `context_briefing` applies co-access boosting to its entry assembly.
- FR-05b: Uses the same algorithm as search boost but with `MAX_BRIEFING_CO_ACCESS_BOOST = 0.01`.
- FR-05c: Anchor entries are the top entries from the search portion of briefing assembly.

### FR-06: Confidence Factor
- FR-06a: Confidence weight redistribution: six existing factors reduced proportionally to accommodate W_COAC = 0.08. New weights: base 0.18, usage 0.14, freshness 0.18, helpfulness 0.14, correction 0.14, trust 0.14. Sum = 0.92.
- FR-06b: `compute_confidence()` returns values in [0.0, 0.92] with the redistributed weights.
- FR-06c: Co-access affinity is computed at query time: `W_COAC * normalized_partner_score`.
- FR-06d: `normalized_partner_score = min(ln(1 + partner_count) / ln(1 + MAX_MEANINGFUL_PARTNERS), 1.0) * avg_partner_confidence`.
- FR-06e: `MAX_MEANINGFUL_PARTNERS = 10`.
- FR-06f: Effective confidence at query time: `stored_confidence + co_access_affinity`, clamped to [0.0, 1.0].
- FR-06g: `rerank_score()` uses effective confidence (with co-access affinity included).

### FR-07: Staleness
- FR-07a: Co-access pairs with `last_updated` older than `CO_ACCESS_STALENESS_SECONDS` (default: 30 days = 2,592,000 seconds) are excluded from boost calculations.
- FR-07b: Staleness filtering is applied during read operations (`get_co_access_partners`, `co_access_stats`).
- FR-07c: Stale pair cleanup runs during `context_status` execution (piggybacked maintenance).
- FR-07d: `cleanup_stale_co_access()` removes stale pairs from the table and returns the count removed.

### FR-08: Status Reporting
- FR-08a: `StatusReport` extended with `total_co_access_pairs`, `active_co_access_pairs`, `top_co_access_pairs`, and `stale_pairs_cleaned`.
- FR-08b: `context_status` response includes a co-access section in all formats (summary, markdown, json).
- FR-08c: Top co-access pairs include entry IDs, titles, count, and last_updated timestamp.
- FR-08d: Top-5 co-access pairs are reported (configurable).

## Non-Functional Requirements

- NFR-01: Co-access recording must not add measurable latency to tool responses. Fire-and-forget with `spawn_blocking`.
- NFR-02: Search boost overhead must be < 20ms per search call at 10K co-access pairs (per ADR-001 analysis).
- NFR-03: Storage overhead for CO_ACCESS must be < 1MB at 10K pairs (~280KB expected).
- NFR-04: No new crate dependencies. Uses existing redb, bincode, tokio.
- NFR-05: `#![forbid(unsafe_code)]`, edition 2024.
- NFR-06: All computation uses f64 intermediates for numerical stability, cast to f32 for storage/output (consistent with crt-002 ADR-002).

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | CO_ACCESS redb table exists with `(u64, u64) -> &[u8]` schema, ordered keys | Unit test: table opens, key ordering verified |
| AC-02 | CoAccessRecord roundtrips through bincode serde path | Unit test: serialize/deserialize roundtrip |
| AC-03 | Co-access pairs recorded when result set contains 2+ entries | Integration test: search returns 3 entries, verify 3 pairs in CO_ACCESS |
| AC-04 | Pair generation capped at MAX_CO_ACCESS_ENTRIES (10) | Unit test: 15 IDs -> only first 10 used = 45 pairs |
| AC-05 | Ordered keys: (min, max) deduplication | Unit test: co_access_key(5,3) == co_access_key(3,5) == (3,5) |
| AC-06 | Count incremented atomically on re-encounter | Integration test: record same pair twice, verify count=2 |
| AC-07 | last_updated set on every increment | Integration test: record pair, sleep, re-record, verify last_updated changed |
| AC-08 | Session dedup prevents duplicate co-access recording | Unit test: filter_co_access_pairs returns pair first time, empty second time |
| AC-09 | context_search applies co-access boost after reranking | Integration test: seed CO_ACCESS, search, verify boosted entry moved up |
| AC-10 | Boost is additive and capped at MAX_CO_ACCESS_BOOST | Unit test: boost formula at count=100 returns MAX_CO_ACCESS_BOOST |
| AC-11 | Top 3 results used as anchors | Unit test: compute_search_boost with known anchors |
| AC-12 | Stale pairs excluded from boost | Integration test: seed old pair, verify not used in boost |
| AC-13 | context_status reports co-access stats | Integration test: seed pairs, call status, verify stats in response |
| AC-14 | Confidence weights redistributed, sum check | Unit test: W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92 |
| AC-15 | Effective confidence = stored + co_access_affinity, sum <= 1.0 | Unit test: boundary values |
| AC-16 | Co-access affinity computed from partner data | Unit test: co_access_affinity with known inputs |
| AC-17 | context_briefing applies direct co-access boost with very small weight | Integration test: seed CO_ACCESS, briefing, verify boost applied |
| AC-18 | Stale pairs cleaned up during context_status | Integration test: seed stale pairs, call status, verify removal |
| AC-19 | All new code has unit tests; integration tests for recording, dedup, boost, staleness | Test count assertion |
| AC-20 | Existing tests pass (no regressions from weight redistribution) | `cargo test` passes |
| AC-21 | Co-access recording is fire-and-forget | Code review: spawn_blocking, no .await on result before returning |
| AC-22 | `#![forbid(unsafe_code)]`, no new dependencies | Cargo.toml diff, compiler check |

## Domain Models

### CoAccessRecord
A pairwise relationship between two entries tracked in the CO_ACCESS table:
- **Key**: `(min_entry_id, max_entry_id)` -- ordered pair, stored once
- **count**: Number of times these two entries appeared in the same tool result set
- **last_updated**: Unix timestamp of most recent co-retrieval

### Co-Access Partner
For a given entry X, a co-access partner is any entry Y such that `(min(X,Y), max(X,Y))` exists in CO_ACCESS with a non-stale `last_updated`. The partnership is symmetric: if Y is a partner of X, X is a partner of Y.

### Anchor Entry
In the co-access boost algorithm, anchor entries are the top-ranked results from the existing similarity+confidence re-ranking. They serve as the reference points for co-access lookups. Default: top 3 results.

### Co-Access Boost
An additive score in [0.0, MAX_CO_ACCESS_BOOST] applied to search result rankings. Computed from co-access count using a log-transform formula with a hard cap.

### Effective Confidence
The sum of stored confidence (six-factor composite) and query-time co-access affinity. Clamped to [0.0, 1.0]. Used in `rerank_score()` for search ranking.

### Staleness
A co-access pair is stale when `current_time - last_updated > CO_ACCESS_STALENESS_SECONDS`. Stale pairs are excluded from boost calculations and cleaned up during `context_status` calls.

## User Workflows

### Agent Search with Co-Access Boost
1. Agent calls `context_search` with a natural language query
2. Server embeds query, searches HNSW, fetches entries, filters quarantined
3. Results re-ranked by similarity + confidence (existing crt-002 behavior)
4. Top 3 results selected as co-access anchors
5. CO_ACCESS table queried for each anchor's partners
6. Results that are co-access partners receive additive boost
7. Results re-sorted by boosted score, returned to agent
8. Co-access pairs from the result set recorded (fire-and-forget)

### Operator Status Check
1. Operator calls `context_status`
2. Report includes "Co-Access: N pairs (M active, K cleaned)"
3. Top-5 co-access pairs listed with entry titles and counts
4. Stale pairs cleaned up as a side effect

## Constraints

- Confidence weights must sum to 0.92 (six factors) + 0.08 (co-access at query time) = 1.0 effective
- `compute_confidence` function signature unchanged: `(&EntryRecord, u64) -> f32`
- CO_ACCESS table opened in Store::open() following existing table initialization pattern
- Fire-and-forget pattern: co-access recording must not block tool response
- Object-safe traits: any trait extensions maintain object safety
- Test infrastructure is cumulative: build on existing fixtures

## Dependencies

- **unimatrix-store**: redb, bincode (existing)
- **unimatrix-server**: tokio, rmcp, unimatrix-store, unimatrix-core (existing)
- **No new external dependencies**

## NOT In Scope

- Graph algorithms (PageRank, authority scores, transitive relationships)
- Per-agent co-access profiles
- Cross-session co-access computation
- New MCP tools for co-access queries
- UI for co-access visualization
- Background co-access computation or batch jobs
- Co-access influence on deterministic retrieval (context_lookup, context_get)
- Schema migration or new fields on EntryRecord
