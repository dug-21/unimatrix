# FINDINGS: Core Scalability Strategy for Unimatrix

**Spike**: ass-047
**Date**: 2026-04-11
**Approach**: investigation + analytical modeling
**Confidence**: directional

---

## Findings

### Q1: Write path analysis — queue depth and latency at N=5, 10, 20 concurrent agents writing to the same repo

**Answer**: The write path bifurcates into two distinct lanes: (1) integrity writes (entries, audit_log, tags, vectors) go directly through the `write_pool` and block on acquisition; (2) analytics writes (co_access, sessions, observations, query_log, injection_log, etc.) go through a bounded async channel to a single drain task.

**Actual pool configuration** (from `crates/unimatrix-store/src/pool_config.rs`):
- `write_max_connections`: default **1** (hard cap is 2; the default is 1 to prevent `SQLITE_BUSY_SNAPSHOT` under concurrent WAL deferred transactions)
- `write_acquire_timeout`: 5 seconds
- `ANALYTICS_QUEUE_CAPACITY`: 1000 events
- `DRAIN_BATCH_SIZE`: 50 events per transaction
- `DRAIN_FLUSH_INTERVAL`: 500ms

**Evidence**: `crates/unimatrix-store/src/pool_config.rs` lines 31–37 (drain constants), 68–80 (default PoolConfig). `crates/unimatrix-store/src/analytics.rs` implements the drain task logic at lines 257–312.

**Modeling**:

*Integrity write path (context_store, context_correct, audit_log):*

Each integrity operation (context_store) involves one write_pool transaction with 2–4 SQL statements (insert entry, insert tags, increment counter). SQLite WAL transaction commit time: ~1–5ms on NVMe, ~5–20ms on HDD or shared volume.

At write_max=1, all integrity writes are serialized. At N concurrent agents each attempting 1 write/sec:
- N=5: 5 write requests/sec → average queue depth near 0 (well within 5s timeout)
- N=10: 10 write requests/sec → average queue depth 1–3, latency ~10–50ms each
- N=20: 20 write requests/sec → queue depth 3–8; p99 latency approaches the 5s write_acquire_timeout on slow storage

At average 5ms/write, throughput ceiling is ~200 writes/sec — adequate for 20 agents at normal usage (1–2 writes/sec each). At slow storage (20ms/write), the ceiling drops to ~50 writes/sec, reached at 10–12 agents doing 5 writes/sec each.

*Analytics write path (co_access, observations, session updates):*

Agents enqueue asynchronously into the 1000-event bounded channel and return immediately. The drain task batches ≤50 events per 500ms window into a single transaction. Drain rate: 50 events/500ms = 100 events/sec maximum.

- N=5: 25 analytics events/sec → queue stays near zero (100 events/sec drain capacity)
- N=10: 50 events/sec → exactly half of drain capacity; queue stable
- N=20: 100 events/sec → at drain ceiling; any write slowdown causes queue growth
- N=30+: queue begins filling toward the 1000-event capacity; shed events follow

*Critical distinction — audit log is NOT on the analytics path:*

`log_audit_event` in `crates/unimatrix-store/src/audit.rs` uses `write_pool` directly (synchronous integrity path). Each write tool call waits for the audit write before returning. At write_max=1, audit log writes compete with entry writes for the single connection. The 500ms batching window applies only to co_access, sessions, observations — not the audit log.

**Assessment of working hypothesis "async write queue is the write bottleneck"**: Partially correct but incomplete. The analytics channel is not the bottleneck at N≤20 (drain capacity is 100 events/sec). The real bottleneck is the single write_pool connection shared by integrity writes and audit log writes. This serialization is intentional (SQLite WAL correctness) but sets the write ceiling.

**Recommendation**: The write path sustains approximately 20 concurrent agents writing to the same repo at normal usage (1–3 integrity writes/sec, 5–10 analytics events/sec per agent). Above 20 agents with sustained write pressure, the single write connection becomes the bottleneck. The 500ms batching window is acceptable for analytics (co_access, observations) but irrelevant to the audit log, which is synchronous by design.

---

### Q2: Control plane DB concurrency — reads/writes at 10 agents; SQLite WAL bottleneck threshold; read pool size

