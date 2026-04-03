# crt-043: Behavioral Signal Infrastructure — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-043/SCOPE.md |
| Architecture | product/features/crt-043/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-043/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-043/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-043/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| schema-migration | pseudocode/schema-migration.md | test-plan/schema-migration.md |
| goal-embedding | pseudocode/goal-embedding.md | test-plan/goal-embedding.md |
| phase-capture | pseudocode/phase-capture.md | test-plan/phase-capture.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Add two signal-plumbing columns to the SQLite schema (v20 → v21): `goal_embedding BLOB` on
`cycle_events` to enable future H1 goal-clustering, and `phase TEXT` on `observations` to
enable future H3 phase-stratification. Both are write-path-only additions; no retrieval path,
search ranking, or MCP tool response format is changed. They are prerequisites for Group 6
(behavioral edge emission) and Group 7 (goal-conditioned briefing) in the ASS-040 roadmap.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| SQLite embedding blob serialization format | `bincode::serde::encode_to_vec(vec, config::standard())`. Paired `encode_goal_embedding` / `decode_goal_embedding` helpers mandated as `pub(crate)` in `unimatrix-store`, same PR as write path. Rationale: self-describing length prefix, model-upgrade-safe, no new dependency, sets Group 6 pattern. | ADR-001 (Unimatrix #4067) | architecture/ADR-001-bincode-embedding-blob.md |
| INSERT/UPDATE race resolution for `goal_embedding` | Option 1: spawn the embedding task from within `handle_cycle_event` after the INSERT spawn. Options 2 and 3 are architecturally unavailable — MCP handler and UDS listener are independent paths with no shared trigger point. Residual race accepted as cold-start-compatible NULL degradation (identical to embed-service-unavailable path). | ADR-002 (Unimatrix #4068) | architecture/ADR-002-insert-update-race-resolution.md |
| v20→v21 migration atomicity | Both ADD COLUMN statements execute within the existing outer transaction opened by `migrate_if_needed()`. No additional `BEGIN`/`COMMIT` needed. Both `pragma_table_info` pre-checks run before either ALTER TABLE. Order: `goal_embedding` first, then `phase`. | ADR-003 (Unimatrix #4069) | architecture/ADR-003-v21-migration-atomicity.md |

---

## Files to Create / Modify

### unimatrix-store

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/migration.rs` | Modify | Add `current_version < 21` block: two `pragma_table_info` checks + two ALTER TABLE statements + schema_version bump to 21; update `CURRENT_SCHEMA_VERSION` constant from 20 to 21 |
| `crates/unimatrix-store/src/db.rs` | Modify | Add `update_cycle_start_goal_embedding(cycle_id: &str, embedding_bytes: Vec<u8>) -> Result<()>` async store method; add `phase` bind to `insert_observation` and `insert_observations_batch` SQL |
| `crates/unimatrix-store/src/embedding.rs` | Create (or inline in `db.rs`) | `encode_goal_embedding(Vec<f32>) -> Result<Vec<u8>, EncodeError>` and `decode_goal_embedding(&[u8]) -> Result<Vec<f32>, DecodeError>` — `pub(crate)` helpers; round-trip unit test included |

### unimatrix-server

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/uds/listener.rs` | Modify | Extend `handle_cycle_event` signature with `embed_service: &Arc<EmbedServiceHandle>` parameter; add Step 6 embedding spawn after INSERT spawn for `CycleLifecycle::Start` when goal is non-empty; update `dispatch_request` to pass `embed_service` at all three call sites; add `phase: Option<String>` to `ObservationRow`; capture `current_phase` before `spawn_blocking` at all four write sites |

---

## Data Structures

### ObservationRow (unimatrix-server/src/uds/listener.rs)

```rust
struct ObservationRow {
    // ... existing fields ...
    topic_signal: Option<String>,  // existing
    phase: Option<String>,          // NEW — crt-043
}
```

### cycle_events schema (after v21 migration)

```sql
-- existing columns unchanged; one column added:
goal_embedding BLOB  -- NULL unless context_cycle(type=start) with non-empty goal
```

### observations schema (after v21 migration)

```sql
-- existing columns unchanged; one column added:
phase TEXT  -- NULL when no active cycle or current_phase not set
```

---

## Function Signatures

### New store method (unimatrix-store/src/db.rs)

```rust
pub async fn update_cycle_start_goal_embedding(
    &self,
    cycle_id: &str,
    embedding_bytes: Vec<u8>,
) -> Result<()>
```

Issues: `UPDATE cycle_events SET goal_embedding = ?1 WHERE topic = ?2 AND event_type = 'cycle_start'`

### Serialization helpers (unimatrix-store/src/embedding.rs)

```rust
pub(crate) fn encode_goal_embedding(vec: Vec<f32>) -> Result<Vec<u8>, bincode::error::EncodeError>
pub(crate) fn decode_goal_embedding(bytes: &[u8]) -> Result<Vec<f32>, bincode::error::DecodeError>
```

### Extended handle_cycle_event signature (unimatrix-server/src/uds/listener.rs)

```rust
fn handle_cycle_event(
    event: &ImplantEvent,
    lifecycle: CycleLifecycle,
    session_registry: &SessionRegistry,
    store: &Arc<Store>,
    embed_service: &Arc<EmbedServiceHandle>,  // added
)
```

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | No retrieval path changes — write-path infrastructure only |
| C-02 | No new crate dependencies — `bincode` and `EmbedServiceHandle` already in workspace |
| C-03 | Goal embedding MUST NOT happen inside `handle_cycle_event` synchronously — UDS hook budget is 50ms; embedding goes in a fire-and-forget spawn |
| C-04 | `insert_cycle_event` signature MUST NOT change — called from UDS listener, embedding is a separate UPDATE |
| C-05 | Phase values stored as-is — no allowlist at write time; Group 6 must apply `LOWER()` at query time |
| C-06 | INSERT-before-UPDATE race: embed spawn is registered after INSERT spawn in `tokio::spawn` queue; residual race is accepted as NULL degradation |
| C-07 | `update_cycle_start_goal_embedding` MUST NOT acquire the Store mutex independently from other cycle-start fire-and-forget work (NFR-03 / SR-03) |
| C-08 | `current_phase` MUST be captured before `spawn_blocking` at all four observation write sites — same pre-capture pattern as `topic_signal` enrichment |
| C-09 | `context_cycle` MCP response text is unchanged from pre-crt-043 behavior |

**Delivery constraint (WARN-2):** Before opening the PR, resolve whether `decode_goal_embedding` must be `pub` (not `pub(crate)`) for cross-crate use by Group 6. If Group 6 will call the decode helper directly from `unimatrix-server`, it cannot be `pub(crate)`. If Group 6 will consume embeddings through a store query method that decodes internally, `pub(crate)` is correct. The decision must be made and documented before the PR is opened — do not defer to Group 6.

**Delivery constraint (FR-C-07):** Before opening the PR, evaluate and decide whether a composite index on `(topic_signal, phase)` should be added to `observations` in the v21 migration. Justify the decision in writing. If added, verify index presence in the migration test.

**Delivery constraint (edge case):** Decide whether `goal = " "` (whitespace-only) should be treated as absent (no spawn) or non-empty (spawn task). Document the decision in code comments.

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `EmbedServiceHandle` | Internal — `unimatrix-embed` | Already injected into `dispatch_request` via `embed_service: &Arc<EmbedServiceHandle>`; accessible at all three `handle_cycle_event` call sites |
| `bincode` (serde feature) | Internal — already in workspace | No version change. `encode_to_vec` + `decode_from_slice` with `config::standard()` |
| `SessionState.current_phase: Option<String>` | Internal — `unimatrix-server/session.rs` (crt-025) | Read via `session_registry.get_state(session_id)?.current_phase`; O(1) Mutex read |
| `enrich_topic_signal` pre-capture pattern | Internal — `listener.rs` (col-024 ADR-004, entry #3374) | Item C follows identical pre-`spawn_blocking` timing contract |
| `cycle_events.idx_cycle_events_cycle_id` | Schema — existing v16 index | Makes the UPDATE in `update_cycle_start_goal_embedding` cheap |
| `ml_inference_pool` (rayon) | Internal — server runtime | Required for embedding computation; MUST NOT run on a tokio thread (entry #771) |
| Schema v20 | Migration baseline | `CURRENT_SCHEMA_VERSION = 20`; migration target is 21 |
| v20 test fixture database | Test infrastructure | Required for FR-M-04 (entry #378 lesson): migration must be validated against a real v20 database, not a fresh schema |

---

## NOT in Scope

- `observations.feature_id` column — `topic_signal` is the feature ID; no new column needed
- `goal_clusters` table — Group 6 deliverable
- Goal-conditioned briefing changes — Group 7 deliverable
- Behavioral edge emission (S6/S7) — Group 6 deliverable, conditional on crt-043
- `audit_log` changes — explicitly excluded
- Backfill of pre-v21 rows — NULL for historical rows is accepted cold-start degradation
- GitHub API call, `gh` CLI, HTTP client — goal comes exclusively from `context_cycle` MCP parameter
- `context_cycle` MCP response format changes — response text is unchanged
- Primary entry embedding storage in SQLite — noted in ADR-001 as a follow-up for crt-042 SR-01; not implemented here
- Phase allowlist enforcement at write time — normalization deferred to Group 6 query time
- S6/S7/H1/H3 signal consumption — columns are produced here; consumption is Group 6/7

---

## Alignment Status

Vision alignment: **PASS.** crt-043 is pure signal plumbing for Wave 1A adaptive intelligence. `goal_embedding` enables H1 goal-clustering; `phase` enables H3 phase-stratification. Both are direct prerequisites for Group 6 (S6/S7 edge emission) and Group 7 (goal-conditioned briefing). No retrieval path is touched.

Milestone fit: **PASS with WARN.** Group 5 dependencies are clear; crt-041 (S1/S2/S8 enrichment) is shipped. crt-043 can deliver now.

### WARN-1 — ASS-040 Roadmap Group 5 goal-text source is stale

The ASS-040 roadmap (Group 5, row 2) specifies fetching GH issue title + body as the goal-text source. crt-043 SCOPE.md explicitly drops the GitHub fetch and uses the `context_cycle(goal=...)` MCP parameter instead. The roadmap text has not been updated. Future Group 6/7 authors reading the roadmap will expect GH-fetch-sourced embeddings but find MCP-parameter-sourced embeddings instead. **Action required before Group 6 design:** update `product/research/ass-040/ROADMAP.md` Group 5 row 2 to reflect the approved approach. This is roadmap maintenance only — crt-043 scope and all source documents are internally consistent.

### WARN-2 — `decode_goal_embedding` visibility is a delivery-time API surface decision

ARCHITECTURE.md declares the helpers `pub(crate)`, but Group 6 agents implementing H1 goal-clustering will need to decode `goal_embedding` blobs. If they live in `unimatrix-server`, a `pub(crate)` helper in `unimatrix-store` is inaccessible. The delivery agent must resolve this before the PR opens: either promote the helper to `pub`, or confirm that Group 6 will consume embeddings through a store query method that decodes internally. **This is a blocking delivery constraint** — do not defer to Group 6.
