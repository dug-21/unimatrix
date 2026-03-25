# ASS-030 — Data Model Analysis
**Analyst Role**: Data Modeler
**Date**: 2026-03-24
**Scope**: Full schema review, access path analysis, mission alignment, and recommendations

---

## 1. Purpose

This document analyzes Unimatrix's data model against its stated mission: *"a self-learning knowledge integrity engine that captures knowledge emerging from doing work, makes it trustworthy, correctable, and ever-improving, and delivers the right knowledge at the right time."*

The analysis covers table design, access patterns, referential integrity, data quality, and model alignment. No code was changed. Recommendations are for discussion and future planning.

---

## 2. Schema Inventory

Current schema version: **15**. Single SQLite file. ~20 distinct tables divided by write path:

### 2.1 Integrity Tables (write_pool direct)

These tables are authoritative and write-critical. Writes are synchronous and transactional.

| Table | Description | Rows grow via |
|-------|-------------|---------------|
| `entries` | Core knowledge entries — the primary object | context_store |
| `entry_tags` | Many-to-many tag junction (FK CASCADE to entries) | context_store |
| `vector_map` | Bridge: entry_id ↔ HNSW hnsw_data_id | context_store |
| `counters` | Schema version + ID sequences + status counters | every mutation |
| `agent_registry` | Agent identity, trust level, capabilities | context_enroll |
| `audit_log` | Immutable per-request operation log | every MCP call |
| `cycle_events` | Feature lifecycle event log (start/phase/stop) | context_cycle |

### 2.2 Analytics Tables (async drain queue, fire-and-forget)

These tables support the learning pipeline. Writes are batched, sheddable under pressure.

| Table | Description | Rows grow via |
|-------|-------------|---------------|
| `co_access` | Co-retrieval frequency pairs | every search returning 2+ results |
| `graph_edges` | Typed relationship graph (Supersedes, CoAccess, Contradicts, Supports) | migrations, NLI, co-access threshold |
| `feature_entries` | Entry-to-feature-cycle + phase attribution | context_store |
| `outcome_index` | Feature completion signals | context_retrospective |
| `sessions` | Session lifecycle (feature, role, injections) | session open/close |
| `injection_log` | Which entry was injected in which session | every injection |
| `query_log` | Query text, timestamp, result_entry_ids, scores | every context_search |
| `observations` | Raw hook event telemetry (tool, input, response) | every hook event |
| `shadow_evaluations` | Neural vs. rule-based category classification | NLI post-store |
| `observation_metrics` | Aggregate computed metrics per feature_cycle | retrospective tick |
| `observation_phase_metrics` | Phase-level metric breakdown (FK CASCADE to observation_metrics) | retrospective tick |
| `topic_deliveries` | Cross-session topic delivery lifecycle | session attribution |
| `signal_queue` | Work queue for confidence signals | session close |

### 2.3 In-Memory Structures (not persisted)

| Structure | What it holds | Rebuilt from |
|-----------|---------------|--------------|
| HNSW index | Embedding vectors for ANN search | vector_map + embedding pipeline |
| TypedRelationGraph | petgraph StableGraph over graph_edges | graph_edges table on tick |
| SessionRegistry | Per-session state: injection history, category histogram, phase, rework events | transient — lost on reconnect |
| ConfidenceState | Adaptive Bayesian prior + spread | computed on maintenance tick |

### 2.4 Entry Record — Full Field Set

The `entries` table has 26 columns carrying several concerns in a single row:

| Field group | Fields |
|-------------|--------|
| Identity | id, title, content, topic, category, source |
| Lifecycle | status, created_at, updated_at, version |
| Usage | last_accessed_at, access_count, helpful_count, unhelpful_count |
| Integrity chain | content_hash, previous_hash, created_by, modified_by |
| Confidence inputs | correction_count, (helpful/unhelpful above) |
| Provenance | feature_cycle, trust_source |
| Embedding | embedding_dim |
| Correction chain | supersedes, superseded_by |
| State restoration | pre_quarantine_status |