**Answer**: The current architecture is **strictly single-repo, single-process**. There is no control plane DB in the codebase. Every server process owns exactly one SqlxStore. The multi-repo enterprise control plane is a Wave 2 addition that does not yet exist.

**Evidence**: `crates/unimatrix-server/src/main.rs` wires a single `SqlxStore::open()` call to a single project path. Grepping for `multi_repo`, `per_repo`, `org_id`, `tenant`, `multi_project` returns zero matches in the server codebase.

**Predictive modeling for Wave 2 control plane** (per ASS-048 findings: OAuth token validation + RBAC lookup on every MCP request):

At N=10 concurrent agents × ~5 MCP requests/sec each = 50 req/sec through control plane:
- Reads: ~100/sec (2 reads per request: token validation + role binding lookup)
- Writes: ~25/sec (audit log on write operations, approximately 50% of requests)

SQLite WAL on commodity NVMe:
- Concurrent reads scale with the read pool size
- Write ceiling: ~500–2,000 write transactions/sec on NVMe; ~100–500/sec on HDD or shared NFS
- 25 writes/sec at N=10 is well within SQLite WAL's capability on any reasonable hardware

Read pool size for control plane: 4–6 connections is appropriate. At 100 reads/sec with ~10ms average read, 6 connections × (1/0.010) = 600 reads/sec throughput — comfortable headroom.

**SQLite WAL write bottleneck for control plane**: The bottleneck appears at ~100+ concurrent agents generating 5 audit writes/sec each = 500 writes/sec — near the SQLite WAL ceiling on commodity hardware. For a Wave 2 deployment serving 10–50 agents, write rates stay well under 200/sec; SQLite WAL is not the bottleneck.

**Recommendation**: SQLite WAL with `read_max=6` is adequate for a Wave 2 control plane serving up to 50 concurrent agents. PostgreSQL upgrade trigger: above 50 concurrent agents, or when audit write rates sustained exceed 300 transactions/sec. For the data plane (per-repo SQLite), per-repo isolation means each repo's SQLite is under pressure only from agents actively writing to that repo — SQLite per-repo remains appropriate at 20–50 concurrent sessions per repo at normal write patterns.

---

### Q3: PostgreSQL migration readiness — SQLite-specific SQL constructs; minimum changes for control plane config-swap; threshold for PostgreSQL recommendation

**Answer**: Multiple SQLite-specific SQL constructs exist throughout the codebase. The codebase is **not** PostgreSQL-ready for a config-swap today. Migration is feasible (not a rewrite) but requires explicit effort. The sqlx abstraction layer (parameterized queries, no raw SQL concatenation) limits the scope.

**Evidence** — identified in `crates/unimatrix-store/src/`:

*AUTOINCREMENT (SQLite syntax):*
- `db.rs`: `cycle_events`, `observations`, `shadow_evaluations`, `query_log`, `injection_log`, `audit_log` all use `INTEGER PRIMARY KEY AUTOINCREMENT`
- PostgreSQL equivalent: `BIGSERIAL PRIMARY KEY` or `GENERATED ALWAYS AS IDENTITY`

*INSERT OR IGNORE / INSERT OR REPLACE (SQLite-specific):*
- `sessions.rs`: `INSERT OR REPLACE INTO sessions`
- `counters.rs`: `INSERT OR REPLACE INTO counters`
- `db.rs`: `INSERT OR IGNORE INTO counters` (multiple)
- `registry.rs`: `INSERT OR IGNORE INTO agent_registry` (multiple)
- `analytics.rs`: `INSERT OR IGNORE INTO feature_entries`, `INSERT OR IGNORE INTO outcome_index`, `INSERT OR IGNORE INTO graph_edges`
- PostgreSQL equivalents: `INSERT ... ON CONFLICT DO NOTHING` / `INSERT ... ON CONFLICT DO UPDATE`

*ON CONFLICT DO UPDATE upsert syntax (already compatible):*
- `analytics.rs` lines 411, 438, 600, 712, 757: `ON CONFLICT (...) DO UPDATE SET ...`
- This syntax is valid PostgreSQL 9.5+. No change needed.

