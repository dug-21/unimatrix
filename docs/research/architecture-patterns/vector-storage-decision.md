# Vector Storage Decision: Build vs. Integrate

**Date**: 2026-02-19
**Status**: Decision analysis complete
**Context**: Unimatrix Context Engine requires vector storage for semantic search over development knowledge
**Decision Owner**: Solo architect

---

## Executive Summary

**Recommended: Option B -- Depend on underlying libraries directly (hnsw_rs + redb + simsimd as Cargo dependencies) and write a purpose-built wrapper of approximately 800-1200 lines.**

One-sentence rationale: The underlying libraries are mature, well-maintained, and proven; ruvector's wrapper adds integration complexity and code we would need to strip, modify, or work around, when we can write a tighter, better-fitted abstraction in less time than it would take to vendor, audit, strip, and adapt ruvector's files.

---

## Options Comparison Table

| Dimension | Option A: Vendor ruvector core | Option B: Direct library deps | Option C: Qdrant external service |
|---|---|---|---|
| **Lines to write** | ~500 (glue + extensions) | ~800-1200 (thin wrapper + extensions) | ~300 (client integration) |
| **Lines to audit/maintain** | ~2000-3000 (vendored) | ~0 (library code is upstream) | ~0 (service is external) |
| **Total owned code** | ~2500-3500 | ~800-1200 | ~300 |
| **Deployment model** | Single binary | Single binary | Binary + Docker container |
| **Namespace isolation** | Must build | Must build | Collections built-in |
| **Metadata filtering** | Must build (ruvector-filter is separate crate, not in core) | Must build | Built-in payload filtering |
| **Knowledge lifecycle** | Must build | Must build | Partially built (TTL, delete) |
| **Token-budget retrieval** | Must build | Must build | Must build |
| **Dependency count** | hnsw_rs, redb, simsimd, dashmap, bincode, serde, uuid + vendored code | hnsw_rs, redb, simsimd, dashmap, bincode, serde, uuid | qdrant-client, tonic, prost, tokio |
| **Build complexity** | Medium (must strip unused modules, fix imports) | Low (clean Cargo deps) | Low (standard crate) |
| **Risk of upstream bugs** | Inherits ruvector bugs (Issue #134 deadlock) | Only upstream lib bugs (well-maintained) | Qdrant bugs (rare, quickly patched) |
| **Migration difficulty** | Medium (coupled to ruvector's abstractions) | Low (thin wrapper, easy to swap) | High (coupled to Qdrant API) |
| **Operational overhead** | None (embedded) | None (embedded) | Significant (service lifecycle, networking, health checks) |
| **Single-binary friendly** | Yes | Yes | No |
| **Maturity of approach** | 3-month-old wrapper over mature libs | Direct use of mature libs | Mature service (3+ years) |

---

## Underlying Library Assessment

### hnsw_rs (HNSW Approximate Nearest Neighbor Search)

| Attribute | Assessment |
|---|---|
| **Latest version** | 0.3.3 (updated within last week as of 2026-02-19) |
| **Total downloads** | ~194K all-time, ~65K recent |
| **GitHub stars** | 231 (hnswlib-rs repo) |
| **Contributors** | 5 |
| **Open issues** | 5 |
| **License** | Apache-2.0 OR MIT |
| **Last commit** | Actively maintained (0.3.3 published days ago) |
| **Maturity** | Solid. Implements the Malkov-Yashunin HNSW algorithm faithfully. |
| **Performance** | 62K req/s on fashion-mnist, 15K req/s on SIFT-1M with 99%+ recall |
| **Key features** | Multi-threaded insert/search, FilterT trait for filtered search, dump/reload persistence, memory-mapped file support for large datasets |
| **API quality** | Clean generic API. `Hnsw::new()`, `insert()`, `insert_parallel()`, `search()`, `parallel_search()`, `file_dump()`. Supports custom distance functions. |
| **Filtering** | Built-in: pass allowed ID vectors or define filter functions called during search (not post-filter). This is important -- it means we can implement metadata filtering by providing a filter closure to the search call. |
| **Verdict** | **Production-ready.** Small, focused library doing one thing well. Active maintenance. The core of what ruvector wraps. |

Sources: [hnswlib-rs GitHub](https://github.com/jean-pierreBoth/hnswlib-rs), [hnsw_rs on crates.io](https://crates.io/crates/hnsw_rs), [hnsw_rs docs](https://docs.rs/hnsw_rs)

### redb (Embedded Key-Value Store)

| Attribute | Assessment |
|---|---|
| **Latest version** | 3.1.0 (September 2025) |
| **Total downloads** | High (4.2K GitHub stars) |
| **GitHub stars** | 4,200 |
| **Contributors** | 34 |
| **Open issues** | 11 |
| **License** | Apache-2.0 OR MIT |
| **Stability statement** | "Stable and maintained. File format is stable with upgrade path guarantees." |
| **Maturity** | Production-ready. Pure Rust, ACID transactions, MVCC, crash-safe, zero-copy reads. |
| **Performance** | Competitive with LMDB and RocksDB. Individual writes: 920ms (faster than LMDB's 1598ms and RocksDB's 2432ms in published benchmarks). |
| **Key features** | Copy-on-write B-trees, savepoints, MVCC isolation (serializable), single writer + multiple concurrent readers, zero unsafe code |
| **API quality** | Excellent. Table definitions via const, typed keys and values, clean transaction API. |
| **Namespace support** | Multiple named tables within a single database file, or separate database files per namespace. Both patterns work cleanly. |
| **Verdict** | **Excellent choice.** The most mature pure-Rust embedded KV store. Actively maintained, stable file format, battle-tested. Version 3.x is a strong signal of maturity. |

Sources: [redb GitHub](https://github.com/cberner/redb), [redb website](https://www.redb.org/), [redb docs](https://docs.rs/redb)

### SimSIMD (SIMD-Accelerated Distance Calculations)

| Attribute | Assessment |
|---|---|
| **Latest version** | 5.9.x (actively updated) |
| **GitHub stars** | 1,700 |
| **Contributors** | Multiple |
| **License** | Apache-2.0 |
| **Maturity** | Well-maintained. Header-only C99 with Rust bindings. 1,350+ commits. |
| **Performance** | Claims up to 200x faster than naive implementations. AVX2, AVX-512, NEON, SVE, SVE2 support. |
| **Rust binding quality** | Reasonable. Published on crates.io. Used as a dependency by ruvector-core (validates it works in Rust). |
| **Key features** | Cosine, Euclidean (L2), Dot Product, Hamming, Jaccard, KL divergence, Jensen-Shannon. Supports f64, f32, f16, i8, and binary vectors. |
| **Alternative** | For our use case (f32 cosine/euclidean on 768-3072 dim vectors), hnsw_rs has its own distance calculations via the `anndists` crate. We may not even need simsimd as a direct dependency if we rely on hnsw_rs's built-in distance support. |
| **Verdict** | **Good library, but potentially redundant.** hnsw_rs already handles distance calculations internally. SimSIMD becomes useful if we need standalone distance operations outside of HNSW search (e.g., re-ranking, deduplication checks). |

Sources: [SimSIMD GitHub](https://github.com/ashvardanian/SimSIMD), [simsimd on crates.io](https://crates.io/crates/simsimd)

### Supporting Libraries

| Library | Role | Maturity | Notes |
|---|---|---|---|
| **dashmap** | Concurrent hash map for in-memory caches | Mature, widely used | Standard choice in Rust ecosystem |
| **bincode** | Binary serialization for vector data | Mature | v2.0 RC is stable enough; alternatively use rkyv for zero-copy |
| **serde/serde_json** | Metadata serialization | Industry standard | No concerns |
| **uuid** | ID generation | Industry standard | No concerns |

---

## What We'd Need to Build for Each Option

### Common to All Options (Must Build Regardless)

These features are Unimatrix-specific and do not exist in any off-the-shelf solution:

| Feature | Estimated Lines | Notes |
|---|---|---|
| Knowledge lifecycle management (active/aging/deprecated states, decay, promotion) | ~200 | Metadata fields + state machine logic |
| Token-budget-aware retrieval (retrieve top-k until budget exhausted) | ~80 | Post-search truncation with token counting |
| Temporal awareness (last-used, last-validated timestamps, recency scoring) | ~100 | Metadata fields + scoring adjustments |
| Phase-aware filtering (architecture/coding/testing context) | ~60 | Metadata enum + filter predicates |
| Knowledge level scoping (session/project/global) | ~50 | Metadata field + query parameter |
| **Subtotal** | **~490** | |

### Option A: Vendor ruvector core (additional work)

| Task | Effort | Risk |
|---|---|---|
| Clone and extract 7-8 files from 79-crate workspace | 1-2 hours | Low |
| Strip broken module references (graph, raft, gnn imports) | 2-4 hours | Medium -- may find unexpected coupling |
| Audit vendored code for Issue #134 deadlock pattern | 2-3 hours | Medium -- must understand their locking strategy |
| Fix or work around redb transaction locking issues | 2-4 hours | High -- root cause may be in their abstraction layer |
| Adapt Cargo.toml (strip 50+ unused dependencies) | 1-2 hours | Low |
| Build namespace isolation on top of vendored VectorDB | 3-4 hours | Medium -- fighting their abstractions vs. our needs |
| Build metadata filtering using vendored types | 3-4 hours | Medium -- their types may not fit our schema |
| Replace embeddings.rs hash implementation | 2-3 hours | Low |
| Ongoing: maintain vendored code as upstream evolves | Ongoing | Bus factor = 1 upstream developer |
| **Total initial** | **~16-26 hours** | |
| **Ongoing maintenance** | **Medium** -- we own the code but did not design it | |

### Option B: Direct library dependencies (additional work)

| Task | Effort | Risk |
|---|---|---|
| Write `VectorStore` trait + struct (~200 lines) | 3-4 hours | Low -- well-understood pattern |
| Write redb persistence layer (~250 lines) | 3-4 hours | Low -- redb has excellent docs |
| Write HNSW wrapper with ID mapping (~200 lines) | 2-3 hours | Low -- hnsw_rs API is clean |
| Write namespace isolation (per-project DB + index files) | 2-3 hours | Low -- straightforward file isolation |
| Write metadata filtering with hnsw_rs FilterT (~150 lines) | 2-3 hours | Low -- use hnsw_rs's built-in filter callback |
| Write batch operations (~100 lines) | 1-2 hours | Low |
| **Total initial** | **~13-19 hours** | |
| **Ongoing maintenance** | **Low** -- we wrote it, we understand it, dependencies are stable | |

### Option C: Qdrant external service (additional work)

| Task | Effort | Risk |
|---|---|---|
| Write qdrant-client integration (~300 lines) | 3-4 hours | Low -- well-documented API |
| Docker Compose configuration | 1-2 hours | Low |
| Health check / reconnection logic | 2-3 hours | Medium -- network failure handling |
| Namespace isolation via Qdrant collections | 1-2 hours | Low -- native support |
| Metadata filtering via Qdrant payload filters | 1-2 hours | Low -- native support |
| Operational: Qdrant container lifecycle management | Ongoing | Medium -- another process to monitor |
| Operational: Qdrant upgrades, data migration | Ongoing | Medium -- version compatibility |
| Operational: Backup/restore coordination | 2-3 hours | Medium -- two systems to back up |
| **Total initial** | **~10-16 hours** | |
| **Ongoing maintenance** | **High** -- external service lifecycle, container management, cross-process debugging | |

---

## Deployment and Operational Complexity

### For a Solo Architect Running Locally via Docker

| Dimension | Option A (Vendor) | Option B (Direct Libs) | Option C (Qdrant Service) |
|---|---|---|---|
| Docker containers | 1 (unimatrix) | 1 (unimatrix) | 2 (unimatrix + qdrant) |
| Startup sequence | `docker compose up` | `docker compose up` | `docker compose up` (with dependency ordering) |
| Memory footprint | ~100-200MB (vectors in-process) | ~100-200MB (vectors in-process) | ~200MB (unimatrix) + ~200-500MB (qdrant) |
| Network dependencies | None (embedded) | None (embedded) | localhost gRPC (latency ~0.5-2ms per call) |
| Failure modes | Process crash = restart | Process crash = restart | Process crash + service crash + network partition |
| Data location | Single directory (~/.unimatrix/) | Single directory (~/.unimatrix/) | Two locations (app data + qdrant data volume) |
| Backup | Copy one directory | Copy one directory | Coordinate two backup sources |
| Binary size | ~15-30MB | ~15-30MB | ~15MB + 150MB+ Qdrant image |
| Cold start | <2 seconds | <2 seconds | <2 seconds (app) + 3-10 seconds (qdrant) |
| Debug experience | Single process, single log | Single process, single log | Two processes, two log streams, network tracing |

**Assessment**: For a solo architect who values simplicity, single-binary embedded storage (Options A or B) is decisively superior to running a separate service (Option C). The added operational overhead of Qdrant buys features we would need to build custom anyway (lifecycle management, token-budget retrieval, phase-aware filtering). We would use maybe 20% of Qdrant's capabilities while absorbing 100% of its operational cost.

---

## Migration Path (Can We Switch Later?)

### From Option B to Qdrant (if we outgrow embedded)

**Difficulty: Low-Medium.** If we design Option B with a `VectorStore` trait:

```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn insert(&self, id: &str, vector: &[f32], metadata: &Metadata) -> Result<()>;
    async fn search(&self, query: &[f32], k: usize, filter: Option<&Filter>) -> Result<Vec<SearchResult>>;
    async fn delete(&self, id: &str) -> Result<()>;
    async fn get(&self, id: &str) -> Result<Option<VectorEntry>>;
    async fn list(&self, filter: Option<&Filter>, limit: usize) -> Result<Vec<VectorEntry>>;
    async fn count(&self, filter: Option<&Filter>) -> Result<usize>;
}
```

Then migrating to Qdrant is implementing this trait against qdrant-client. The data migration is: iterate all entries from redb, re-insert into Qdrant collections. Estimated migration effort: ~1-2 days.

### From Option A to Option B (if vendored code becomes problematic)

**Difficulty: Medium.** Must rewrite the wrapper layer to remove ruvector abstractions. The underlying libraries (hnsw_rs, redb) are the same, so data files are compatible. Estimated effort: ~1-2 days.

### From Option B to Option A (if we decide we want ruvector's code)

**Difficulty: Low.** We can always vendor later if we find ruvector's code solves a problem we are struggling with. This direction is always available.

### From Option C to Option B (if we want to drop the external service)

**Difficulty: High.** Must rewrite against different storage API, migrate data out of Qdrant, build everything Qdrant was providing (filtering, collections, persistence). Estimated effort: ~3-5 days.

**Key insight**: Option B gives us the best optionality. We can migrate up to Qdrant or sideways to vendoring with minimal friction.

---

## Risk Analysis

### Option A: Vendor ruvector core

| Risk | Likelihood | Impact | Notes |
|---|---|---|---|
| Vendored code has latent bugs (Issue #134 deadlock pattern) | Medium | High | Solo developer, known bugs in adjacent modules |
| ruvector's abstractions fight our needs | Medium | Medium | Their VectorDB was designed for their use case, not ours |
| Dependency version conflicts with vendored Cargo.toml | Medium | Low | Solvable but time-consuming |
| Upstream fixes don't flow to our fork | High | Medium | We would need to manually track and cherry-pick |
| Code we don't fully understand becomes load-bearing | Medium | High | 2-3K lines is enough to hide subtle issues |

### Option B: Direct library dependencies

| Risk | Likelihood | Impact | Notes |
|---|---|---|---|
| hnsw_rs API changes in future versions | Low | Low | Stable crate, semantic versioning, we pin versions |
| Our wrapper has bugs | Medium | Medium | But they are OUR bugs, in code we wrote, with full context |
| redb file format changes | Very Low | Medium | redb explicitly guarantees file format stability and upgrade paths |
| Performance is worse than ruvector's wrapper | Low | Low | We use the same libraries; any perf difference is in glue code |
| We miss an optimization ruvector implemented | Low | Low | We can always read their code for ideas |

### Option C: Qdrant external service

| Risk | Likelihood | Impact | Notes |
|---|---|---|---|
| Qdrant service crashes/hangs | Low | High | Takes down all vector operations |
| Docker networking issues | Medium | Medium | localhost should be reliable but adds failure mode |
| Qdrant version upgrade breaks data format | Low | High | Must coordinate upgrades carefully |
| Over-engineering: using a distributed DB for a single-user system | High | Medium | Paying complexity cost for scale we don't need |
| Vendor lock-in to Qdrant's API and data format | Medium | High | Harder to migrate away from |

### Risk Summary

| Option | Overall Risk Level | Primary Concern |
|---|---|---|
| A: Vendor ruvector | **Medium-High** | Inheriting someone else's bugs and abstractions |
| B: Direct libs | **Low** | Writing ~1000 lines of straightforward code |
| C: Qdrant service | **Medium** | Operational complexity for a solo developer |

---

## Recommendation

### Decision: Option B -- Direct Library Dependencies

Write a purpose-built vector storage layer using hnsw_rs, redb, and the standard Rust serialization stack as direct Cargo dependencies. Do not vendor ruvector code. Do not run Qdrant as an external service.

### Rationale (in priority order)

1. **We need less code than ruvector provides, not more.** Ruvector's 2-3K lines of core include quantization (4 methods), multiple index backends, connection pooling, and embedding provider abstractions we do not need for V1. Our wrapper needs insert, search, delete, persist, filter. That is ~800-1200 lines of code we write ourselves and fully understand.

2. **The underlying libraries are the value, not the wrapper.** hnsw_rs provides the HNSW algorithm with built-in filter support. redb provides ACID persistence. These are the hard problems, and they are solved. What ruvector adds on top is ID mapping, serialization glue, and distance metric abstraction -- all straightforward to write.

3. **ruvector has confirmed bugs in its core abstractions.** Issue #134 (deadlock on second insert) is in the VectorDB/storage layer we would be vendoring. Even if this specific bug is fixed, the pattern -- a solo developer moving fast across 79 crates with sparse testing -- means more latent issues likely exist. Writing 1000 lines ourselves is safer than auditing 3000 lines of someone else's code.

4. **Single-binary deployment is a hard requirement.** Both Options A and B satisfy this. Option C does not. For a local-first Docker system used by a solo architect, eliminating the Qdrant container removes an entire class of operational concerns (service health, networking, coordinated backups, container lifecycle).

5. **Option B preserves maximum optionality.** With a clean `VectorStore` trait, we can migrate to Qdrant if we outgrow embedded storage, or vendor ruvector code if we find we need their quantization implementations. The reverse migrations are harder.

6. **The build-vs-integrate math favors building.** Option B takes ~13-19 hours of initial development. Option A takes ~16-26 hours (mostly auditing, stripping, and fixing vendored code). Option B produces code we fully understand; Option A produces code we partially understand. The effort is comparable, but the quality of understanding is not.

### Implementation Sketch

#### Dependencies (Cargo.toml)

```toml
[dependencies]
# Core vector storage
hnsw_rs = "0.3"          # HNSW approximate nearest neighbor
redb = "3.1"             # Embedded key-value store (ACID, pure Rust)

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bincode = "2.0.0-rc.3"   # Binary serialization for vector data

# Concurrency
dashmap = "6.0"           # Concurrent hash map for caches

# Utilities
uuid = { version = "1.0", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2.0"
tracing = "0.1"

# Optional: standalone SIMD distance (only if needed outside hnsw_rs)
# simsimd = "5.9"
```

Note: simsimd is listed as optional. hnsw_rs uses the `anndists` crate internally for distance calculations, which covers our HNSW search needs. We only need simsimd if we require standalone distance computations for re-ranking, deduplication thresholds, or other operations outside of HNSW search.

#### Module Structure

```
crates/unimatrix-vectors/
  src/
    lib.rs              # Public API, re-exports
    store.rs            # VectorStore trait definition (~50 lines)
    embedded.rs         # EmbeddedVectorStore implementation (~350 lines)
                        #   - hnsw_rs index management
                        #   - redb persistence
                        #   - insert/search/delete/list/count
    persistence.rs      # redb table definitions, serialization (~200 lines)
                        #   - VectorEntry table (id -> vector + metadata)
                        #   - HNSW index save/load
    filter.rs           # Metadata filtering via hnsw_rs FilterT (~150 lines)
                        #   - Phase filter (architecture/coding/testing)
                        #   - Knowledge level filter (session/project/global)
                        #   - Status filter (active/aging/deprecated)
                        #   - Temporal filter (last-used recency)
                        #   - Composite AND/OR filter
    namespace.rs        # Per-project isolation (~100 lines)
                        #   - Separate redb file per project
                        #   - Separate HNSW index file per project
                        #   - Namespace-scoped operations
    lifecycle.rs        # Knowledge lifecycle management (~150 lines)
                        #   - State transitions (active -> aging -> deprecated)
                        #   - Confidence decay on time-without-use
                        #   - Promotion on repeated successful retrieval
                        #   - Batch cleanup of deprecated entries
    retrieval.rs        # Token-budget-aware retrieval (~80 lines)
                        #   - Search top-k, accumulate until budget hit
                        #   - Return results + remaining budget
    types.rs            # Core types (~100 lines)
                        #   - VectorEntry, SearchResult, Metadata
                        #   - Filter, KnowledgeLevel, Phase, Status
    error.rs            # Error types (~30 lines)
  Cargo.toml
```

**Estimated total: ~1,200 lines of focused, purpose-built code.**

#### Key Design Decisions

1. **One redb database file + one HNSW index file per project.** Physical isolation prevents any possibility of cross-project data leakage. This is simpler and safer than logical namespace separation within a shared database.

2. **Metadata stored in redb, vectors in HNSW index.** redb stores the full `VectorEntry` (id, vector, metadata, timestamps, status). The HNSW index stores only the vector for fast similarity search. On search: HNSW returns candidate IDs with distances, then we look up full metadata from redb and apply post-filters or return directly.

3. **hnsw_rs FilterT for pre-filtering.** For common filters (status=active, knowledge_level=project), we use hnsw_rs's built-in filter callback to avoid scanning irrelevant entries during search. This gives us filtered ANN search without post-filtering waste.

4. **Lifecycle as metadata state machine.** Each entry has a `status` field (Active/Aging/Deprecated) and timestamps (created_at, last_used_at, last_validated_at). A background task or explicit `consolidate` call applies decay rules and state transitions. This is domain logic, not storage logic -- it belongs in our code, not in a library.

5. **VectorStore trait for future migration.** The trait is async-ready even though the embedded implementation is synchronous (wrapped in `spawn_blocking`). This lets us swap in a Qdrant-backed implementation later without changing call sites.

#### Sketch of Core Implementation

```rust
use hnsw_rs::hnsw::Hnsw;
use hnsw_rs::dist::DistCosine;
use redb::{Database, TableDefinition, ReadableTable};

const VECTORS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vectors");

pub struct EmbeddedVectorStore {
    db: Database,
    index: RwLock<Hnsw<f32, DistCosine>>,
    id_map: DashMap<String, usize>,  // external ID -> HNSW internal ID
    next_id: AtomicUsize,
    data_dir: PathBuf,
}

impl EmbeddedVectorStore {
    pub fn open(project_dir: &Path) -> Result<Self> {
        let db = Database::create(project_dir.join("vectors.redb"))?;
        let index = Self::load_or_create_index(project_dir)?;
        let id_map = Self::rebuild_id_map(&db)?;
        Ok(Self { db, index: RwLock::new(index), id_map, next_id: AtomicUsize::new(0), data_dir: project_dir.to_path_buf() })
    }

    pub fn insert(&self, id: &str, vector: &[f32], metadata: &Metadata) -> Result<()> {
        let internal_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        // Persist to redb
        let entry = VectorEntry { id: id.to_string(), vector: vector.to_vec(), metadata: metadata.clone(), /* timestamps */ };
        let txn = self.db.begin_write()?;
        { let mut table = txn.open_table(VECTORS_TABLE)?; table.insert(id, bincode::encode(&entry)?)?; }
        txn.commit()?;
        // Insert into HNSW index
        let mut index = self.index.write();
        index.insert((&vector, internal_id));
        self.id_map.insert(id.to_string(), internal_id);
        Ok(())
    }

    pub fn search(&self, query: &[f32], k: usize, filter: Option<&Filter>) -> Result<Vec<SearchResult>> {
        let index = self.index.read();
        let neighbours = match filter {
            Some(f) => {
                let filter_fn = self.build_filter_fn(f);
                index.search_with_filter(query, k, &filter_fn)
            }
            None => index.search(query, k, /* ef_search */ 32)
        };
        // Map HNSW results back to full entries via redb lookup
        self.resolve_results(neighbours)
    }
}
```

This is approximately the same logic ruvector's `vector_db.rs` implements, but tailored exactly to our types, our error handling, our metadata schema, and our lifecycle requirements. No stripping, no auditing, no working around someone else's abstractions.

---

## What About Quantization?

ruvector-core includes scalar, product, and binary quantization implementations (~500 lines). These are genuinely well-written with SIMD optimizations. However:

- **V1 does not need quantization.** At 10K-100K vectors of 768-1536 dimensions, unquantized f32 vectors fit comfortably in memory (~30-600MB). Quantization matters at millions of vectors.
- **If we need it later**, we have three options: (a) implement scalar quantization ourselves (~100 lines for the basic case), (b) vendor just ruvector's `quantization.rs` at that point, or (c) switch to Qdrant which has built-in quantization.
- **Decision**: Skip quantization for V1. Revisit when vector count exceeds 500K per project.

---

## What About Qdrant? (Why Not Option C)

Qdrant is an excellent piece of software. The decision against it is not about quality but about fit:

1. **We would use ~20% of Qdrant's capabilities.** We need: insert, search with filter, delete, collections. We do not need: gRPC API, REST API, snapshots, replicas, sharding, consensus, multi-node deployment, recommendation API, scroll API, cluster management, API keys, or any of the other features that make Qdrant a production-grade distributed system.

2. **100% of the features we need to build are custom regardless.** Knowledge lifecycle, phase-aware filtering, token-budget retrieval, temporal awareness, and learning integration -- none of these exist in Qdrant. These are Unimatrix-specific domain features. Whether we store vectors in Qdrant or in our own redb, we write the same domain code.

3. **The operational cost is non-trivial for a solo developer.** A second Docker container means: startup ordering, health check dependencies, coordinated shutdown, two sets of logs, two backup targets, network debugging when things go wrong, and version upgrade coordination. For a team with ops support, this is fine. For a solo architect, it is friction without value.

4. **The performance characteristics favor embedded.** Our search latency target is <10ms. In-process hnsw_rs search on 10K vectors: <1ms. Qdrant over localhost gRPC: 2-5ms. Both meet the target, but the embedded path has simpler performance characteristics (no serialization, no network, no connection pool).

5. **We can always add Qdrant later.** The VectorStore trait means switching to Qdrant is ~1-2 days of work if we ever need distributed search, multi-node deployment, or Qdrant's advanced features. Starting with embedded does not close this door.

---

## Decision Summary

| Question | Answer |
|---|---|
| Should we vendor ruvector's core files? | **No.** The wrapper adds complexity without proportional value. Known bugs (Issue #134) in the storage layer. Solo developer upstream. |
| Should we use the same underlying libraries? | **Yes.** hnsw_rs (0.3.3, actively maintained, 194K downloads), redb (3.1, 4.2K stars, "stable and maintained"), and potentially simsimd are the right building blocks. |
| Should we run Qdrant as an external service? | **Not for V1.** Over-engineered for a single-user system. Adds operational complexity without reducing our custom code burden. Keep it as a migration target if we outgrow embedded. |
| How much code do we need to write? | **~1,200 lines** for a purpose-built vector storage layer with namespace isolation, metadata filtering, lifecycle management, and token-budget retrieval. |
| Can we switch if we are wrong? | **Yes.** VectorStore trait abstracts the implementation. Migration to Qdrant: ~1-2 days. Vendoring ruvector code later: always available. |

---

## Sources

- [hnswlib-rs GitHub](https://github.com/jean-pierreBoth/hnswlib-rs) -- HNSW implementation in Rust
- [hnsw_rs on crates.io](https://crates.io/crates/hnsw_rs) -- Package registry listing
- [hnsw_rs API docs](https://docs.rs/hnsw_rs/latest/hnsw_rs/hnsw/index.html) -- API documentation
- [redb GitHub](https://github.com/cberner/redb) -- Embedded key-value database
- [redb website](https://www.redb.org/) -- Official site with stability statement
- [redb docs](https://docs.rs/redb) -- API documentation
- [SimSIMD GitHub](https://github.com/ashvardanian/SimSIMD) -- SIMD-accelerated distance calculations
- [simsimd on crates.io](https://crates.io/crates/simsimd) -- Package registry listing
- [Qdrant GitHub](https://github.com/qdrant/qdrant) -- Vector database
- [Qdrant Rust client](https://github.com/qdrant/rust-client) -- Rust client library
- [ruvector-core on lib.rs](https://lib.rs/crates/ruvector-core) -- ruvector core library
- [Unimatrix ruvector analysis](../tools-evaluation/ruvector-analysis.md) -- Internal analysis of ruvector
- [Unimatrix research synthesis](../synthesis/research-synthesis-r1.md) -- Internal research synthesis
