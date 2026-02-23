# vnc-003: v0.2 Tool Implementations

## Problem Statement

The MCP server has four working v0.1 tools (context_search, context_lookup, context_store, context_get) but no way to manage knowledge lifecycle. Agents cannot correct wrong entries, deprecate obsolete knowledge, check knowledge base health, or receive compiled briefings. Without correction chains, a wrong entry stays active and propagates to every agent that retrieves it. Without deprecation, abandoned patterns remain discoverable. Without health metrics, no one knows the knowledge base is degrading. Without briefings, every agent must compose multiple tool calls to orient itself -- wasting context window and increasing latency.

Additionally, GH issue #14 identified that VECTOR_MAP writes happen outside the combined audit transaction in `insert_with_audit`, creating a crash-safety gap where entries can exist without vector mappings. This must be fixed as prerequisite infrastructure since `context_correct` also inserts entries with embeddings.

GH issue #11 (audit write transaction optimization) was partially addressed by vnc-002's combined transaction pattern for `context_store`. vnc-003 adds two more mutating tools (`context_correct`, `context_deprecate`), which will use the same combined-transaction pattern. Closing #11 depends on confirming the pattern suffices without batching/async channels.

## Goals

1. **Implement `context_correct`** -- supersede an entry with a corrected version, preserving the original as deprecated with a bidirectional correction chain (`supersedes`/`superseded_by`). Apply content scanning and category validation to the new content. Re-embed and index the correction.

2. **Implement `context_deprecate`** -- mark an entry as no longer relevant without replacement. Transition status to `Deprecated`. Record the reason in an audit event.

3. **Implement `context_status`** -- compute and return knowledge base health metrics: entry counts by status, by category, by topic, correction chain counts, and security metrics (entries by trust_source, entries without attribution).

4. **Implement `context_briefing`** -- compile an orientation briefing for an agent about to start work. Internally execute lookup(topic: role, category: "convention") + lookup(topic: role, category: "duties") + search(query: task, k: 3). Assemble into a single response under a configurable token budget.

5. **Fix GH #14** -- move VECTOR_MAP write into the combined write transaction so entry insert + vector mapping + audit event are fully atomic. Requires refactoring `VectorIndex::insert()` to support writing the VECTOR_MAP in an external transaction.

6. **Resolve GH #11** -- confirm the combined-transaction pattern used by `context_store` (and now `context_correct`) is sufficient. Close the issue if no throughput problems are measured.

## Non-Goals

- **No v0.3 tools.** MCP Resources, Prompts, and local embeddings are future milestones.
- **No confidence computation.** The `confidence` field exists but the formula (usage x freshness x correction x helpfulness) is crt-002. `context_correct` increments `correction_count` but does not recompute confidence.
- **No usage tracking.** Access counting is crt-001. `context_briefing` does not log which entries it assembled.
- **No "force store" parameter.** Near-duplicate override on `context_store` is deferred.
- **No contradiction detection.** Detecting conflicting entries with high similarity is crt-003. `context_status` does not scan for duplicates or semantic conflicts.
- **No staleness metrics.** Time-based staleness is a poor proxy -- a repo may simply be inactive. Utilization-based staleness depends on crt-001 (usage tracking) populating `access_count` and `last_accessed_at`. `context_status` does not report stale entries.
- **No content_hash re-validation.** Re-hashing all entries to detect tampering/corruption is a separate audit concern, deferred to a future explicit audit command.
- **No inline duplicate scanning.** Pairwise similarity scans in `context_status` are O(n^2) and deferred to a background process or future feature.
- **No persisted category allowlist.** Categories remain runtime-only (`RwLock<HashSet>`). Persistence to redb is a future enhancement.
- **No HTTP/SSE transport.** Stdio only.
- **No cross-project scope.** Single project per server instance.
- **No batch operations.** Each tool call operates on a single entry or query.
- **No token counting.** `context_briefing` uses character-based budget estimation, not actual tokenizer. Real token counting requires a tokenizer dependency not justified yet.