*json_extract (SQLite-only function):*
- `query_log.rs`: `CAST(json_extract(o.input, '$.id') AS INTEGER)` used in join conditions (lines 248, 251, 256, 303)
- PostgreSQL equivalent: `(o.input::jsonb->>'id')::INTEGER` (requires column type change to JSONB)
- This is the most substantive migration — changes both schema and query text

*PRAGMA statements (SQLite-only):*
- `pool_config.rs`: 6 PRAGMAs applied at connection-open: `journal_mode`, `synchronous`, `wal_autocheckpoint`, `foreign_keys`, `busy_timeout`, `cache_size`
- `db.rs`: `PRAGMA wal_checkpoint(TRUNCATE)` in compaction code; PRAGMA reads in tests
- All PRAGMA calls must be removed or guarded behind a SQLite-only code path

*sqlite_master (SQLite system table):*
- `migration.rs`: `SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='entries'`
- PostgreSQL: `SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'entries'`

**Minimum changes for PostgreSQL control plane config-swap** (control plane tables: OAuth tokens, role bindings, audit log — not the full data plane):

1. Remove PRAGMA calls via a database-type configuration flag at connection setup
2. Change AUTOINCREMENT to GENERATED ALWAYS AS IDENTITY on control plane tables
3. Change INSERT OR IGNORE to INSERT ... ON CONFLICT DO NOTHING in control plane paths
4. Change INSERT OR REPLACE to INSERT ... ON CONFLICT DO UPDATE in control plane upsert paths
5. Replace sqlite_master check with information_schema equivalent in migration detection

`json_extract` is only in `query_log.rs` (analytics, not control plane). Control plane tables do not use JSON extraction — this change can be deferred to a full data-plane migration.

Estimated effort: **~1 engineer-week** for control-plane-only PostgreSQL config-swap; **2–3 engineer-weeks** for full data-plane migration including json_extract and analytics tables.

**PostgreSQL threshold recommendation**: Use SQLite for Wave 2 control plane up to 50 concurrent agents. Recommend PostgreSQL for control plane when: (a) concurrent agent count exceeds 50, or (b) sustained audit write rate exceeds 300 transactions/sec, or (c) the operator's infrastructure already runs PostgreSQL and they prefer a single database technology. Build Wave 2 control plane with PostgreSQL as a documented deployment option from the start, so operators can choose at deploy time.

---

### Q4: Read path analysis — pool saturation; Arc\<RwLock\<_\>\> contention; HNSW thread safety

**Answer**: The read pool (8 connections default) saturates at approximately 8 concurrent blocking read operations. HNSW search acquires a shared read lock held only for the HNSW search call, making concurrent reads safe. TypedRelationGraph is cloned under a brief read lock and then traversed lock-free — RwLock contention is negligible below 100 concurrent queries.

**Evidence**:

