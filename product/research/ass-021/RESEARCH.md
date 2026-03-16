# ASS-021: context_briefing & Injection Pipeline Analysis

**Date:** 2026-03-15
**Status:** Research complete

---

## 1. context_briefing MCP Tool

### 1.1 Tool Input Parameters

Struct `BriefingParams` — `crates/unimatrix-server/src/mcp/tools.rs:204-222`

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `role` | `String` | required | Convention lookup topic filter |
| `task` | `String` | required | Semantic search query (embedded) |
| `feature` | `Option<String>` | None | Tag to boost feature-relevant entries |
| `max_tokens` | `Option<i64>` | 3000 | Budget, clamped [500, 10000] |
| `agent_id` | `Option<String>` | None | Caller identity for capability check |
| `format` | `Option<String>` | "markdown" | summary / markdown / json |
| `helpful` | `Option<bool>` | None | Feedback signal (confidence training) |
| `session_id` | `Option<String>` | None | Audit context — hooks provide this |

### 1.2 Handler Execution Path

`crates/unimatrix-server/src/mcp/tools.rs:858-927`

1. Identity check → `Capability::Read` required
2. `validate_briefing_params()` + `validated_max_tokens()`
3. `BriefingService::assemble()` with parsed params
4. Fire-and-forget audit emission (S5) — all entry IDs
5. Fire-and-forget usage recording → confidence scoring
6. `format_briefing()` → `CallToolResult` (markdown/JSON)

---

## 2. BriefingService Assembly Pipeline

**File:** `crates/unimatrix-server/src/services/briefing.rs`

### 2.1 BriefingParams (internal)

| Field | Type | Notes |
|-------|------|-------|
| `role` | `Option<String>` | Topic filter for convention lookup |
| `task` | `Option<String>` | Embedding query for semantic search |
| `feature` | `Option<String>` | Feature tag boost |
| `max_tokens` | `usize` | Token budget |
| `include_conventions` | `bool` | Whether to query store for conventions |
| `include_semantic` | `bool` | Whether to run embedding + HNSW search |
| `injection_history` | `Option<Vec<InjectionEntry>>` | UDS CompactPayload path |

### 2.2 Assembly Steps

**Step 1:** Snapshot effectiveness categories (crt-018b)
- Short read lock on `EffectivenessState` → read `generation` → drop guard
- Acquire `Mutex<EffectivenessSnapshot>` → generation-aware clone
- Lock ordering: EffectivenessState THEN snapshot mutex (never both simultaneously)

**Step 2:** Init char budget
- `char_budget = max_tokens * 4` (~4 chars/token)

**Step 3:** Injection history path (if `injection_history` is `Some`)
- `process_injection_history()` → `InjectionSections` + `chars_used`
- Partition by: decisions (`category="decision"`), injections (other), conventions (`category="convention"`)
- Sort: **confidence DESC**, then **effectiveness_priority DESC** (AC-07, R-09)
- Deduct `chars_used` from budget

**Step 4:** Convention lookup (if `include_conventions && role.is_some()`)
- Store query: `topic=role`, `category="convention"`, `status=Active`
- Exclude quarantined entries (S4 defense-in-depth)
- Sort order:
  - With `feature` tag: feature-tagged entries first, then confidence DESC + effectiveness_priority DESC (AC-08)
  - Without `feature`: confidence DESC, then effectiveness_priority DESC
- Linear fill while `budget_remaining >= entry_chars`

