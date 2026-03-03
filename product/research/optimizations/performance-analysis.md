# Unimatrix Performance Analysis

**Date**: 2026-03-03
**Scope**: All Rust source code across 8 crates
**Type**: Research only (no code modifications)

---

## Executive Summary

The Unimatrix codebase is well-structured with clean separation of concerns across crates. The primary performance bottleneck is the **ONNX inference pipeline** behind a single `Mutex<Session>`, which serializes all embedding operations. The second most impactful issue is the **co-access full table scan** in `get_co_access_partners`, which executes on every search result's boost computation. Most other findings are moderate-impact allocation patterns and architectural choices that compound under load but are acceptable at current scale (~200 entries, single-user MCP server).

**Estimated impact ranking** (by user-visible latency):
1. ONNX Mutex serialization -- blocks all concurrent embedding
2. Co-access full table scan -- O(n) on CO_ACCESS per anchor per search
3. Double sort in context_search -- redundant work on every search
4. spawn_blocking overhead on trivial operations -- ~100us per call
5. Unnecessary Vec allocations in embed/vector pipeline -- memory pressure

---

## 1. Hot Path Analysis

The primary hot paths, in order of frequency during normal MCP usage:

### 1.1 Search Path (context_search)

**Call chain**: `context_search` -> `embed_entry` -> `vector.search` -> `store.get` (per result) -> `rerank_score` -> `compute_search_boost` -> second sort

**Finding HP-01: Double sort in context_search**
- **File**: `crates/unimatrix-server/src/tools.rs:354-399`
- **Severity**: Medium
- **Description**: Results are sorted twice: once at line 354 (by rerank_score + provenance), then again at line 389 (by rerank_score + provenance + co-access boost). When `boost_map` is non-empty (which it typically is once the system has usage data), the first sort at line 354 is fully wasted work.
- **Impact**: For k=10 results, this is O(n log n) wasted comparison+branch work. Small n mitigates severity, but the sort closures each recompute `rerank_score` for every comparison -- no caching of computed scores.
- **Recommendation**: Compute final scores once into a `Vec<(f64, usize)>`, sort once. Alternatively, defer the first sort entirely and only sort after boost computation.

**Finding HP-02: Per-result entry fetch via spawn_blocking**
- **File**: `crates/unimatrix-server/src/tools.rs:342`
- **Severity**: Medium
- **Description**: After vector search returns N results, each result triggers `self.entry_store.get(sr.entry_id).await`, which goes through `AsyncEntryStore::get` -> `spawn_blocking` -> `Store::get`. Each call opens a separate read transaction and spawns a blocking task. For k=10 results, this is 10 separate `spawn_blocking` calls.
- **Impact**: ~100us overhead per spawn_blocking (task spawn + thread pool scheduling). 10 results = ~1ms overhead just from task scheduling, on top of the actual redb reads.
- **Recommendation**: Batch fetch -- add a `get_batch(&[u64]) -> Vec<EntryRecord>` method that opens one read transaction and fetches all entries. Wrap in a single `spawn_blocking`.

**Finding HP-03: Co-access boost queries one anchor at a time**
- **File**: `crates/unimatrix-engine/src/coaccess.rs:114-121`
- **Severity**: Medium-High
- **Description**: `compute_boost_internal` iterates over anchor IDs and calls `store.get_co_access_partners(anchor_id, ...)` for each one. Each call opens a new read transaction and does a prefix scan + full table scan (see HP-04). With 3 anchors, this is 3 separate transactions, 3 prefix scans, and 3 full table scans.
- **Recommendation**: Add a `get_co_access_partners_batch` that opens one transaction and processes all anchors.