*Read pool saturation:*
- `pool_config.rs`: `read_max_connections: 8` (default). PoolConfig `validate()` enforces only a zero-check — no upper bound on `read_max`.
- `db.rs`: read pool uses `.read_only(true)` connections opened via SqlitePoolOptions
- Read acquire timeout: 2 seconds
- SQL reads occupy the pool connection for the duration of the SQL operation (~5–50ms per search query's SQL portion)
- HNSW search and graph traversal happen in-memory after the SQL read, not occupying the pool
- At N=20 agents × 3 searches/sec = 60 SQL reads/sec; 8 connections × (1/0.010 avg) = 800 reads/sec capacity → ample headroom at normal latency
- At slow storage (50ms per read): 8 connections × 20 reads/sec = 160 reads/sec capacity. 20 agents × 3 searches/sec = 60 reads/sec — still within capacity

*HNSW concurrent read safety:*
- `crates/unimatrix-vector/src/index.rs`: `hnsw: RwLock<Hnsw<'static, f32, DistDot>>`
- `search()` method: acquires `self.hnsw.read()` (shared read lock), calls `hnsw.search(&self)`, releases lock, then acquires `self.id_map.read()` for result mapping
- Multiple callers can hold the read lock simultaneously — `RwLock::read()` grants shared access to N concurrent readers
- Insert holds a write lock (`self.hnsw.write()`) and blocks concurrent searches during insert. Background tick inserts from `context_store` briefly block all searches during the write window (~1–5ms per insert in normal operation)
- **Documented guarantee**: HNSW search is safe for N concurrent readers. The code pattern (RwLock read guard for search, write guard for insert) is correct for concurrent use.

*TypedRelationGraph RwLock contention:*
- `typed_graph.rs`: `TypedGraphStateHandle = Arc<RwLock<TypedGraphState>>`
- Search path: acquires read lock briefly to **clone** the TypedRelationGraph, releases lock, traverses the clone lock-free
- Clone duration: microseconds for small graphs; potentially milliseconds for 10,000+ nodes / 50,000+ edges (petgraph StableGraph clone)
- Background tick is sole writer (every 15 minutes, write lock held for ~1–2 seconds during graph rebuild)
- Contention at N=20 concurrent searches: 20 simultaneous read-lock acquisitions for clone. Negligible (read locks do not block each other)

**Recommendation**: Read pool of 8 connections is adequate for 20 concurrent agents. Expose `UNIMATRIX_READ_MAX_CONNECTIONS` env var to allow operator tuning above 20 agents (16 is a reasonable Wave 2 ceiling). HNSW concurrent reads are safe; no changes needed. TypedRelationGraph clone-under-read-lock may become a measurable latency contributor for very large graphs (50,000+ edges) under sustained concurrent load — monitor at realistic graph sizes in Wave 2 load testing.

---

### Q5: In-memory structures under multi-repo load — memory envelope at 10/20/50 repos; lazy vs. eager loading

**Answer**: The current architecture is **single-repo per server process** with no multi-repo dispatch. The Wave 2 enterprise server will need per-repo structures loaded concurrently. Memory estimates: 2–100 MB per repo depending on entry count. Lazy loading is necessary above ~20 active repos on typical server hardware.

**Evidence**: `crates/unimatrix-server/src/main.rs` wires a single `SqlxStore::open()`, single `VectorIndex::new()`, single TypedGraphStateHandle, PhaseFreqTableHandle. No routing table or multi-repo dispatch exists in the codebase.

**Per-repo memory envelope** (estimated from data structure definitions):

*VectorIndex*: `hnsw_rs` HNSW with `max_nb_connection=16`, 384-dimensional f32 embeddings (from `VectorConfig::default()`). Per entry: 384 × 4 bytes = 1,536 bytes embedding + ~128 bytes graph links = ~1.7 KB. IdMap: 2 HashMaps × N entries × ~50 bytes = ~100 bytes/entry.
- 1,000 entries: ~1.8 MB
- 10,000 entries: ~18 MB

*TypedRelationGraph*: petgraph `StableGraph<u64, RelationEdge>`. RelationEdge contains String fields totaling ~100–150 bytes per edge.
- 1,000 entries, 5 edges average: ~750 KB
- 10,000 entries, 5 edges average: ~7.5 MB

*EffectivenessState*: Two `HashMap<u64, _>` totaling ~50 bytes per entry.
- 10,000 entries: ~1 MB

*PhaseFreqTable*: `HashMap<(String, String), Vec<(u64, f32)>>`. At 10 phases × 20 categories × 100 entries per bucket: ~400 KB per repo.

*ConfidenceState*: 4 × f64 = 32 bytes. Negligible.

**Total per-repo estimates**:
- Small repo (1,000 entries): ~3–5 MB
- Medium repo (5,000 entries): ~30–50 MB
- Large repo (10,000 entries): ~60–100 MB

Additionally, each repo's SqlxStore holds SQLite page cache (configured at `cache_size = -16384`, 16 MB per connection × 8 read connections = up to 128 MB theoretical cache per repo). In practice, SQLite page cache is OS-managed and subject to memory pressure, but this is a design consideration.

**Multi-repo projections**:
- 10 medium repos: ~500 MB for in-memory structures
- 20 medium repos: ~1 GB
- 50 medium repos: ~2.5 GB

**Lazy vs. eager loading assessment**: Eager loading of all repos at startup is unacceptable above 20 repos on a standard server (8 GB RAM). The Wave 2 enterprise server must implement lazy loading with LRU eviction for in-memory structures. The eviction unit: VectorIndex (largest), TypedGraphState, PhaseFreqTable, EffectivenessState. The SqlxStore (connection pool) can remain open for lower-overhead reads even when in-memory structures are evicted. Cold-start on access after eviction: VectorIndex reconstruction from VECTOR_MAP may take 1–5 seconds for a large repo — this latency must be documented and potentially surfaced as a loading indicator.

**Recommendation**: Design the Wave 2 enterprise server with per-repo lazy loading from the start. This is a Wave 2 engineering task, not a future optimization — omitting it forecloses the 50-repo enterprise deployment target on typical server hardware. The per-repo eviction policy should target repos inactive for more than 30 minutes.

---

### Q6: Target session count for Wave 2 — evaluate 5, 20, 50 concurrent; inflection point

**Answer**: The defensible Wave 2 target is **20 concurrent agent sessions per repo**. This is achievable with the current architecture plus minor tuning (read pool size env var). The inflection point above which the architecture needs significant change is **50 concurrent agents per repo**.

**Evidence and reasoning**:

*Write path ceiling at N=5/20/50 per repo* (from Q1 analysis):
- N=5: trivially supported
- N=20: 40 integrity writes/sec at 2 writes/sec each — 20% of the 200 writes/sec ceiling at 5ms/write. Comfortable.
- N=50: 100 integrity writes/sec at 2 writes/sec each — 50% of ceiling. Manageable under normal storage. On slow storage (20ms/write, 50 writes/sec ceiling), N=20 already saturates.

*Read pool ceiling* (from Q4 analysis):
- N=20: 60 SQL reads/sec at 3 reads/sec each — within 8-connection default pool at normal latency
- N=50: 150 reads/sec — exceeds default pool without tuning; requires `read_max_connections=16`+

*Memory ceiling* (from Q5 analysis, single-repo):
- Single repo of any size: in-memory structures are bounded by one repo's envelope (≤100 MB for a large 10K-entry repo)
- The memory constraint is not per-session but per-simultaneously-active-repo

*Rate limiter*: Current default is 300 searches/hour per agent (from `RateLimitConfig::default()` in `gateway.rs`). This is per-agent, not per-repo. At N=20 agents × 5 searches/min = 100 searches/min per agent — well within the 300/hour limit.

**Concurrency ceiling summary**:

| Agent count | Write path | Read pool | Architecture state |
|---|---|---|---|
| N=5 | Trivial (<10% of ceiling) | Trivial | No changes needed |
| N=20 | 20–50% of ceiling at normal rates | Within 8-connection default | Minor tuning (read_max env var) |
| N=50 | Approaches ceiling on slow storage | Requires explicit tuning to 16+ | Tuning required; at limit |
| N=100 | Exceeds ceiling under sustained write pattern | Pool must be significantly enlarged | Architecture change needed |

**Recommendation**: Commit to **20 concurrent agents per repo** as the Wave 2 target in enterprise documentation. This is achievable without architecture changes. Document the ceiling honestly: "Supports up to 20 concurrent agents per repository on NVMe-backed storage; performance degrades gracefully toward 50 concurrent agents with read pool tuning but is not recommended above 20 for sustained write-heavy workloads." The 50-agent inflection point is where "tuning required" becomes "architecture change required" (PostgreSQL control plane, write pool separation).

---

### Q7: SaaS optionality additions — per-org write queue, rate limiting, Prometheus metrics, structured logging

**Answer**: Four additions evaluated against the SaaS optionality criteria. Two are Wave 2 recommended; one is Wave 2 required (per-credential rate limiting); one requires no action.

**Evidence**:

*1. Per-org write queue keying*

Current: analytics write queue is per-SqlxStore (per-repo). Each SqlxStore has its own drain task. Per-repo isolation is already per-org if each org has isolated repos.

**Assessment**: No Wave 2 action needed. The current per-repo drain task architecture already achieves per-org isolation if repos are not shared across orgs. Do not consolidate drain tasks — keep per-repo drain task isolation.

*2. Per-credential rate limiting*

Current: `RateLimiter` in `gateway.rs` is in-memory, keyed by `CallerId` (agent_id string from MCP session). UDS sessions are fully exempt. Rate limiter state resets on server restart. Defaults: 300 searches/hour, configurable write limit via `UNIMATRIX_WRITE_RATE_LIMIT` env var.

**Assessment**: **Wave 2 required** for SaaS optionality. Current rate limiting is per-agent-id-string, not per-OAuth-credential. For SaaS, a credential shared across multiple agent instances would bypass per-agent rate limiting. The UDS exemption must not extend to HTTP transport — the code explicitly warns this at `gateway.rs` but relies on convention, not structural enforcement.

Effort: **1–2 days** to add per-token-id rate limiting at the OAuth token validation entry point. Foreclosure risk if omitted: medium-high — retrofitting rate limiting into a deployed OAuth system is painful; designing it at token-validation time costs nothing extra.

*3. Prometheus metrics endpoint*

Current: No metrics endpoint. The only available signal is `shed_events_total()` on SqlxStore (an `Arc<AtomicU64>`). No HTTP endpoint, no request latency, no pool saturation, no write queue depth instrumentation.

**Assessment**: **Wave 2 recommended**. Without an observable metrics endpoint, SaaS operators cannot run Unimatrix in production. Key metrics: request count per tool type per agent, write queue depth, `shed_events_total`, pool acquire latency histograms, background tick completion time, audit log write latency.

Effort: **~2 days** using `metrics` + `metrics-exporter-prometheus` crates. Adding structured metric recording at tool handler entry points is the largest part. Foreclosure risk: medium — retrofitting instrumentation throughout the service layer post-Wave-2 is expensive; adding early is trivially cheap.

*4. Structured logging with org_id/project_id fields*

Current: `tracing` macros used throughout. Log events have local fields but no org_id/project_id threaded through request spans.

**Assessment**: **Wave 2 recommended**. Adding `org_id`/`project_id` to the `tracing::Span` at the MCP request boundary automatically propagates to all nested log events via tracing's span hierarchy. Required for SaaS log routing (tenant filtering by org_id in log aggregation pipelines).

Effort: **~1 day** — add span fields at MCP handler entry when OAuth context is resolved. Foreclosure risk: low — retrofittable without business logic changes. But trivially cheap to add alongside the OAuth implementation in Wave 2.

*5. Connection pool per org vs. shared*

**Assessment**: No Wave 2 action needed. The per-repo SqlxStore isolation already achieves per-org pool isolation if repos are org-scoped. The architecture does not foreclose per-org pooling — it is the current design.

---

## Unanswered Questions

1. **Actual SQLite write latency on target deployment hardware**: The write path analysis assumes 5–20ms per WAL transaction. On shared NFS volumes (Kubernetes), this can be 50–200ms, which pushes the N=20 agent write ceiling to N=5–8. Before committing to the "20 concurrent agents" target, measure actual SQLite write transaction latency in the target deployment environment (storage class: NVMe vs. EBS vs. NFS). Requires benchmarking.

2. **hnsw_rs concurrent insert + search behavior**: The analysis confirms search is read-lock-safe. Whether `hnsw_rs::insert_slice` is safe to call while other threads hold read guards depends on the hnsw_rs crate's internal concurrency model, which was not directly verified from hnsw_rs source code or documentation. The VectorIndex implementation's use of a write lock for insert is conservative and correct — but the exact blocking duration under concurrent load is not measured.

3. **Control plane DB schema design**: The minimum PostgreSQL compatibility changes are identified above, but they apply to control-plane-specific tables whose schema does not yet exist. The full control plane schema (OAuth tokens, role bindings, org management, audit log) is the ASS-042 deliverable. The PostgreSQL readiness analysis above is predictive for that yet-to-be-designed schema.

4. **PoolConfig read_max_connections upper bound**: `validate()` enforces only the zero-check on `read_max_connections`; no enforced upper bound. Increasing to 16+ is possible today but untested. Whether SQLite WAL benefits from more than 8 concurrent read connections (vs. increasing page cache contention and OS file descriptor pressure) requires empirical validation.

5. **VectorIndex cold-start latency on eviction/reload**: The lazy loading recommendation requires a re-load path (reconstruct VectorIndex from VECTOR_MAP on next access after eviction). Reconstruction of a 10,000-entry HNSW index from the VECTOR_MAP table could take 1–5 seconds. This cold-start latency spike must be measured and either acceptable to operators or mitigated (e.g., background pre-load on access signal).

---

## Out-of-Scope Discoveries

1. **Audit log write contention with integrity writes at write_max=1**: `log_audit_event` uses `write_pool` directly and blocks the calling MCP handler until the write completes. At write_max=1, the audit log write and the entry integrity write compete for the same connection within the same tool call execution. Effectively, every audited write operation requires two sequential connection acquisitions. Wave 2 consideration: separating the audit log into a dedicated write connection (using one of the write_max=2 slots) would halve effective write throughput contention per agent.

2. **UDS transport is permanently exempt from rate limiting**: `gateway.rs` exempts all `CallerId::UdsSession` callers from rate limiting by design. Wave 2 must ensure the HTTP transport `CallerId` variant does not inherit this exemption. This is a security requirement that must be enforced structurally at the CallerId type level, not by convention.

3. **RateLimiter state resets on server restart**: All per-agent sliding window state is in-memory. Server restart clears all rate limit windows. For SaaS, this means a restart grants all agents a fresh rate limit window — a potential fairness/abuse vector if server restarts are operator-accessible by tenants. Not a Wave 2 blocking issue for self-hosted enterprise, but worth noting for SaaS architecture.

4. **shed_events_total has no observable endpoint**: The `SqlxStore::shed_events_total()` counter is the only available signal that the analytics queue is under pressure. Under production load, the only operator-visible indicator of queue saturation is a WARN log line. This is insufficient for production operations. The Prometheus metrics endpoint recommendation directly addresses this.

5. **TypedRelationGraph clone is O(V+E) per search call**: The search hot path clones the full petgraph `StableGraph` on every search call after a brief read lock. For a large repo with 10,000 entries and 50,000 edges, this clone is non-trivial. At 20 concurrent searches simultaneously cloning a 50,000-edge graph, the combined allocation pressure could become a hidden latency contributor. An alternative (return a reference-counted snapshot pointer) would eliminate this cost. Not blocking for Wave 2, but worth profiling at realistic graph sizes.

---

## Recommendations Summary

| Question | Recommendation |
|---|---|
| Q1: Write path N=5/10/20 | 20 concurrent agents is the defensible write ceiling for same-repo load at normal rates (1–3 integrity writes/sec each). Analytics queue (1000 capacity, 100 events/sec drain) does not saturate below N=30. The audit log is synchronous (write_pool path), not batched — this is correct for SOC 2 integrity. |
| Q2: Control plane DB concurrency | No control plane exists yet. Predictive: SQLite WAL + read_max=6 adequate for Wave 2 control plane up to 50 agents. PostgreSQL upgrade trigger: >50 agents or >300 audit writes/sec sustained. |
| Q3: PostgreSQL migration readiness | Not ready for config-swap today. Five incompatibility categories: AUTOINCREMENT, INSERT OR IGNORE/REPLACE, json_extract, PRAGMA, sqlite_master. Control-plane-only migration: ~1 engineer-week. Full data-plane migration: ~2–3 weeks. Build Wave 2 control plane with PostgreSQL as a documented config option from the start. |
| Q4: Read path analysis | Read pool of 8 connections is adequate for 20 agents. Expose `UNIMATRIX_READ_MAX_CONNECTIONS` env var for tuning above 20 agents. HNSW concurrent reads are safe (read lock pattern is correct). TypedRelationGraph RwLock contention is negligible below 100 concurrent queries. |
| Q5: In-memory multi-repo structures | No multi-repo architecture exists today. Per-repo memory: 3–100 MB depending on entry count. Lazy loading is **required** for Wave 2 multi-repo server above ~20 active repos on typical server hardware (8 GB RAM). Design lazy eviction of VectorIndex + TypedGraphState as a Wave 2 engineering task, not a future optimization. |
| Q6: Target session count | **20 concurrent agents per repo** is the Wave 2 target (defensible, achievable without architecture changes). **50 agents** is the hard inflection point requiring architecture changes (PostgreSQL control plane, read pool expansion, lazy-load multi-repo). Document this ceiling honestly in enterprise documentation. |
| Q7: SaaS optionality | Per-org write queue keying: no action (per-repo isolation is sufficient). Per-credential rate limiting: **Wave 2 required** (1–2 days, wire at OAuth token validation). Prometheus metrics endpoint: **Wave 2 recommended** (2 days). Structured logging with org_id/project_id: **Wave 2 recommended** (1 day, add at OAuth span boundary). |