---

## 3. Access Path Analysis

### 3.1 Primary Hot Path: context_search

```
context_search(query, topic?, category?, tags?, k=5)
  → embed(query)         — ONNX inference (rayon pool)
  → HNSW top-20         — in-memory index
  → load entries         — read_pool SELECT by id IN (...)
  → NLI re-rank          — cross-encoder ONNX (rayon pool)
  → FusedScoreInputs:
      similarity        — from HNSW
      nli_entailment    — from NLI
      confidence        — from entries.confidence
      coac_norm         — from in-memory TypedRelationGraph
      util_norm         — from EffectivenessSnapshot
      prov_norm         — from config boosted_categories
      phase_histogram   — from SessionState.category_counts (in-memory)
      phase_explicit    — always 0.0 (W3-1 placeholder)
  → top-k returned
  → fire-and-forget: co_access upsert, query_log insert, injection_log insert
```

**Key observation**: The hot path reads ZERO analytics tables directly. All runtime ranking signals come from in-memory caches. Analytics tables are write-only from the hot path. This is the correct design.

### 3.2 Write Path: context_store

```
context_store(title, content, topic, category, tags, source, feature_cycle, trust_source)
  → capability check     — agent_registry read
  → content_hash compute — SHA-256(title+content)
  → insert entry         — write_pool transaction
  → insert entry_tags    — same transaction
  → enqueue analytics:   — fire-and-forget
      feature_entries insert
      session co_access update
  → embed(content)       — async, after return
  → HNSW insert          — in-memory, after embed
  → vector_map insert    — write_pool, after embed
  → NLI post-store check — fire-and-forget, after embed
  → graph_edges insert   — write_pool, if NLI confirms
  → confidence refresh   — fire-and-forget
  → audit_log insert     — write_pool
```

### 3.3 Lifecycle Path: context_cycle

```
context_cycle(type, topic, phase?, outcome?)
  → resolve cycle_id from topic
  → cycle_events INSERT  — write_pool direct (not analytics drain)
  → session state update  — SessionRegistry.current_phase, feature
  → sessions row update  — analytics drain
  → topic_deliveries upsert — analytics drain
```

### 3.4 Learning Path: Background Maintenance Tick

```
every tick (background.rs):
  → drain signal_queue   → update entries.helpful_count, unhelpful_count
  → recompute confidence → update entries.confidence
  → rebuild TypedRelationGraph → from graph_edges
  → rebuild EffectivenessSnapshot → from co_access + observation_metrics
  → coherence Lambda computation → read all active entries
  → NLI contradiction sweep → write graph_edges if new edges
  → co_access cleanup    → prune low-count pairs
```

### 3.5 Attribution Path: context_cycle_review

Current (pre-col-024):
```
sessions WHERE feature_cycle = 'feature-N'
  → fetch observations WHERE session_id IN (above)
  → compute 21 UniversalMetrics
  → write observation_metrics, observation_phase_metrics
```

Post-col-024 (agreed design):
```
PRIMARY:
  cycle_events WHERE cycle_id = 'feature-N' → (start, stop) windows
    → observations WHERE topic_signal = 'feature-N' AND ts_millis IN windows
    → confirmed session_ids
    → all observations WHERE session_id IN confirmed AND ts_millis IN windows
LEGACY FALLBACK:
  sessions WHERE feature_cycle = 'feature-N'
```

---

## 4. Mission Alignment Assessment

### 4.1 "Trustworthy, correctable, with full provenance" — Strong