**Finding HP-04: Co-access full table scan (Scan 2)**
- **File**: `crates/unimatrix-store/src/read.rs:243-253`
- **Severity**: High
- **Description**: `get_co_access_partners` does two scans. Scan 1 is an efficient prefix scan for pairs where `entry_id` is the min key. Scan 2 is a **full table scan** of the entire CO_ACCESS table to find pairs where `entry_id` is the max key (i.e., the second element in the `(min, max)` composite key). This is O(n) where n is the total number of co-access pairs.
- **Impact**: With 368 co-access pairs currently, this is negligible. At 10K+ pairs, this becomes the dominant cost in the search path. Called once per anchor (typically 3 times per search).
- **Root cause**: The CO_ACCESS table uses `(min_id, max_id)` as composite key, so prefix scanning only works for the min_id dimension.
- **Recommendation**: Add a reverse index table `CO_ACCESS_REVERSE: (max_id, min_id) -> ()` to enable prefix scans in both directions. Alternatively, store pairs bidirectionally: insert both `(a, b)` and `(b, a)`.

### 1.2 Embedding Path

**Finding HP-05: ONNX Session behind Mutex**
- **File**: `crates/unimatrix-embed/src/onnx.rs:22`
- **Severity**: High (for concurrent workloads)
- **Description**: The ONNX `Session` is wrapped in `Mutex<Session>`, serializing ALL inference. The `tokenizer` is lock-free, but the session lock is held for the entire inference duration (~5-20ms for MiniLM-L6). This means:
  - Concurrent `context_store` calls queue behind one another
  - UDS hook injection embeds are serialized with MCP tool embeds
  - Batch embedding (`embed_batch_internal`) processes sub-batches sequentially
- **Impact**: At current single-user load, this is rarely a bottleneck. Under multi-agent swarm scenarios (5+ agents), this becomes the primary bottleneck since every store/search triggers embedding.
- **Recommendation**: Pool multiple ONNX sessions (e.g., 2-4 sessions). Use a semaphore or channel-based pool. ONNX Runtime supports multiple sessions sharing the same model graph.

**Finding HP-06: Redundant Vec allocations in embed pipeline**
- **File**: `crates/unimatrix-embed/src/onnx.rs:91-102, 129`
- **Severity**: Low
- **Description**: In `embed_single`:
  - Line 91-93: Three `Vec<i64>` allocations for token conversion (input_ids, attention_mask, token_type_ids). Each is `seq_len` elements (~128 i64s = 1KB each).
  - Line 129: `output_data.to_vec()` copies the entire ONNX output tensor (`seq_len * hidden_dim * sizeof(f32)` = ~196KB for max sequence length).
  - These allocations happen under the Mutex lock, extending lock hold time.
- **Recommendation**: Pre-allocate reusable buffers (attached to the provider or via thread-local). Move `to_vec()` outside the lock by extracting shape info first, then doing the copy after unlock. This requires restructuring the lock scope.

### 1.3 Store Write Path

**Finding HP-07: Content hash on every insert**
- **File**: `crates/unimatrix-store/src/hash.rs` (called from `write.rs`)
- **Severity**: Low
- **Description**: `compute_content_hash` computes SHA-256 over `format!("{title}:{content}")`. The `format!` allocates a String that concatenates title and content. For a 1KB entry, this allocates ~1KB, hashes it, then drops the String.
- **Impact**: Negligible at current volume. SHA-256 is ~500MB/s; the allocation is the slower part.
- **Recommendation**: Use `Sha256::update()` incrementally (title, separator, content) to avoid the intermediate allocation.

---

## 2. Concurrency Analysis

### 2.1 Lock Contention Points

| Lock | Type | Location | Contention Risk |
|------|------|----------|-----------------|
| ONNX Session | `Mutex<Session>` | `onnx.rs:22` | **High** -- held for 5-20ms per embed |
| HNSW Index | `RwLock<Hnsw>` | `index.rs:50` | Low -- readers concurrent, write rare |
| HNSW IdMap | `RwLock<IdMap>` | `index.rs:54` | Low -- fast HashMap ops |
| UsageDedup | `Mutex<DedupState>` | `usage_dedup.rs` | Low -- fast HashSet ops |
| PendingEntries | `Mutex<HashMap>` | `server.rs` | Low -- 1000-entry cap |
| CategoryAllowlist | `RwLock` | `scanning.rs` | Low -- read-heavy |