**Step 5:** Semantic search (if `include_semantic && task.is_some()`)
- Rate check (S2) via `SecurityGateway`
- Build `ServiceSearchParams`:
  - `k = semantic_k` (env `UNIMATRIX_BRIEFING_K`, default 3, clamped [1,20])
  - `similarity_floor = None` (briefing does not enforce)
  - `co_access_anchors = convention + injection entry IDs` (cross-boost)
  - `retrieval_mode = Flexible` (penalizes deprecated/superseded, doesn't exclude)
- Call `SearchService::search()`
- On `EmbeddingFailed(_)`: set `search_available=false`, graceful degrade
- Budget-fill results like conventions

**Step 6:** Dedup entry IDs

**Step 7:** Audit emit (S5)

**Step 8:** Return `BriefingResult`

### 2.3 BriefingResult

```rust
BriefingResult {
    conventions: Vec<EntryRecord>,
    relevant_context: Vec<(EntryRecord, f64)>,   // (entry, similarity_score)
    injection_sections: InjectionSections,        // partitioned injection history
    entry_ids: Vec<u64>,                          // deduped all entries
    search_available: bool,
}
```

---

## 3. Injection Pipeline (UDS)

The injection pipeline is the **agent-facing path** — triggered by Claude Code hooks via the Unix domain socket, not the MCP server.

### 3.1 ContextSearch (UserPromptSubmit Hook)

**File:** `crates/unimatrix-server/src/uds/listener.rs:931-1120`

Triggered when an agent submits a prompt. This is the **write path** — it performs a fresh search and records what was injected.

1. Extract topic signal from query text (`col-018`)
2. Record `UserPromptSubmit` observation (col-018)
3. `SearchService::search()` with:
   - `retrieval_mode = Strict` (hard filter: Active only, no superseded)
   - `similarity_floor = 0.5`
   - `confidence_floor = 0.3`
   - `k = 5` (`INJECTION_K`)
4. Store `(session_id, entry_id, reranked_confidence)` in `SessionRegistry.injection_history`
5. Persist to `INJECTION_LOG` table (fire-and-forget)
6. Persist to `query_log` table
7. Record co-access pairs: top-3 anchors × all results (dedup by session_id)
8. Return entries as `HookResponse::Entries`

### 3.2 CompactPayload (Prompt Construction Hook)

**File:** `crates/unimatrix-server/src/uds/listener.rs:1126-1330`

Triggered when building the agent prompt. This is the **read path** — it assembles context from what was previously injected.

**Path A — Injection history exists:**
```
BriefingParams {
    role:                 session.role or override,
    feature:              session.feature or override,
    task:                 None,          // NO embedding
    include_conventions:  false,         // skip store lookup
    include_semantic:     false,         // NO vector search
    injection_history:    Some(history), // re-inject prior entries
}
```
→ `BriefingService::assemble()` → `InjectionSections`

**Path B — No injection history (fallback):**
```
BriefingParams {
    include_conventions: true,
    include_semantic:    false,
}
```
→ conventions only as baseline context

**Output Format** (section priority and byte budgets, ADR-003):

| Section | Budget (bytes) | ~Tokens |
|---------|---------------|---------|
| Decisions | 1600 | ~400 |
| Key Context (injections) | 2400 | ~600 |
| Conventions | 1600 | ~400 |
| Metadata/context | 800 | ~200 |

Format per entry: markdown with title, content, confidence%, entry ID

### 3.3 Injection Log Storage

**File:** `crates/unimatrix-store/src/injection_log.rs`

```rust
InjectionLogRecord {
    log_id:     u64,    // monotonic
    session_id: String,
    entry_id:   u64,
    confidence: f64,    // reranked score at injection time
    timestamp:  u64,    // Unix epoch seconds
}
```

Batch write API: `insert_injection_log_batch()` — atomic transaction, contiguous `log_id` range.
Scan API: `scan_injection_log_by_sessions()` — chunked IN clauses (50 per chunk, R-11).

---

## 4. SearchService Full Pipeline

**File:** `crates/unimatrix-server/src/services/search.rs`

### 4.1 ServiceSearchParams

```rust
ServiceSearchParams {
    query:             String,
    k:                 usize,
    filters:           Option<QueryFilter>,      // topic, category, tags, status
    similarity_floor:  Option<f64>,
    confidence_floor:  Option<f64>,
    feature_tag:       Option<String>,           // feature boost (not currently used)
    co_access_anchors: Option<Vec<u64>>,         // cross-boost from other entries
    caller_agent_id:   Option<String>,           // rate limiting key
    retrieval_mode:    RetrievalMode,            // Strict | Flexible
}
```

**RetrievalMode:**
- `Strict` — hard filter: Active + non-superseded only (UDS injection path)
- `Flexible` — soft penalty: penalize deprecated/superseded but include (MCP briefing path)

### 4.2 Pipeline Steps

0. Rate limit check (S2)
1. Query validation (S1 + S3)
2. Get embedding adapter (async)
3. Embed query via `spawn_blocking` with timeout (MCP_HANDLER_TIMEOUT, fix #277)
4. Adapt embedding via MicroLoRA + L2 normalize
5. HNSW search: `search_filtered()` if filters, else `search()` with EF_SEARCH=32
   → `Vec<(entry_id, similarity)>`
6. Fetch entries + quarantine filter (S4)
6a. Status filter/penalty (crt-010):
   - Read supersession graph from `SupersessionStateHandle` (cache, ADR fix #264)
   - **Strict:** drop non-Active + superseded
   - **Flexible:** build `penalty_map`: superseded/deprecated → `graph_penalty()` or `FALLBACK_PENALTY` ∈ [0.0, 1.0]
6b. Supersession injection (crt-010, crt-014):
   - For each superseded entry: find terminal active node via multi-hop graph traversal
   - Fetch terminal entry, compute cosine similarity, inject into candidates
7. Re-rank sort key:
   ```
   (rerank_score + utility_delta + provenance_boost) * status_penalty
   ```
   - `rerank_score = 0.85 * similarity + 0.15 * confidence`
   - `utility_delta`: Effective=+0.10, Settled=+0.05, Ineffective/Noisy=-0.20, else=0.0
   - `provenance_boost = 0.05` for `category="lesson-learned"`
   - `status_penalty = 1.0` (Active) or `graph_penalty` (deprecated/superseded)
8. Co-access boost (if 2+ results):
   - Top-3 anchors × all results
   - Exclude deprecated from boost (crt-010, C3)
   - `compute_search_boost()` via spawn_blocking, staleness cutoff = 24h
   - Max boost: 0.03
   - Re-sort with co-access boost applied
9. Truncate to `k`
10. Apply `similarity_floor`, `confidence_floor`
11. Audit + return

---

## 5. Behavioral Comparison: context_briefing vs Injection Pipeline

| Dimension | context_briefing (MCP) | ContextSearch (UDS hook) |
|-----------|----------------------|--------------------------|
| Transport | MCP stdio | Unix domain socket |
| Caller | Claude agent (explicit tool call) | Hook (automatic on user prompt) |
| Retrieval mode | Flexible (includes deprecated with penalty) | Strict (Active only) |
| k | 3 (env: `UNIMATRIX_BRIEFING_K`) | 5 (`INJECTION_K`) |
| Similarity floor | None | 0.5 |
| Confidence floor | None | 0.3 |
| Convention path | Yes (`include_conventions=true`) | No |
| Injection history | Yes (CompactPayload re-injection path) | No (writes history) |
| Co-access anchors | Convention + injection IDs (cross-boost) | Top-3 results |
| Writes injection log | No | Yes |
| Audit | Yes (S5) | Via injection log |

---

## 6. Key Observations / Potential Rework Areas

### O1: context_briefing does not write injection history
`context_briefing` (MCP) queries knowledge but never writes to `INJECTION_LOG`. This means:
- Results from explicit `context_briefing` calls are invisible to `CompactPayload`
- CompactPayload only knows about what came through `ContextSearch` (hook path)
- If an agent calls `context_briefing` directly, the injected knowledge is not tracked

### O2: k=3 is small and not floor-filtered
Briefing uses `k=3` with no similarity/confidence floors. SearchService Flexible mode can return low-confidence results. The injection path uses `k=5` with hard floors.

### O3: Two separate embedding triggers
- `context_briefing`: embeds `task` field on every call (MCP path, user-triggered)
- `ContextSearch`: embeds the full prompt on every UserPromptSubmit (hook path, auto)
- These are parallel and independent — no deduplication

### O4: CompactPayload disables semantic search entirely
On the CompactPayload path, `include_semantic=false` always. This means the prompt construction step never re-ranks by current relevance — it only re-injects the historical snapshot. Freshness is bounded by the last `ContextSearch` call.

### O5: Convention lookup in briefing uses `topic=role` filter
The Store query for conventions filters by `topic = role`. If entries don't have matching `topic` metadata, they won't appear regardless of semantic relevance.

### O6: feature_tag in SearchService not wired in briefing
`ServiceSearchParams.feature_tag` exists but the comment in briefing.rs says "not yet used in briefing path." Feature boosting in briefing happens at sort-time (convention sort step), not inside SearchService.

---

## 7. File Map

| File | Purpose |
|------|---------|
| `crates/unimatrix-server/src/mcp/tools.rs:841-928` | context_briefing MCP handler |
| `crates/unimatrix-server/src/mcp/response/briefing.rs` | Format BriefingResult → CallToolResult |
| `crates/unimatrix-server/src/services/briefing.rs` | BriefingService::assemble() |
| `crates/unimatrix-server/src/services/search.rs` | SearchService::search() full pipeline |
| `crates/unimatrix-server/src/uds/listener.rs:931-1120` | UDS ContextSearch handler |
| `crates/unimatrix-server/src/uds/listener.rs:1126-1330` | UDS CompactPayload handler |
| `crates/unimatrix-store/src/injection_log.rs` | InjectionLogRecord persistence |
