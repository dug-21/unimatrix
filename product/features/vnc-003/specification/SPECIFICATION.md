# Specification: vnc-003 v0.2 Tool Implementations

## Objective

Implement four v0.2 MCP tools (`context_correct`, `context_deprecate`, `context_status`, `context_briefing`) that enable knowledge lifecycle management, health monitoring, and agent orientation. Fix the VECTOR_MAP transaction atomicity bug (GH #14) and extend the category allowlist with "duties" and "reference" categories.

## Functional Requirements

### FR-01: context_correct

- **FR-01a**: Accept parameters: `original_id` (required, i64), `content` (required, string), `reason` (optional), `topic` (optional), `category` (optional), `tags` (optional), `title` (optional), `agent_id` (optional), `format` (optional).
- **FR-01b**: Deprecate the original entry by setting `status = Deprecated`, `superseded_by = new_entry_id`, and incrementing `correction_count` by 1.
- **FR-01c**: Insert a new correction entry with `supersedes = original_id`. Inherit `topic`, `category`, and `tags` from the original entry when not provided in params.
- **FR-01d**: Apply content scanning to the new content and optional title. Reject on match (same patterns as `context_store`).
- **FR-01e**: Apply category validation when a new category is provided. Do not re-validate the inherited category from the original entry.
- **FR-01f**: Embed the correction's title+content and insert into the vector index. The correction is discoverable via `context_search`.
- **FR-01g**: Perform the deprecation and insertion in a single write transaction (atomicity). Include VECTOR_MAP write in the transaction.
- **FR-01h**: Return error when `original_id` does not exist (`EntryNotFound`).
- **FR-01i**: Return error when the original entry is already deprecated (cannot correct a deprecated entry).
- **FR-01j**: Require `Write` capability. Return MCP error -32003 if denied.
- **FR-01k**: Log an audit event with `target_ids = [original_id, new_id]` in the combined transaction.
- **FR-01l**: Return a response showing both the deprecated original and the new correction entry.

### FR-02: context_deprecate

- **FR-02a**: Accept parameters: `id` (required, i64), `reason` (optional), `agent_id` (optional), `format` (optional).
- **FR-02b**: Set the entry's status to `Deprecated`. Update STATUS_INDEX and status counters atomically.
- **FR-02c**: Deprecating an already-deprecated entry is a no-op that returns success (idempotent).
- **FR-02d**: Return error when the ID does not exist (`EntryNotFound`).
- **FR-02e**: Require `Write` capability. Return MCP error -32003 if denied.
- **FR-02f**: Log an audit event with the reason (if provided) in the detail field. Use combined transaction for the mutation case. No audit event for the idempotent no-op case.
- **FR-02g**: Return a confirmation response with the entry's title and ID.

### FR-03: context_status

- **FR-03a**: Accept parameters: `topic` (optional), `category` (optional), `agent_id` (optional), `format` (optional).
- **FR-03b**: Return entry counts by status: active, deprecated, proposed. These are always global (not filtered).
- **FR-03c**: Return category distribution: count per category. Filtered when `category` param is provided (show only that category's count).
- **FR-03d**: Return topic distribution: count per topic. Filtered when `topic` param is provided (show only that topic's count).
- **FR-03e**: Return correction chain metrics: count of entries with `supersedes` set, count with `superseded_by` set, total `correction_count` sum across all entries.
- **FR-03f**: Return security metrics: entries grouped by `trust_source`, count of entries with empty `created_by`.
- **FR-03g**: Require `Admin` capability. Return MCP error -32003 if denied.
- **FR-03h**: All reads happen in a single read transaction for a consistent snapshot.
- **FR-03i**: Log an audit event (standalone, read-only path).

### FR-04: context_briefing

- **FR-04a**: Accept parameters: `role` (required, string), `task` (required, string), `feature` (optional), `max_tokens` (optional, default 3000), `agent_id` (optional), `format` (optional).
- **FR-04b**: Look up conventions for the role: query `topic=role, category="convention", status=Active`.
- **FR-04c**: Look up duties for the role: query `topic=role, category="duties", status=Active`.
- **FR-04d**: Search for task-relevant context: embed the task description, search with k=3. When `feature` is provided, boost entries tagged with the feature ID (reorder, not filter).
- **FR-04e**: Assemble the briefing within the `max_tokens` budget (character-based estimate: max_tokens * 4 chars). Truncate from least-relevant entries first.
- **FR-04f**: When the embedding model is not ready, return a lookup-only briefing (conventions + duties only) instead of failing. Include an indicator that search was unavailable.
- **FR-04g**: Require `Read` capability. Return MCP error -32003 if denied.
- **FR-04h**: The `role` parameter is freeform text (not an enum). It is used as a topic filter.
- **FR-04i**: Log an audit event (standalone, read-only path).

### FR-05: VECTOR_MAP Transaction Atomicity (GH #14 Fix)

- **FR-05a**: Include the VECTOR_MAP write in the same write transaction as entry insert and audit event in `insert_with_audit`.
- **FR-05b**: After a crash following commit but before HNSW insert, the VECTOR_MAP mapping is present. The entry is semantically discoverable after HNSW rebuild from VECTOR_MAP.
- **FR-05c**: The HNSW in-memory insert still happens after the transaction commits (not a redb operation).
- **FR-05d**: The fix applies to both `insert_with_audit` (context_store) and `correct_with_audit` (context_correct).

### FR-06: Category Allowlist Extension

- **FR-06a**: Add "duties" and "reference" to the initial category allowlist (total: 8 categories).
- **FR-06b**: The categories are available immediately on server startup without runtime `add_category()` calls.

### FR-07: Response Formatting

- **FR-07a**: All 4 new tools accept the optional `format` parameter (summary/markdown/json).
- **FR-07b**: `context_correct` response shows both the deprecated original and the new correction.
- **FR-07c**: `context_deprecate` response confirms deprecation with the entry's title and ID.
- **FR-07d**: `context_status` response has structured sections: status counts, category/topic distributions, correction chain metrics, security metrics.
- **FR-07e**: `context_briefing` response has sections: Conventions, Duties, Relevant Context. Markdown is the natural format.

### FR-08: Audit

- **FR-08a**: All 4 new tools log audit events. Mutating tools (correct, deprecate) use the combined transaction path. Read-only tools (status, briefing) use standalone audit.
- **FR-08b**: Audit events for corrections include both the original and new entry IDs in `target_ids`.
- **FR-08c**: Audit event monotonic IDs remain sequential across all tools (v0.1 and v0.2), using the shared COUNTERS["next_audit_id"].

## Non-Functional Requirements

- **NFR-01**: All existing tests (506 across 5 crates) continue to pass.
- **NFR-02**: All code follows workspace conventions: `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89.
- **NFR-03**: No new crate dependencies. All needed crates (regex, serde_json, tokio, etc.) are already in the dependency tree.
- **NFR-04**: `context_status` full scan completes within reasonable time for up to 10,000 entries (the expected Unimatrix scale).
- **NFR-05**: `context_briefing` total latency should be dominated by the embedding call, not by the lookup/assembly logic.
- **NFR-06**: Combined write transactions hold redb's single write lock for minimal duration.

## Acceptance Criteria

Every AC from SCOPE.md is reproduced here with verification method.

### context_correct (AC-01 through AC-10)

| AC-ID | Description | Verification Method |
|-------|-------------|-------------------|
| AC-01 | `context_correct` accepts `original_id` (required), `content` (required), and optional `reason`, `topic`, `category`, `tags`, `title`, `agent_id`, `format`. Returns the new corrected entry. | test: param deserialization + handler integration |
| AC-02 | Original entry's status set to Deprecated, superseded_by set to new ID, correction_count incremented by 1. | test: verify original entry fields after correction |
| AC-03 | New entry's supersedes set to original ID. Topic/category/tags inherited when not provided. | test: verify inheritance and override behavior |
| AC-04 | Content scanning applied to new content and optional title. Injection and PII rejected. | test: trigger scan rejection on correction content |
| AC-05 | Category validation applied when new category provided. Inherited category not re-validated. | test: validate new category, skip inherited |
| AC-06 | Correction is embedded and indexed in vector store. Discoverable via context_search. | test: search for correction after insert |
| AC-07 | Original and correction updated/created in single write transaction (atomicity). | test: verify both entries in one commit |
| AC-08 | Returns EntryNotFound when original_id does not exist. | test: correct non-existent ID |
| AC-09 | Returns error when original is already deprecated. | test: correct deprecated entry |
| AC-10 | Write capability required. Agents lacking Write receive MCP error -32003. | test: capability denial |

### context_deprecate (AC-11 through AC-16)

| AC-ID | Description | Verification Method |
|-------|-------------|-------------------|
| AC-11 | Accepts `id` (required), optional `reason`, `agent_id`, `format`. Returns confirmation. | test: param deserialization + handler |
| AC-12 | Entry status set to Deprecated. Counters/indexes updated atomically. | test: verify status + counters after deprecation |
| AC-13 | Deprecating already-deprecated entry is a no-op returning success. | test: double deprecation |
| AC-14 | Returns EntryNotFound when ID does not exist. | test: deprecate non-existent ID |
| AC-15 | Write capability required. MCP error -32003 if denied. | test: capability denial |
| AC-16 | Audit event with reason logged for deprecation. | test: verify audit log entry |

### context_status (AC-17 through AC-22)

| AC-ID | Description | Verification Method |
|-------|-------------|-------------------|
| AC-17 | Accepts optional `topic`, `category`, `agent_id`, `format`. Returns health report. | test: param deserialization + handler |
| AC-18 | Report includes entry counts by status. | test: verify status counts |
| AC-19 | Report includes category and topic distribution, filtered when params provided. | test: verify distributions with and without filters |
| AC-20 | Report includes correction chain metrics. | test: verify chain metrics after corrections |
| AC-21 | Report includes security metrics. | test: verify trust_source distribution + attribution gaps |
| AC-22 | Admin capability required. MCP error -32003 if denied. | test: capability denial |

### context_briefing (AC-23 through AC-29)

| AC-ID | Description | Verification Method |
|-------|-------------|-------------------|
| AC-23 | Accepts `role` (required), `task` (required), optional `feature`, `max_tokens`, `agent_id`, `format`. | test: param deserialization |
| AC-24 | Briefing includes conventions for the role. | test: store conventions, verify in briefing |
| AC-25 | Briefing includes duties for the role. | test: store duties, verify in briefing |
| AC-26 | Briefing includes task-relevant context from semantic search. Feature entries boosted. | test: with embedding model |
| AC-27 | Briefing respects max_tokens budget. | test: verify truncation at budget |
| AC-28 | Embed not ready: falls back to lookup-only. | test: briefing without embedding model |
| AC-29 | Read capability required. MCP error -32003 if denied. | test: capability denial |

### Bug Fix (AC-30 through AC-33)

| AC-ID | Description | Verification Method |
|-------|-------------|-------------------|
| AC-30 | VECTOR_MAP write in same transaction as entry insert + audit. | test: verify VECTOR_MAP present in same txn |
| AC-31 | Crash after commit: VECTOR_MAP mapping present. | test: verify mapping after simulated crash |
| AC-32 | HNSW insert still happens after transaction commits. | test: verify HNSW point count after insert |
| AC-33 | Fix applies to both insert_with_audit and correct_with_audit. | test: verify for both code paths |

### Category / Response / Integration / Audit (AC-34 through AC-45)

| AC-ID | Description | Verification Method |
|-------|-------------|-------------------|
| AC-34 | Initial allowlist includes "duties" and "reference" (total: 8). | test: validate both new categories |
| AC-35 | All 4 new tools accept format parameter. | test: each tool with each format |
| AC-36 | Correct response shows deprecated original + new correction. | test: verify response content |
| AC-37 | Deprecate response confirms with title and ID. | test: verify response content |
| AC-38 | Status response is structured with all sections. | test: parse JSON response |
| AC-39 | Briefing response has Conventions/Duties/Relevant Context sections. | test: verify section presence |
| AC-40 | All existing tests (506) continue to pass. | shell: cargo test --workspace |
| AC-41 | Code follows workspace conventions. | grep: forbid(unsafe_code), edition check |
| AC-42 | No new crate dependencies. | file-check: Cargo.toml diff |
| AC-43 | All 4 tools log audit events. Combined txn for mutations. | test: verify audit log entries |
| AC-44 | Correction audit includes both original and new IDs. | test: verify target_ids |
| AC-45 | Audit IDs remain sequential across all tools. | test: verify monotonic IDs after mixed operations |

## Domain Models

### Correction Chain

An entry can supersede at most one other entry (`supersedes: Option<u64>`), and be superseded by at most one entry (`superseded_by: Option<u64>`). This forms a linked list: `A -> B -> C` where A is the original, B supersedes A, C supersedes B. Each link is bidirectional.

When `context_correct(original_id=A)` creates entry B:
- A.superseded_by = B.id
- A.status = Deprecated
- A.correction_count += 1
- B.supersedes = A.id
- B.status = Active

Multi-way corrections (one entry superseded by multiple) are not supported. Correcting an already-deprecated entry is rejected.

### Status Report (Health Metrics)

A read-only aggregate of the knowledge base state:

- **Status counts**: total active, deprecated, proposed (from COUNTERS table)
- **Category distribution**: count per category (from CATEGORY_INDEX scan)
- **Topic distribution**: count per topic (from TOPIC_INDEX scan)
- **Correction chain metrics**: entries with supersedes set, entries with superseded_by set, total correction_count sum (from ENTRIES scan)
- **Security metrics**: trust_source distribution, entries without created_by (from ENTRIES scan)

Excluded from scope: stale entry detection (needs crt-001 usage tracking), duplicate candidate scanning (O(n^2), deferred), content_hash re-validation (separate audit concern).

### Briefing

A composite read that assembles role-specific orientation:

- **Conventions**: entries with `topic=role, category="convention", status=Active`
- **Duties**: entries with `topic=role, category="duties", status=Active`
- **Relevant Context**: top-3 semantically similar entries to the task description

The briefing respects a character budget (max_tokens * 4). Content is assembled in priority order: conventions first, duties second, relevant context third. Truncation removes entries from the lowest-priority section first (relevant context), then duties, then conventions.

### Capability Requirements

| Tool | Required Capability | Rationale |
|------|-------------------|-----------|
| context_correct | Write | Creates new entry + modifies original |
| context_deprecate | Write | Modifies entry status |
| context_status | Admin | Exposes knowledge base internals |
| context_briefing | Read | Read-only composite query |

## User Workflows

### Workflow 1: Correcting Wrong Knowledge

An agent discovers that a stored convention is incorrect.

1. Agent calls `context_correct(original_id=42, content="The correct convention is...", reason="Previous version was based on outdated API")`
2. Server verifies entry 42 exists and is Active
3. Server scans new content for injection/PII
4. Server deprecates entry 42 (superseded_by=43), inserts entry 43 (supersedes=42)
5. Agent receives response showing both entries
6. Future searches find entry 43 (active); entry 42 is deprecated but still accessible via `context_get`

### Workflow 2: Deprecating Obsolete Knowledge

An agent or human determines a stored pattern is no longer relevant.

1. Agent calls `context_deprecate(id=42, reason="Pattern replaced by new framework")`
2. Server sets entry 42 to Deprecated
3. Agent receives confirmation
4. Future lookups with `status=active` (default) exclude entry 42

### Workflow 3: Knowledge Base Health Check

An admin agent checks the health of the knowledge base.

1. Agent calls `context_status(format="json")`
2. Server computes all metrics in a consistent read transaction
3. Agent receives structured report: 150 active, 23 deprecated, 2 proposed; 5 corrections; 3 entries without attribution
4. Agent can act on findings (e.g., investigate entries without attribution)

### Workflow 4: Agent Orientation

An agent is about to start architecture work on a feature.

1. Agent calls `context_briefing(role="architect", task="Design the authentication module", feature="vnc-003")`
2. Server looks up conventions for "architect" topic
3. Server looks up duties for "architect" topic
4. Server embeds the task and searches for relevant context
5. Server boosts entries tagged with "vnc-003"
6. Server assembles briefing within 3000 token budget
7. Agent receives structured orientation

## Constraints

- **rmcp =0.16.0** pinned exactly, per vnc-001.
- **Rust edition 2024, MSRV 1.89, `#![forbid(unsafe_code)]`** per workspace.
- **No new crate dependencies.** All needed crates are already in the dependency tree.
- **redb write serialization**: only one write transaction at a time. Combined transactions are essential.
- **EmbedServiceHandle lazy loading**: context_correct must handle not-ready. context_briefing degrades gracefully.
- **Store engine auto-computes** content_hash, previous_hash, version on update(). update_status() does NOT change these.
- **Test infrastructure is cumulative**: build on existing 506 tests.
- **No hardcoded agent roles**: the `role` param in context_briefing is freeform text.
- **EntryRecord supersedes/superseded_by are Option<u64>**: single-entry correction chains only.
- **VectorIndex HNSW insert and VECTOR_MAP write are currently coupled**: decoupling is required (GH #14 fix).

## Dependencies

### Crate Dependencies (all existing)

| Crate | Used For |
|-------|---------|
| rmcp =0.16.0 | MCP protocol, #[tool] macro |
| serde, serde_json | Serialization, JSON responses |
| schemars | JSON Schema for tool parameters |
| tokio | Async runtime, spawn_blocking |
| redb | Database transactions |
| regex | Content scanning (existing) |
| bincode | Audit event serialization (existing) |

### Internal Dependencies

| Crate | Used By |
|-------|---------|
| unimatrix-store | ENTRIES, VECTOR_MAP, indexes, counters |
| unimatrix-vector | VectorIndex (extended with new methods) |
| unimatrix-core | Traits, async wrappers, types |
| unimatrix-embed | EmbedAdapter (consumed via EmbedServiceHandle) |

## NOT in Scope

- **No v0.3 tools.** MCP Resources, Prompts, and local embeddings are future milestones.
- **No confidence computation.** The confidence formula (usage x freshness x correction x helpfulness) is crt-002.
- **No usage tracking.** Access counting is crt-001. context_briefing does not log entry usage.
- **No "force store" parameter.** Near-duplicate override on context_store is deferred.
- **No contradiction detection.** Detecting conflicting entries is crt-003.
- **No staleness metrics.** Time-based staleness depends on crt-001 usage tracking.
- **No content_hash re-validation.** Deferred to a future audit command.
- **No inline duplicate scanning.** O(n^2) pairwise similarity is deferred.
- **No persisted category allowlist.** Categories remain runtime-only.
- **No HTTP/SSE transport.** Stdio only.
- **No cross-project scope.** Single project per server instance.
- **No batch operations.** Each tool call operates on one entry/query.
- **No token counting.** Character-based budget estimation only.