**Finding CC-01: ONNX Mutex is the only high-contention lock**
The ONNX Mutex serializes the most expensive operation in the system (neural inference). All other locks protect fast in-memory operations. The HNSW `RwLock` is well-designed -- reads (search) are concurrent, writes (insert) are exclusive but fast.

### 2.2 spawn_blocking Usage

**Finding CC-02: Excessive spawn_blocking for trivial operations**
- **File**: `crates/unimatrix-core/src/async_wrappers.rs` (entire file)
- **Severity**: Low-Medium
- **Description**: Every single store operation goes through `spawn_blocking`, including:
  - `contains(id)` -- a single B-tree lookup, ~1us
  - `read_counter(name)` -- a single key lookup, ~1us
  - `exists(id)` -- a single key lookup, ~1us
  - `point_count()` -- reads an atomic counter, ~10ns
  - `dimension()` -- returns a constant, ~1ns
- Each `spawn_blocking` has overhead: Arc::clone, closure construction, task spawn on tokio's blocking pool, cross-thread result transfer. This is ~50-100us per call.
- **Impact**: For `point_count()` and `dimension()`, the overhead is 1000x-10000x the actual operation cost. For `get()` and `query()`, the overhead is acceptable since the underlying redb operations take 10-100us.
- **Recommendation**: For trivial accessors (`point_count`, `dimension`, `contains`, `stale_count`), bypass `spawn_blocking` and call directly. These do not perform I/O. For redb reads that are truly fast (single key lookups like `get`, `exists`, `read_counter`), consider using `tokio::task::block_in_place` in the calling context instead of `spawn_blocking`, which avoids the cross-thread hop while still being async-compatible.

### 2.3 Poison Recovery

The codebase consistently uses `unwrap_or_else(|e| e.into_inner())` for poisoned lock recovery on both `RwLock` and `Mutex` guards. This is a sound defensive pattern that prevents cascading panics from a single thread's failure.

---

## 3. Memory & Allocation Findings

### 3.1 EntryRecord Allocation Weight

**Finding MA-01: EntryRecord has 24 fields with heavy String allocation**
- **File**: `crates/unimatrix-store/src/schema.rs`
- **Description**: `EntryRecord` contains 12 String fields and 1 `Vec<String>` (tags). Each `deserialize_entry` call allocates all of these. In the search path, up to k=10 entries are deserialized, meaning 10 * ~13 heap allocations = ~130 allocations per search.
- **Impact**: At current scale, negligible. Becomes relevant at k=50+ or with very long content fields.
- **Recommendation**: No action needed at current scale. If this becomes a bottleneck, consider a "lite" deserialization that only reads id/title/confidence/category for ranking, deferring full deserialization to the final results.

### 3.2 HashSet Intersection Pattern

**Finding MA-02: Repeated HashSet allocation in query intersection**
- **File**: `crates/unimatrix-store/src/query.rs:64-67` and `crates/unimatrix-store/src/read.rs:53-56`
- **Description**: Combined query intersects filter results using:
  ```rust
  result_ids = result_ids.intersection(&set).copied().collect();
  ```
  This creates a new `HashSet` for each intersection step. With 5 filter dimensions, this is 4 intermediate HashSets.
- In `collect_ids_by_tags` (read.rs:55), the same pattern applies per tag.
- **Impact**: Low. Each HashSet is small (bounded by entry count) and intersection is fast.
- **Recommendation**: Use `retain()` instead of `intersection().collect()` to avoid allocation:
  ```rust
  result_ids.retain(|id| set.contains(id));
  ```

### 3.3 Vector Embedding Copies