## Background Research

### Existing Codebase Patterns

**EntryRecord correction fields (pre-seeded in nxs-001):** `supersedes: Option<u64>`, `superseded_by: Option<u64>`, `correction_count: u32`. These fields exist on every EntryRecord but are never written by vnc-002. `context_correct` will be the first tool to use them, establishing the correction chain pattern.

**Store update API:** `Store::update(EntryRecord)` performs an atomic read-diff-write of all indexes. It auto-computes `content_hash`, `previous_hash`, and `version` increment. `Store::update_status(id, Status)` is a lightweight status-only transition that does NOT bump version or hash. Both are available through `AsyncEntryStore` wrappers.

**Combined transaction pattern (ADR-001):** `UnimatrixServer::insert_with_audit()` opens a single redb `WriteTransaction`, writes the entry + all indexes + audit event, then commits. After commit, it inserts the embedding into the HNSW index (separate data structure). This pattern will be reused for `context_correct` (which both inserts a new entry and updates the original).

**VECTOR_MAP crash gap (GH #14):** `VectorIndex::insert()` writes the VECTOR_MAP in its own `store.put_vector_mapping()` call -- a separate write transaction from `insert_with_audit`. Moving this write into the combined transaction requires either: (a) `VectorIndex` accepting an external `WriteTransaction`, or (b) the server writing VECTOR_MAP directly (bypassing `VectorIndex` for the mapping write, while still calling `VectorIndex` for the HNSW insert).

**Category allowlist:** `CategoryAllowlist` with `add_category()` is already runtime-extensible. vnc-003 may add new categories (e.g., "reference", "duties") if `context_briefing` lookups need them, or rely on agents using existing categories.

**Response formatting:** `response.rs` provides `format_single_entry`, `format_search_results`, `format_lookup_results`, `format_store_success`, `format_duplicate_found`, `format_empty_results`. New formatters needed for: `context_correct` (correction success), `context_deprecate` (deprecation confirmation), `context_status` (health metrics), `context_briefing` (assembled briefing).

**Security enforcement:** Capability checks, input validation, content scanning, and category validation are established patterns in `tools.rs`. The execution order (identity -> capability -> validation -> category -> scanning -> business logic -> format -> audit) is consistent across all v0.1 tools and will be extended to v0.2 tools.

**Audit transaction patterns:** Read-only tools use standalone `audit.log_event()`. Mutating tools use `audit.write_in_txn()` inside a combined transaction. Both patterns are proven in vnc-002.

### VECTOR_MAP Fix Analysis (GH #14)

The cleanest approach is option (b): have `insert_with_audit` write the VECTOR_MAP entry directly in the combined transaction, since it already has access to `Arc<Store>` and the `WriteTransaction`. The HNSW in-memory insert still happens after commit (it's not a redb operation). This avoids modifying `VectorIndex`'s public API while ensuring atomicity.

Specifically: after generating the entry ID and before committing, the combined transaction should call `txn.open_table(VECTOR_MAP)?.insert(entry_id, data_id)` where `data_id` comes from `VectorIndex::next_data_id()` (which already exists as an atomic counter). Then, after commit, call `VectorIndex::insert_hnsw_only()` or similar to insert only into the HNSW data structure (skipping the now-redundant VECTOR_MAP write).

This requires adding a method to `VectorIndex` that inserts into HNSW only (without the VECTOR_MAP write), and exposing the `next_data_id` allocation.

### context_correct Design

`context_correct` performs a two-entry operation atomically:
1. **Deprecate the original:** Set `status = Deprecated`, `superseded_by = new_id`, increment `correction_count`.
2. **Insert the correction:** Create a new entry with `supersedes = original_id`, inheriting topic/category/tags from the original unless overridden.

Both operations should happen in a single write transaction for consistency. This extends the `insert_with_audit` pattern to also include an update of the original entry.

The correction also needs embedding and vector indexing for the new entry. Content scanning applies to the new content (same as `context_store`).

### context_deprecate Design

Simple status transition: `update_status(id, Status::Deprecated)`. This is the lightest mutating operation -- one read + one write transaction. The existing `Store::update_status()` method handles STATUS_INDEX migration and counter updates atomically.

Since `update_status` does NOT bump version or hash (per nxs-004 design), deprecation is a metadata-only change. This is intentional: deprecation reflects relevance, not content correctness.

### context_status Design

Read-only aggregation over the store. Queries:
- `read_counter("total_active")`, `read_counter("total_deprecated")`, `read_counter("total_proposed")` for status counts.
- Category/topic distribution: iterate CATEGORY_INDEX/TOPIC_INDEX range scans.
- Correction chain metrics: count entries with non-None `supersedes` or `superseded_by`, total `correction_count` sum.
- Security metrics: scan ENTRIES for trust_source distribution, count of entries with empty `created_by`.

Explicitly excluded from this scope: stale entry detection (time-based staleness is a poor proxy -- utilization metrics depend on crt-001), duplicate candidate scanning (O(n^2), deferred to background), and content_hash re-validation (separate audit concern).

Performance concern: full scans are acceptable at Unimatrix's expected scale (hundreds to low thousands of entries per project). If performance becomes an issue, counters can be pre-computed incrementally.

### context_briefing Design

Composite read operation that internally calls existing infrastructure:
1. `entry_store.query(QueryFilter { topic: role, category: "convention", status: Active })` -- role conventions
2. `entry_store.query(QueryFilter { topic: role, category: "duties", status: Active })` -- role duties
3. `vector_store.search(embed(task), k=3, ef_search)` -- task-relevant entries
4. When `feature` is provided, boost/prioritize entries whose tags include the feature ID in the search results. This is a scoring adjustment on results already returned -- no extra queries or filtering needed.

Assemble results into a structured briefing:
```
## Conventions
{role conventions as bullet points}

## Duties
{role duties as bullet points}

## Relevant Context
{task-relevant entries with titles and excerpts}
```

Token budget: `max_tokens` parameter (default 3000) controls total output size. Use character count as a proxy (~4 chars per token). Truncate from least-relevant entries first.

The "duties" category does not exist in the initial allowlist. Either: (a) add it via `categories.add_category("duties")` at server startup, or (b) add "duties" and "reference" to the initial allowlist in `categories.rs`. Option (b) is cleaner -- it ensures the categories are always available.

### Capability Requirements

Per the product vision's security table for vnc-003:
- `context_correct`: `Write` capability (creates new entry + modifies original)
- `context_deprecate`: `Write` capability (modifies entry status)
- `context_status`: `Admin` capability (exposes knowledge base internals)
- `context_briefing`: `Read` capability (read-only composite query)

### Test Infrastructure

506 tests across 5 crates (117 store + 85 vector + 76 embed + 21 core + 207 server). Test infrastructure is cumulative -- build on existing `make_server()` test helper in `server.rs::tests`, existing `TestDb`/`TestEntry` builders in `unimatrix-store`, and existing patterns for async tool handler testing.

## Proposed Approach

### Module Changes

**`crates/unimatrix-server/src/tools.rs`** -- Add 4 new tool handlers (`context_correct`, `context_deprecate`, `context_status`, `context_briefing`) following the same `#[tool]` macro pattern and execution order as v0.1 tools. Add param structs (`CorrectParams`, `DeprecateParams`, `StatusParams`, `BriefingParams`).

**`crates/unimatrix-server/src/server.rs`** -- Add `correct_with_audit()` method that performs the two-entry atomic operation (deprecate original + insert correction + audit) in a single write transaction. Fix `insert_with_audit()` to include VECTOR_MAP write in the combined transaction (GH #14).

**`crates/unimatrix-server/src/response.rs`** -- Add format functions: `format_correct_success()`, `format_deprecate_success()`, `format_status_report()`, `format_briefing()`.

**`crates/unimatrix-server/src/categories.rs`** -- Add "duties" and "reference" to the initial category allowlist.

**`crates/unimatrix-vector/src/index.rs`** -- Add `insert_hnsw_only(entry_id, embedding) -> u64` that inserts into HNSW and updates IdMap but skips the VECTOR_MAP write. Expose `allocate_data_id()` for external transaction use.

**`crates/unimatrix-server/src/validation.rs`** -- Add `validate_correct_params()`, `validate_deprecate_params()`, `validate_status_params()`, `validate_briefing_params()`.

### Tool Execution Flows

**context_correct:**
1. Identity -> require_capability(Write) -> validate -> parse format
2. Category validation (if category provided)
3. Content scanning (new content + optional title)
4. Fetch original entry (verify exists, verify active)
5. Embed new title+content
6. `correct_with_audit()`: single write txn that deprecates original (set superseded_by, status=Deprecated, increment correction_count) + inserts correction (set supersedes=original_id) + VECTOR_MAP + audit
7. HNSW insert (after commit)
8. Format response (shows both old and new entry)

**context_deprecate:**
1. Identity -> require_capability(Write) -> validate -> parse format
2. Fetch entry (verify exists, verify not already deprecated)
3. Combined deprecate+audit in single write txn: update_status + audit event
4. Format response (confirmation with entry summary)

**context_status:**
1. Identity -> require_capability(Admin) -> validate -> parse format
2. Read counters (total_active, total_deprecated, total_proposed)
3. Scan indexes for category/topic distribution (filtered if params provided)
4. Correction chain metrics (entries with supersedes/superseded_by, total corrections)
5. Security metrics scan (trust_source distribution, attribution gaps)
6. Format response (structured report)

**context_briefing:**
1. Identity -> require_capability(Read) -> validate -> parse format
2. Lookup conventions: query(topic=role, category="convention", status=Active)
3. Lookup duties: query(topic=role, category="duties", status=Active)
4. Embed task description, search for task-relevant entries (k=3)
5. If `feature` provided, boost entries tagged with the feature ID in result ordering
6. Assemble briefing within token budget
7. Audit (standalone, read-only)
8. Format response

### GH #14 Fix

Modify `insert_with_audit()` in `server.rs`:
1. Call `vector_index.allocate_data_id()` to get the next data_id
2. Write `VECTOR_MAP.insert(entry_id, data_id)` inside the combined transaction
3. After commit, call `vector_index.insert_hnsw_only(entry_id, data_id, &embedding)` for the HNSW insert only

This requires:
- `VectorIndex::allocate_data_id() -> u64` -- atomic increment of `next_data_id`
- `VectorIndex::insert_hnsw_only(entry_id: u64, data_id: u64, embedding: &[f32])` -- insert into HNSW + update IdMap, skip VECTOR_MAP write

The server needs access to `Arc<VectorIndex>` (not just `Arc<AsyncVectorStore>`). Either add it as a field on `UnimatrixServer` or extract it from the `AsyncVectorStore`.

## Acceptance Criteria

### context_correct

- **AC-01**: `context_correct` accepts `original_id` (required), `content` (required), and optional `reason`, `topic`, `category`, `tags`, `title`, `agent_id`, `format`. Returns the new corrected entry.
- **AC-02**: The original entry's status is set to `Deprecated`, its `superseded_by` field is set to the new entry's ID, and its `correction_count` is incremented by 1.
- **AC-03**: The new entry's `supersedes` field is set to the original entry's ID. Topic, category, and tags are inherited from the original when not provided in params.
- **AC-04**: Content scanning is applied to the new content and optional title. Prompt injection and PII patterns are rejected.
- **AC-05**: Category validation is applied when a new category is provided. The inherited category (from original) is not re-validated.
- **AC-06**: The correction is embedded and indexed in the vector store. The new entry is discoverable via `context_search`.
- **AC-07**: The original entry and the correction entry are both updated/created in a single write transaction (atomicity).
- **AC-08**: `context_correct` returns `ServerError::Core(EntryNotFound)` when `original_id` does not exist.
- **AC-09**: `context_correct` returns an error when the original entry is already deprecated (cannot correct a deprecated entry).
- **AC-10**: Write capability is required. Agents lacking Write receive MCP error -32003.

### context_deprecate

- **AC-11**: `context_deprecate` accepts `id` (required), optional `reason`, `agent_id`, `format`. Returns confirmation.
- **AC-12**: The entry's status is set to `Deprecated`. The status counter, STATUS_INDEX, and ENTRIES table are updated atomically.
- **AC-13**: Deprecating an already-deprecated entry is a no-op that returns success (idempotent).
- **AC-14**: `context_deprecate` returns `ServerError::Core(EntryNotFound)` when the ID does not exist.
- **AC-15**: Write capability is required. Agents lacking Write receive MCP error -32003.
- **AC-16**: An audit event with the reason (if provided) is logged for the deprecation.

### context_status

- **AC-17**: `context_status` accepts optional `topic`, `category`, `agent_id`, `format`. Returns a health report.
- **AC-18**: The report includes entry counts by status (active, deprecated, proposed).
- **AC-19**: The report includes category distribution (count per category) and topic distribution (count per topic), filtered when params are provided.
- **AC-20**: The report includes correction chain metrics: count of entries with `supersedes` set, count with `superseded_by` set, total `correction_count` sum across all entries.
- **AC-21**: The report includes security metrics: entries by `trust_source`, count of entries with empty `created_by`.
- **AC-22**: Admin capability is required. Agents lacking Admin receive MCP error -32003.

### context_briefing

- **AC-23**: `context_briefing` accepts `role` (required), `task` (required), optional `feature`, `max_tokens` (default 3000), `agent_id`, `format`. Returns an assembled briefing.
- **AC-24**: The briefing includes conventions for the role (lookup topic=role, category="convention").
- **AC-25**: The briefing includes duties for the role (lookup topic=role, category="duties").
- **AC-26**: The briefing includes task-relevant context from semantic search (embed task, search k=3). When `feature` is provided, entries tagged with that feature ID are boosted in result ordering (prioritization, not filtering).
- **AC-27**: The briefing respects the `max_tokens` budget, truncating least-relevant content first.
- **AC-28**: When the embedding model is not ready, the briefing falls back to lookup-only results (no search component) instead of failing entirely.
- **AC-29**: Read capability is required. Agents lacking Read receive MCP error -32003.

### Bug Fix: VECTOR_MAP Transaction (GH #14)

- **AC-30**: The VECTOR_MAP write is included in the same write transaction as entry insert and audit event in `insert_with_audit`.
- **AC-31**: A crash after commit but before HNSW insert results in the VECTOR_MAP mapping being present (entry is semantically discoverable after HNSW rebuild from VECTOR_MAP).
- **AC-32**: The HNSW in-memory insert still happens after the transaction commits (it is not a redb operation).
- **AC-33**: The fix applies to both `insert_with_audit` (used by `context_store`) and `correct_with_audit` (used by `context_correct`).

### Category Allowlist Extension

- **AC-34**: The initial category allowlist includes "duties" and "reference" in addition to the existing 6 categories (total: 8).

### Response Formatting

- **AC-35**: All 4 new tools accept the optional `format` parameter (summary/markdown/json) consistent with v0.1 tools.
- **AC-36**: `context_correct` response shows both the deprecated original and the new correction entry.
- **AC-37**: `context_deprecate` response confirms deprecation with the entry's title and ID.
- **AC-38**: `context_status` response is structured: status counts, category/topic distributions, correction chain metrics, security metrics. JSON format returns a structured object.
- **AC-39**: `context_briefing` response has sections: Conventions, Duties, Relevant Context. Markdown is the natural format; summary returns a compact version; JSON returns structured sections.

### Integration

- **AC-40**: All existing tests (506 across 5 crates) continue to pass.
- **AC-41**: All server code follows workspace conventions: `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89.
- **AC-42**: No new crate dependencies beyond what the workspace already has (regex, serde_json, etc. are already present).

### Audit

- **AC-43**: All 4 new tools log audit events. Mutating tools (correct, deprecate) use the combined transaction path. Read-only tools (status, briefing) use standalone audit.
- **AC-44**: Audit events for corrections include both the original and new entry IDs in `target_ids`.
- **AC-45**: Audit event monotonic IDs remain sequential across all tools (v0.1 and v0.2).

## Constraints

- **rmcp =0.16.0** pinned exactly, per vnc-001.
- **Rust edition 2024, MSRV 1.89, `#![forbid(unsafe_code)]`** per workspace.
- **No new crate dependencies.** All needed crates (regex, serde_json, tokio, etc.) are already in the dependency tree.
- **redb write serialization** -- only one write transaction at a time. Combined transactions are essential for multi-table atomicity.
- **EmbedServiceHandle lazy loading** -- `context_correct` and `context_briefing` must handle the not-ready state. `context_briefing` degrades gracefully (lookup-only).
- **Store engine auto-computes** `content_hash`, `previous_hash`, `version` on `update()`. The `update_status()` method does NOT change these fields.
- **Test infrastructure is cumulative** -- build on the existing 506 tests.
- **No hardcoded agent roles** -- the `role` param in `context_briefing` is freeform text, not an enum. Lookups use it as a topic filter.
- **EntryRecord `supersedes`/`superseded_by` are `Option<u64>`** -- single-entry correction chains (one supersedes one). Multi-way corrections are not supported.
- **VectorIndex HNSW insert and VECTOR_MAP write are currently coupled** in `VectorIndex::insert()`. Decoupling them (GH #14 fix) requires adding new methods to `VectorIndex` without breaking existing callers.

## Resolved Open Questions

1. **Should `context_correct` allow correcting a deprecated entry?** RESOLVED: No. A deprecated entry is already marked irrelevant -- correcting it does not make sense. AC-09 stands: reject corrections on deprecated entries.

2. **What age threshold defines "stale" in `context_status`?** RESOLVED: Drop time-based staleness entirely. An entry is not stale because it is old -- a repo may simply be inactive. What matters is utilization, not age. Usage tracking (crt-001) will populate `access_count` and `last_accessed_at` (fields already exist, initialized to 0). `context_status` reports basic counts now and defers utilization metrics to crt-001. Age distribution, stale entry detection, and time-based thresholds are removed from this scope.

3. **Should `context_briefing` use the `feature` parameter for filtering?** RESOLVED: Use for prioritization, not filtering. When `feature` is provided, entries tagged with the matching feature ID are boosted in search result ordering, but entries without the tag are not rejected. This is a scoring adjustment on results already returned -- no extra queries needed.

4. **Duplicate candidates in `context_status` -- how expensive is the scan?** RESOLVED: Defer to background. Pairwise similarity scanning is O(n^2) and should not run inline in `context_status`. Duplicate detection is excluded from this scope entirely. If needed later, it will be a background process or a dedicated command.

5. **Should `context_status` report content_hash validation?** RESOLVED: Separate audit command. Content hash re-validation (re-hash every entry and compare) is an integrity audit concern, not a health metrics concern. It is excluded from vnc-003 and deferred to a future explicit audit command.

## Tracking

GH Issue: https://github.com/dug-21/unimatrix/issues/15