The integrity chain design is solid. `content_hash` (SHA-256 of title+content), `previous_hash` (hash of the superseded entry's content_hash), `version`, `created_by`, `modified_by`, and the audit_log together form a tamper-evident provenance chain. The correction path (supersedes/superseded_by + graph_edges Supersedes type) provides readable history. Quarantine with pre_quarantine_status enables state restoration. **This part of the mission is well-served.**

### 4.2 "Self-learning from usage" — Partially Implemented, Data Collection Strong

The learning signal pipeline is comprehensive in design:
- Explicit signals: `helpful_count`, `unhelpful_count` per entry (updated from signal_queue)
- Implicit signals: rework events, session outcomes → signal_queue → confidence
- Co-access: co_access table + TypedRelationGraph CoAccess edges
- Phase-labeled storage: `feature_entries.phase` for W3-1 GNN training
- Behavioral: `observations` table for retrospective detection rules

The data collection infrastructure is strong. However, the learning consumer (W3-1 GNN) does not yet exist. The model is collecting the right data but the loop from data → learned model → improved retrieval is not yet closed. **The schema anticipates W3-1 correctly. The mission readiness depends on W3-1 delivery.**

### 4.3 "Deliver the right knowledge at the right time" — Reactive, Moving Toward Proactive

The current delivery surfaces are:
- **Reactive**: context_search (query-driven)
- **Transition**: context_briefing (phase-conditioned, post-WA-4)
- **Proactive**: UDS injection (pre-tool hook, phase-conditioned candidate cache, post-WA-4)

The ranking formula (fused 8-term score) is well-grounded in the data model. Every signal in the formula maps to a real stored field. **The schema serves the current delivery model. The roadmap toward proactive session-conditioned delivery (WA-4, W3-1) requires data the schema already captures.**

### 4.4 "Any domain without code changes" — Partially Completed

W0-3 config externalization decoupled categories, freshness half-life, and server instructions. W1-5 (in progress) generalizes the HookType enum. However:
- `observations.hook` is a TEXT field that currently stores dev-workflow hook names
- `trust_source` vocabulary remains dev-flavored (product vision: "Low" open gap)
- `observation_metrics` has 21 hardcoded claude-code-specific columns plus a `domain_metrics_json` escape hatch (schema v14 ADR-006 hybrid)

**Domain agnosticism is partially achieved at the config layer but not fully at the schema layer.**

---

## 5. Structural Findings

### F1. Supersedes/Superseded_by in Entries + graph_edges — Complementary, Not Redundant

*Clarified 2026-03-24 based on design intent discussion.*

These two structures solve **different access problems** and both are necessary.

**`entries.supersedes` / `entries.superseded_by`** — local, embedded provenance on the record itself. An agent reading a single entry can immediately answer "is this still current?" and "what did this correct?" without any traversal. One hop, self-describing, tamper-evident on the artifact.

**`graph_edges WHERE relation_type = 'Supersedes'`** — efficient multi-hop traversal. When A supersedes B supersedes C, `find_terminal_active` walks the full DAG in a single call and returns the active tip. Search surfaces only the current knowledge — deprecated predecessors are filtered by the graph before results are returned. An agent that wants the full correction history gets it in one traversal rather than N individual entry lookups.

Together they form a complete correction chain architecture:

| Need | Structure used |
|------|---------------|
| "Is this entry still current?" | `entries.superseded_by` — single field read, no graph |
| "What did this correct?" | `entries.supersedes` — single field read, no graph |
| Search surfaces the active entry | `graph_edges` → `find_terminal_active` at query time |
| Trace the full correction chain | `graph_edges` — full DAG path in one call |
| Verify provenance (audit/integrity) | `entries` chain — hop-by-hop, hash-anchored |

The relationship is: **entries writes first (within the transaction), graph_edges is kept in sync as the traversal index.**

**Risk (Low)**: A write path that updates entries but not graph_edges leaves traversal temporarily stale — bounded by the next tick's graph rebuild. The reverse (graph_edges without entries) breaks embedded provenance and is a higher-severity defect. `store_correct.rs` must remain the only write path for corrections.

**Recommendation**: Document this dual-structure design explicitly as an ADR. The pattern is non-obvious to new contributors who may see two places tracking "supersession" and assume one is redundant. A test asserting graph_edges.Supersedes consistency after context_correct would enforce the write-ordering contract structurally.

### F2. Missing Referential Integrity on Analytics Tables — Low-Medium Risk

Several tables reference entry IDs without SQL foreign key constraints:

| Table | Column | Has FK? |
|-------|--------|---------|
| co_access | entry_id_a, entry_id_b | No |
| graph_edges | source_id, target_id | No |
| feature_entries | entry_id | No |
| injection_log | entry_id | No |
| outcome_index | entry_id | No |

When an entry is deprecated or quarantined (not deleted), these references remain valid by convention. But no structural guarantee prevents referencing a non-existent entry_id. The HNSW vector_map bridge also has no FK.

**Risk**: Low for current workload (entries are rarely hard-deleted). Moderate risk as the knowledge base grows and compaction removes entries from the HNSW index.

### F3. JSON Array Columns in Query-Relevant Tables — Low-Medium Concern

The following columns store structured arrays as JSON TEXT:

| Table | Column | Queried? |
|-------|--------|---------|
| query_log | result_entry_ids | Yes — eval harness, training data |
| query_log | similarity_scores | Yes — eval harness |
| signal_queue | entry_ids | Yes — confidence pipeline, read at drain time |
| agent_registry | capabilities, allowed_topics, allowed_categories | Yes — capability checks |
| sessions | keywords | Unknown — see F7 |
| topic_deliveries | phases_completed | Occasionally |
| observation_metrics | domain_metrics_json | Domain-specific queries |

`result_entry_ids` in `query_log` is particularly important: it's the ground-truth data for the W1-3 eval harness and potential W3-1 training labels. Deserializing a JSON array on every eval replay adds overhead and prevents SQL-level joins.

ADR-007 consciously chose JSON for "non-queried vec fields" — but result_entry_ids is queried. This was an acceptable trade-off at the time but warrants revisiting as eval harness usage grows.

### F4. Unbounded Append-Only Tables — Long-Term Risk

No retention policy exists for:

| Table | Growth driver | Daily row estimate (heavy use) |
|-------|--------------|-------------------------------|
| observations | Every hook event | 1,000–10,000+ |
| query_log | Every search | 50–500 |
| injection_log | Every injection | 50–500 |
| shadow_evaluations | Every NLI post-store | 10–100 |
| audit_log | Every MCP call | 100–1,000 |

Over months of daemon use on a developer workstation, `observations` in particular will become the dominant table by row count. The current compaction (`compact()`) runs WAL checkpoint + VACUUM, which reduces file fragmentation but does not remove rows.

**This is not an immediate crisis but is a known gap for a long-running daemon deployment.**

### F5. sessions.feature_cycle Attribution Reliability — Active Bug

Documented in col-024 memory. `sessions.feature_cycle` is written via a read-modify-write pattern under async conditions, making it vulnerable to:
- Last-writer-wins when a bugfix session overlaps a feature session
- Race conditions on server restart

The `cycle_events` table (written as a plain append) is the more durable source. The col-024 redesign correctly moves `context_cycle_review` to use cycle_events as primary attribution. **The model has the right tables — the usage pattern is wrong.**

### F6. In-Memory Session Histogram Lost on Reconnect — Design Acknowledged Gap

`SessionState.category_counts` (the WA-2 category histogram for the affinity boost) is never persisted. The product vision explicitly accepts this. However:
- The WA-2 boost (w_phase_histogram=0.02) is cold on every new connection
- If an agent reconnects mid-session (e.g., IDE restart), the histogram is lost
- This is a mild degradation of the intelligence pipeline

The fix is straightforward: a lightweight `session_category_counts` table would survive reconnections. The current state is an acknowledged cold-start condition.

### F7. sessions.keywords — Inert Stored Field

The Unimatrix knowledge base contains an explicit entry noting: *"context_cycle keywords field is inert — stored but never consumed"* (entry #2987). The `keywords` column on `sessions` is populated via context_cycle but never read by any downstream path. It was added in schema v12 (col-022) with the intent of enabling keyword-driven injection, but that feature was not implemented.

**This is dead schema — data is collected but produces no value.**

### F8. counters Table Status Counter Consistency Risk — Low Risk

The `counters` table maintains `total_active`, `total_deprecated`, `total_proposed`, `total_quarantined` as running counters incremented/decremented by write operations. These are denormalized aggregates of `entries.status`.

**Risk**: If any write path modifies `entries.status` without updating the corresponding counter (e.g., a bulk migration, a direct DB operation, or a bug in counter management), the counters drift from reality. The `context_status` output relies on these counters.

**Mitigation**: The counters are used for display purposes only; stale counts don't affect ranking. But they affect operator confidence in the system state.

### F9. bootstrap_only Flag Lifecycle — Open Question

`graph_edges.bootstrap_only = 1` marks edges that need NLI confirmation before being used in confidence scoring. The v13 migration sets bootstrap_only = 0 for Supersedes edges (authoritative) and co_access edges with count >= 3 (reliable).

The bootstrap_only flag is excluded from confidence scoring (TypedRelationGraph two-pass build, W1-1 pattern). However, there is no documented process for when bootstrap_only edges are "complete" — i.e., all have been either confirmed (bootstrap_only → 0) or removed by NLI contradiction detection. This creates a permanent underclass of edges that may never be promoted.

### F10. graph_edges.metadata Untyped — Low Concern

The `metadata` column stores NLI confidence scores as JSON TEXT. For the specific use case (NLI confidence from the cross-encoder), a typed `REAL` column would be more efficient and queryable. The current design anticipates potential future metadata fields, but the column is rarely queried directly.

---

## 6. Opportunities

### O1. Document the Dual-Structure Correction Chain as an ADR

The complementary roles of entries fields (single-hop embedded provenance) and graph_edges (multi-hop traversal index) are non-obvious. A new contributor seeing "supersedes" in two places will assume redundancy.

Document as an ADR:
- `entries.supersedes` / `superseded_by`: canonical per-record provenance. Written first, in the correction transaction. Participates in hash chain. Self-describing for an agent reading any single entry.
- `graph_edges Supersedes`: traversal index. Bootstrapped from entries. Enables `find_terminal_active` to resolve the active tip of any correction chain in one call. Search uses this to surface only current knowledge.
- Write ordering: entries transaction commits first; graph_edges insert is enqueued. Drift is bounded by the next TypedRelationGraph rebuild tick.

Add a test: after `context_correct`, assert that graph_edges contains a Supersedes edge matching entries.supersedes for the corrected pair.

**Impact**: Protects the design from being "simplified away" by future contributors. Makes the provenance architecture legible.

### O2. query_log Result Junction Table

Replace `query_log.result_entry_ids TEXT` with a `query_log_results` table:

```sql
CREATE TABLE query_log_results (
    query_id INTEGER NOT NULL REFERENCES query_log(query_id) ON DELETE CASCADE,
    entry_id INTEGER NOT NULL,
    rank     INTEGER NOT NULL,
    score    REAL,
    PRIMARY KEY (query_id, entry_id)
);
```

**Impact**: Enables efficient SQL joins for eval harness. Enables FK integrity on result_entry_ids. Makes training label queries simpler. Trade-off: higher write volume per search (currently one row, becomes N rows for top-K results).

### O3. Retention Policy for High-Volume Tables

Define and implement a rolling retention strategy:

| Table | Suggested retention |
|-------|---------------------|
| observations | Last 90 days OR last 100,000 rows, whichever is less |
| query_log | Last 90 days |
| injection_log | Last 90 days (until W3-1 training completes) |
| shadow_evaluations | Last 30 days (model shadow period only) |
| audit_log | Last 180 days (security/compliance) |

Retention sweeps could run in the background maintenance tick. A `DELETE FROM observations WHERE ts_millis < (now - 90d * 1000)` is a low-cost operation during off-peak periods.

### O4. Persist Session Category Histogram

Add a lightweight table to persist the WA-2 category signal across reconnections:

```sql
CREATE TABLE session_category_counts (
    session_id TEXT NOT NULL,
    category   TEXT NOT NULL,
    count      INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (session_id, category)
);
```

Populate via the same path that updates `SessionState.category_counts`. Load into `SessionState` on session registration. TTL-cleaned on session close.

**Impact**: WA-2 affinity boost is no longer cold on reconnect. Small table — bounded by sessions × categories.

### O5. Implement keywords Consumption or Drop the Column

The sessions.keywords column collects data that is never consumed. Either:
- **Implement**: Use keywords in the UDS injection candidate selection (the original intent — keyword-driven injection)
- **Remove**: Drop the column in the next schema migration and stop writing it

Leaving inert columns in the schema is a maintenance burden and misleads future contributors about what the system does.

### O6. Typed Phase Completion in topic_deliveries

`topic_deliveries.phases_completed` stores a comma-separated TEXT list. Promote to a junction table:

```sql
CREATE TABLE topic_delivery_phases (
    topic        TEXT NOT NULL REFERENCES topic_deliveries(topic) ON DELETE CASCADE,
    phase_name   TEXT NOT NULL,
    completed_at INTEGER NOT NULL,
    PRIMARY KEY (topic, phase_name)
);
```

**Impact**: Enables phase completion queries. Consistent with `observation_phase_metrics` pattern.

### O7. Add FK Constraints to High-Value Analytics Tables

At minimum, add FK constraints to `feature_entries` and `injection_log`:

```sql
-- feature_entries
FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE

-- injection_log
FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE SET NULL
```

For `co_access` and `graph_edges`, cascade-on-delete FK would require the compaction process to remove graph edges when entries are hard-deleted — consistent with the existing graph rebuild pattern.

### O8. source_domain Column on observations (W1-5 Alignment)

As W1-5 generalizes the observation pipeline, the `observations.hook` TEXT column will evolve. Adding a `source_domain TEXT NOT NULL DEFAULT 'claude-code'` column now would enable per-domain analytics queries and make the W1-5 migration smoother.

### O9. Composite Index on cycle_events for col-024

The col-024 review redesign queries:
```sql
SELECT ... FROM cycle_events WHERE cycle_id = 'feature-N' ORDER BY timestamp ASC
```

The current index is `idx_cycle_events_cycle_id ON cycle_events(cycle_id)`. A composite index:
```sql
CREATE INDEX idx_cycle_events_cycle_id_ts ON cycle_events(cycle_id, timestamp);
```

would serve the ordered window query without a separate sort step. Low-cost addition.

### O10. W2-3 Tenant Isolation — Schema Planning Now

W2-3 (Multi-Project Routing + OAuth) introduces owner-tier and project-tier stores. Currently, there are no tenant discriminator columns in any table. Two paths:

**Path A: Separate DB files per project** (current approach for projects). Clean isolation. No schema changes needed. Cross-project queries require application-level federation.

**Path B: Tenant column discriminator** (if single-DB multi-tenant is pursued). Requires adding `project_id TEXT NOT NULL` to `entries`, `feature_entries`, `graph_edges`, and related tables. Significant migration.

The product vision describes `TenantRouter` resolving `Arc<Store>` pairs at request time — this implies Path A (separate DB files), which is already the current architecture. However, the owner-tier (cross-project) store design implies a separate DB file for the owner tier. **No schema changes needed for W2-3 if the separate-DB-per-project model is confirmed.** This should be documented explicitly.

---

## 7. Questions for the Human

These questions arose during the analysis where design intent was unclear from the code alone.

**Q1 — Retention policy**: What is the intended lifetime of the analytics tables (observations, query_log, injection_log, shadow_evaluations, audit_log)? Is there a target DB size budget for a developer workstation over, say, 12 months of active use?

**Q2 — supersedes/superseded_by ownership**: *(Answered 2026-03-24)* entries.supersedes/superseded_by are canonical — they represent the provable knowledge chain embedded in the record itself, visible to agents without graph traversal. graph_edges Supersedes is the derivative index for graph algorithms. No plans to retire the entries fields.

**Q3 — graph_edges.metadata extensibility**: Is NLI confidence the only thing stored in metadata, or is this intended to be open-ended? If it's genuinely open-ended, what query patterns are anticipated against it?

**Q4 — session histogram persistence**: Is the loss of category_counts on reconnect considered acceptable long-term (i.e., agents always get a cold-start histogram at reconnect), or is O4 (persist via table) on the roadmap before W3-1 training begins?

**Q5 — bootstrap_only edge promotion lifecycle**: What completes the bootstrap_only → 0 promotion for all edges? Is there a background tick that re-evaluates and promotes/removes bootstrap edges over time, or is this a one-time migration artifact?

**Q6 — co_access table vs. graph_edges CoAccess edges**: Are both co_access and graph_edges kept permanently, with co_access being the raw frequency counter and graph_edges being the filtered derived view (threshold ≥ 3)? Or is there a path to consolidating these into a single structure?

**Q7 — keywords field intent**: Is the sessions.keywords column still planned for a keyword-driven injection feature, or should it be treated as inert and candidates for removal?

**Q8 — query_log result_entry_ids format**: Is the eval harness (W1-3) planning to parse the JSON column at eval time, or is there a plan to migrate to a structured junction table before W3-1 training scales up?

**Q9 — PostgreSQL migration horizon**: W0-1 sqlx migration was explicitly designed to enable PostgreSQL migration "with no application logic rewrite." Is PostgreSQL on a concrete timeline? This significantly affects the value of SQLite-specific optimizations in the data model vs. investing in a model that will migrate cleanly.

**Q10 — cycle_events authority**: With the col-024 redesign making cycle_events the primary attribution source, should sessions.feature_cycle be formally demoted to "audit/status display only" in documentation? Or is there a scenario where sessions.feature_cycle should continue to be used as a primary lookup?

---

## 8. Summary Scorecard

| Dimension | Assessment | Notes |
|-----------|------------|-------|
| Knowledge integrity chain | Strong | Hash-chain, correction chain, audit log all well-designed |
| Confidence model | Strong | 6-factor composite correctly stored; adaptive state in memory |
| Ranking signal coverage | Strong | All 8 fusion terms map to stored or computable signals |
| Referential integrity | Partial | entry_tags has FK CASCADE; analytics tables mostly do not |
| Attribution reliability | Weak → Improving | sessions.feature_cycle bug documented; col-024 fixes this |
| Analytics table retention | Not implemented | Unbounded growth is a long-term risk |
| Domain agnosticism | Partial | Config layer done; observation schema still dev-workflow-flavored |
| GNN training data readiness | On track | feature_entries.phase, query_log, behavioral signals all collected |
| Tenant isolation (W2-3) | Needs clarification | Path A (separate files) vs Path B (discriminator) not confirmed in schema |
| Inert / dead data | Minor issue | sessions.keywords collected, never consumed |
| Correction chain ownership | Intentional — document it | entries.supersedes is canonical; graph_edges Supersedes is derived index |
| Table growth policy | Missing | High-priority gap for long-running daemon |

---

*Report compiled from: schema.rs, migration.rs, db.rs, analytics.rs, read.rs, write.rs, signal.rs, observations.rs, topic_deliveries.rs, sessions.rs (store crate); search.rs, confidence.rs (server crate); PRODUCT-VISION.md; Unimatrix knowledge base (ADRs #360, #361, #634, #818, #819, #2284, #2476, #2701, #2844, #2908, #2987, #3162, #3175, #3210, #3367); col-024 design memory.*