**Finding MA-03: Unnecessary embedding copy in vector insert**
- **File**: `crates/unimatrix-vector/src/index.rs:155`
- **Description**: `embedding.to_vec()` copies the 384-element f32 slice (1.5KB) because `hnsw_rs::insert_slice` takes `(&[f32], usize)` but the original slice reference would be valid for the call.
- **Closer inspection**: Actually, `insert_slice` takes `(&Vec<f32>, usize)`, requiring owned data. The copy is **necessary** due to hnsw_rs API requiring `&Vec<f32>`.
- **Recommendation**: No action. The copy is required by the dependency's API.

**Finding MA-04: l2_normalized allocates new Vec**
- **File**: `crates/unimatrix-embed/src/normalize.rs:21-25`
- **Description**: `l2_normalized()` calls `embedding.to_vec()` then normalizes in place. This allocates 384 * 4 = 1.5KB.
- **Current usage**: Appears to be used sparingly (search path uses `l2_normalize` in-place). The allocating version is used where the original must be preserved.
- **Recommendation**: No action -- usage pattern is correct. The in-place version is used on the hot path.

### 3.4 Box::leak in Persistence

**Finding MA-05: Intentional memory leak in vector persistence load**
- **File**: `crates/unimatrix-vector/src/persistence.rs:130`
- **Description**: `Box::leak(Box::new(hnswio::HnswIo::new(dir, &basename)))` leaks the HnswIo struct to satisfy hnsw_rs's lifetime constraint that requires `'static` for the loaded Hnsw. The code comments explain this is intentional and the leaked memory is small (paths + metadata only, ~100-500 bytes).
- **Impact**: Each `VectorIndex::load()` leaks ~100-500 bytes. This is called at server startup and during compaction (once per maintenance cycle).
- **Recommendation**: Acceptable. Cumulative leak is negligible (< 10KB even after 20 compaction cycles). Could be addressed by using `ManuallyDrop` with careful lifecycle management, but the complexity is not justified.

---

## 4. I/O Optimization Opportunities

### 4.1 Transaction Granularity

**Finding IO-01: Per-result read transactions in search path**
- **File**: `crates/unimatrix-store/src/read.rs:113-119` (via async wrapper)
- **Description**: Each `Store::get(entry_id)` opens its own read transaction via `self.db.begin_read()`. In the search path, this means N separate read transactions for N results.
- **Impact**: redb read transactions are lightweight (they just snapshot the B-tree root), so overhead is ~1-5us per transaction. For k=10, this is 10-50us total.
- **Recommendation**: Batch read within a single transaction. Add `get_batch(ids: &[u64]) -> Vec<EntryRecord>` that opens one transaction and reads all entries.

**Finding IO-02: Co-access statistics iterate entire table**
- **File**: `crates/unimatrix-store/src/read.rs:260-278`
- **Description**: `co_access_stats` and `top_co_access_pairs` both iterate the entire CO_ACCESS table, deserializing every record. `top_co_access_pairs` additionally sorts and truncates.
- **Impact**: Called from `context_status` tool, which is invoked for health checks and maintenance. Not in the search hot path. Acceptable.
- **Recommendation**: No immediate action. If CO_ACCESS grows large, maintain a cached count.

### 4.2 Signal Queue

**Finding IO-03: Signal queue cap enforcement iterates to find oldest**
- **File**: `crates/unimatrix-store/src/db.rs` (insert_signal)
- **Description**: When the signal queue exceeds capacity, the code iterates from the beginning to find and delete the oldest entries. This is O(n) but bounded by the queue cap.
- **Impact**: Queue cap is small (bounded). Not a concern.

### 4.3 Event Queue File I/O

**Finding IO-04: EventQueue reads entire file to check line count**
- **File**: `crates/unimatrix-engine/src/event_queue.rs:192-198`
- **Description**: `find_or_create_target` reads the entire latest queue file with `fs::read_to_string(latest)` just to count non-empty lines and check if rotation is needed.
- **Impact**: MAX_EVENTS_PER_FILE = 1000 lines. At ~100 bytes per JSON line, this reads ~100KB to check if rotation is needed.
- **Recommendation**: Track line count in the filename or a sidecar file, or seek to end of file and count newlines backwards.

---

## 5. Caching Recommendations

### 5.1 Missing Caches

**Finding CA-01: No embedding cache**
- **Severity**: Medium
- **Description**: There is no cache for embeddings. If the same text is embedded multiple times (e.g., near-duplicate detection in `context_store` followed by the actual insert embedding), the ONNX inference runs twice.
- **Current mitigating factor**: `context_store` in `tools.rs` reuses the embedding from near-duplicate detection for the actual vector insert. So the most obvious double-embed is already avoided.
- **Remaining cases**: Briefing tool computes embeddings for role+task queries that may be repeated by the same agent in the same session. UDS injection embeds may repeat for similar hook payloads.
- **Recommendation**: Add an LRU cache (size ~100-200 entries) keyed by content hash. This avoids the most expensive operation (ONNX inference) for repeated queries.

**Finding CA-02: No confidence cache**
- **Description**: `compute_confidence` is called in the search re-ranking path for each result. The function is pure and fast (~50ns), but it reads `entry.confidence` which is already stored in the EntryRecord.
- **Current state**: Looking more carefully, `entry.confidence` is the pre-stored confidence value. The `rerank_score` function uses `entry.confidence` directly, not `compute_confidence`. So there is no redundant computation here.
- **Status**: Not an issue. Confidence is pre-computed on write and stored in the EntryRecord.

### 5.2 Existing Caches (Working Well)

- **ContentScanner**: `OnceLock` singleton compiles regex patterns once. Good.
- **UsageDedup**: Deduplicates access recording within a time window. Good.
- **PendingEntriesAnalysis**: Caches analysis results with 1000-entry cap. Good.

---

## 6. Dependency Analysis

### 6.1 Dependency Weight Audit

| Crate | Key Dependencies | Weight Concern |
|-------|-----------------|----------------|
| unimatrix-store | redb 3.1, bincode 2, sha2, serde | Minimal. redb is lean. |
| unimatrix-vector | hnsw_rs 0.3 (simdeez_f), anndists | **simdeez_f** enables SIMD. Good for perf, adds compile complexity. |
| unimatrix-embed | ort 2.0.0-rc.9, tokenizers (onig), hf-hub | **Heavy**. See below. |
| unimatrix-core | Thin bridge. Optional tokio for async. | Minimal. |
| unimatrix-server | rmcp 0.16, tokio (full), schemars 1, regex | **schemars** for JSON Schema generation. tokio "full" includes all features. |
| unimatrix-engine | serde_json, sha2, dirs, nix | Moderate. nix pulls in libc. |
| unimatrix-observe | serde, bincode, serde_json | Minimal. No heavy deps. |
| unimatrix-adapt | ndarray, rand, rand_distr | Moderate. ndarray is medium-weight. |

**Finding DA-01: tokenizers with `onig` feature**
- **File**: `crates/unimatrix-embed/Cargo.toml`
- **Description**: The `tokenizers` crate is pulled with the `onig` feature enabled, which brings in the Oniguruma regex engine as a C dependency. This adds:
  - Compile time: ~30-60s for onig C compilation
  - Binary size: ~500KB additional
  - Build dependency: requires C compiler and cmake
- **Recommendation**: Investigate if `onig` can be replaced with the default regex backend. The `onig` feature is needed for some tokenizer patterns in certain HuggingFace models, but all-MiniLM-L6-v2 may not require it. Test with `default-features = false, features = ["progressbar"]` or similar minimal feature set.

**Finding DA-02: tokio "full" feature**
- **File**: `crates/unimatrix-server/Cargo.toml`
- **Description**: `tokio = { version = "1", features = ["full"] }` includes all tokio features: rt-multi-thread, io-util, io-std, net, time, process, signal, sync, macros, fs. The server uses rt, macros, sync (spawn_blocking), and net (for UDS). Features like `process`, `fs`, and `signal` may be unused.
- **Impact**: tokio features mostly affect compile time, not runtime. The unused features add ~5-10s to compile time.
- **Recommendation**: Low priority. Replace `"full"` with explicit features: `["rt-multi-thread", "macros", "sync", "net", "io-util", "io-std", "time"]`.

**Finding DA-03: schemars for JSON Schema generation**
- **File**: `crates/unimatrix-server/Cargo.toml`
- **Description**: `schemars = "1"` is used for MCP tool parameter schema generation. It pulls in serde_json and a schema generation framework.
- **Impact**: Required by rmcp for MCP tool definitions. Cannot be removed.
- **Recommendation**: No action. This is a hard requirement.

**Finding DA-04: ort ONNX Runtime pre-release**
- **File**: `crates/unimatrix-embed/Cargo.toml`
- **Description**: `ort = "=2.0.0-rc.9"` pins an exact pre-release version. This is appropriate for stability but means the crate won't automatically get performance improvements from newer ort releases.
- **Impact**: ort 2.0 rc.9 -> stable may include performance improvements. The pinned version ensures reproducibility.
- **Recommendation**: Track ort 2.0 stable release and update when available. The pinning is correct for now.

### 6.2 Workspace Patch

**Finding DA-05: anndists edition 2024 patch**
- **File**: `/workspaces/unimatrix/Cargo.toml` (workspace root)
- **Description**: The workspace patches `anndists` to a git fork for edition 2024 compatibility. This is a build-time concern, not a runtime performance issue.
- **Recommendation**: Monitor upstream anndists for edition 2024 support to remove the patch.

---

## 7. Additional Findings

### 7.1 Validate Embedding on Every Operation

**Finding AF-01: validate_embedding iterates all 384 elements**
- **File**: `crates/unimatrix-vector/src/index.rs` (validate_embedding method)
- **Description**: Both `insert` and `search` call `validate_embedding`, which iterates all 384 f32 elements checking for NaN/Inf. This is ~384 comparisons per call.
- **Impact**: ~100ns per call. Negligible compared to HNSW search (~1ms) or insert.
- **Recommendation**: No action. Defensive validation is worth the negligible cost.

### 7.2 String Cloning in UsageDedup

**Finding AF-02: String allocation in usage dedup filter**
- **File**: `crates/unimatrix-server/src/usage_dedup.rs`
- **Description**: `filter_access` creates `(agent_id.to_string(), id)` tuples for HashSet membership checks. This clones the agent_id string on every call.
- **Impact**: Low. Agent IDs are short strings (~10-20 bytes). Called once per tool response.
- **Recommendation**: Use `Cow<str>` or intern agent IDs to avoid repeated allocation.

### 7.3 Observation Parser Performance

**Finding AF-03: Observation parser is well-optimized**
- **File**: `crates/unimatrix-observe/src/parser.rs`
- **Description**: The JSONL parser uses `BufReader`, hand-rolled timestamp parsing (no chrono dependency), and graceful skip of malformed lines. The test at line 377 verifies 10K records parse in < 2 seconds.
- **Impact**: Positive finding. No optimization needed.

### 7.4 Confidence Computation is Pure and Fast

**Finding AF-04: Confidence pipeline is well-designed**
- **File**: `crates/unimatrix-engine/src/confidence.rs`
- **Description**: All confidence functions are pure (no I/O, no allocation), operating on primitive types (f64). The composite computation is a weighted sum of 6 function calls, each doing simple arithmetic. Total: ~50ns per call.
- **Impact**: Positive finding. No optimization needed.

---

## 8. Priority Matrix

| ID | Finding | Impact | Effort | Priority |
|----|---------|--------|--------|----------|
| HP-04 | Co-access full table scan (Scan 2) | High | Medium | **P1** |
| HP-05 | ONNX Mutex serialization | High (at scale) | Medium | **P1** |
| CA-01 | No embedding cache | Medium | Low | **P2** |
| HP-01 | Double sort in context_search | Medium | Low | **P2** |
| HP-02 | Per-result entry fetch (no batch) | Medium | Low | **P2** |
| HP-03 | Co-access queries one anchor at a time | Medium | Low | **P2** |
| CC-02 | spawn_blocking for trivial ops | Low-Medium | Low | **P3** |
| MA-02 | HashSet intersection allocation | Low | Low | **P3** |
| HP-06 | Vec allocations in embed pipeline | Low | Medium | **P3** |
| DA-01 | tokenizers onig feature | Low (build) | Low | **P3** |
| DA-02 | tokio "full" feature | Low (build) | Low | **P4** |
| HP-07 | Content hash intermediate allocation | Low | Low | **P4** |
| IO-04 | EventQueue reads file for line count | Low | Low | **P4** |
| AF-02 | String cloning in UsageDedup | Low | Low | **P4** |
| MA-05 | Box::leak in persistence | Negligible | High | **Skip** |

### Priority Definitions

- **P1**: Address before scaling beyond single-user. These are the limiting factors for concurrent agent scenarios.
- **P2**: Quick wins. Low effort improvements that reduce latency on the primary search path.
- **P3**: Cleanup. Reduce allocation pressure and build overhead. Worth doing in a maintenance pass.
- **P4**: Marginal. Only address if touching the affected code for other reasons.
- **Skip**: Acceptable trade-off. Cost of fix exceeds benefit.

---

## 9. Scaling Projections

At current scale (~200 entries, 368 co-access pairs, single-user):
- Search latency is dominated by ONNX embedding (~10ms) + HNSW search (~1ms) + redb reads (~0.5ms)
- All identified issues add < 2ms total overhead
- Memory usage is well-bounded by design

At projected scale (~10K entries, ~50K co-access pairs, 5 concurrent agents):
- HP-04 (full table scan) becomes ~50ms per search (3 anchors * 50K rows)
- HP-05 (ONNX Mutex) becomes the throughput bottleneck: 5 agents * 10ms embed = 50ms queue delay
- HP-02 (per-result fetch) remains acceptable due to redb's fast reads
- Total search latency could exceed 100ms without P1 fixes

---

## Appendix: File Reference

| Finding | Primary File | Line(s) |
|---------|-------------|---------|
| HP-01 | `crates/unimatrix-server/src/tools.rs` | 354-399 |
| HP-02 | `crates/unimatrix-server/src/tools.rs` | 342 |
| HP-03 | `crates/unimatrix-engine/src/coaccess.rs` | 114-121 |
| HP-04 | `crates/unimatrix-store/src/read.rs` | 243-253 |
| HP-05 | `crates/unimatrix-embed/src/onnx.rs` | 22 |
| HP-06 | `crates/unimatrix-embed/src/onnx.rs` | 91-102, 129 |
| HP-07 | `crates/unimatrix-store/src/hash.rs` | (all) |
| CC-01 | `crates/unimatrix-embed/src/onnx.rs` | 22 |
| CC-02 | `crates/unimatrix-core/src/async_wrappers.rs` | (all) |
| MA-01 | `crates/unimatrix-store/src/schema.rs` | EntryRecord struct |
| MA-02 | `crates/unimatrix-store/src/query.rs` | 64-67 |
| MA-03 | `crates/unimatrix-vector/src/index.rs` | 155 |
| MA-04 | `crates/unimatrix-embed/src/normalize.rs` | 21-25 |
| MA-05 | `crates/unimatrix-vector/src/persistence.rs` | 130 |
| IO-01 | `crates/unimatrix-store/src/read.rs` | 113-119 |
| IO-02 | `crates/unimatrix-store/src/read.rs` | 260-278 |
| IO-04 | `crates/unimatrix-engine/src/event_queue.rs` | 192-198 |
| CA-01 | (missing infrastructure) | N/A |
| DA-01 | `crates/unimatrix-embed/Cargo.toml` | tokenizers dep |
| DA-02 | `crates/unimatrix-server/Cargo.toml` | tokio dep |
| AF-02 | `crates/unimatrix-server/src/usage_dedup.rs` | filter_access |
