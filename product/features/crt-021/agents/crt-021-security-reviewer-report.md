# Security Review: crt-021-security-reviewer

## Risk Level: low

## Summary

crt-021 (W1-1) upgrades the in-memory supersession graph to a typed, persisted
`GRAPH_EDGES` table and replaces `SupersessionState` with `TypedGraphState`. The change
is internal infrastructure only — no MCP tool signatures change, no new external inputs
are introduced. All SQL is parameterized (no injection surface). Input validation on
`weight` (NaN/Inf guard), `relation_type` (whitelist via `from_str`), and endpoint
resolution (skip-and-warn on missing node) is correct and adequate. No hardcoded
credentials or secrets. Two low-severity structural observations are noted; neither is
blocking.

---

## Findings

### Finding 1: Cycle dispatch uses string-match on error message

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/background.rs`, added block at line ~505
- **Description**: The background tick distinguishes a `CycleDetected` rebuild failure
  from a generic store error using `e.to_string().contains("supersession cycle detected")`.
  This relies on the exact string literal `"supersession cycle detected"` in
  `TypedGraphState::rebuild()` (`typed_graph.rs` line 121). If that string is ever
  refactored (e.g., renamed to `"typed-graph cycle detected"`) the match silently falls
  through to the generic error arm (`tracing::error!("typed graph state rebuild
  failed")`) without setting `use_fallback=true`. The search path would then continue
  using the stale graph rather than falling back safely. This is a future maintenance
  hazard, not an exploitable flaw.
- **Recommendation**: Replace the string match with a typed error variant (e.g., add
  `StoreError::CycleDetected` distinct from `StoreError::InvalidInput`) so the
  dispatch is checked at compile time. This is a low-priority code quality issue; the
  current implementation is functionally correct.
- **Blocking**: no

### Finding 2: `GraphEdgeRow` is duplicated across two crates

- **Severity**: low
- **Location**: `crates/unimatrix-engine/src/graph.rs` line 136 and
  `crates/unimatrix-store/src/read.rs` line 1251
- **Description**: Two identical `GraphEdgeRow` structs exist — one in `unimatrix-engine`
  (the primary definition per the architecture's build-sequencing note) and one in
  `unimatrix-store` (exported from `lib.rs`). `TypedGraphState::rebuild()` manually maps
  from `unimatrix_store::GraphEdgeRow` to `unimatrix_engine::GraphEdgeRow` field by
  field. If a field is added to the store type but not the engine type (or vice versa),
  the map silently omits it. This is by design (avoiding a crate cycle) but creates a
  maintenance surface: any future field addition in `GRAPH_EDGES` must be propagated to
  both types. The duplication is documented in the architecture as a consequence of the
  crate dependency ordering (engine cannot depend on store).
- **Recommendation**: Add a comment in both struct definitions cross-referencing the
  other location. Acceptable for W1-1 scope; the architecture acknowledges this.
- **Blocking**: no

---

## Input Validation Assessment

### `AnalyticsWrite::GraphEdge` drain path (`analytics.rs`)

- `weight`: validated with `weight.is_finite()` before the INSERT; NaN/Inf drops the
  event and logs `tracing::error!`. Correct.
- `relation_type`: string field inserted via parameterized `sqlx::query(...).bind(...)`.
  No raw concatenation. The field is not validated against the `RelationType` whitelist
  before insertion (it's trusted as `RelationType::as_str()` output from callers), but
  at query time `build_typed_relation_graph` applies `RelationType::from_str` and skips
  unrecognized strings with a warning. Defense-in-depth is adequate.
- `source_id`, `target_id`: bound as `i64` (Rust type prevents overflow). The
  `UNIQUE(source_id, target_id, relation_type)` constraint and `INSERT OR IGNORE` pattern
  prevent duplicate insertion.
- `created_by`, `source`: free text strings inserted via parameterized query. No
  validation needed for this trust level (internal analytics writes). No injection
  vector.

### `query_graph_edges()` read path (`read.rs`)

- Fixed SQL, no user-supplied parameters. Zero injection surface.
- All column reads use `try_get` with explicit type annotations; type mismatches surface
  as `StoreError::Database`, not panics.
- `bootstrap_only` stored as `i64` in SQLite, correctly mapped to `bool` via `!= 0`.

### Migration v12→v13 (`migration.rs`)

- All SQL is fixed DDL + fixed INSERT/SELECT statements with a single bound parameter
  (`CO_ACCESS_BOOTSTRAP_MIN_COUNT` for the threshold). No caller-supplied strings.
- `COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)` correctly guards
  division by zero and empty-table NULL propagation (R-06 resolved).
- `INSERT OR IGNORE` throughout ensures idempotency (R-08 resolved).

### Graph builder input boundary (`graph.rs`)

- `bootstrap_only=true` rows are excluded at the top of the Pass 2b loop before any
  index resolution — structural exclusion, not conditional scoring (R-03 resolved).
- Supersedes rows from GRAPH_EDGES are skipped in Pass 2b; only `entries.supersedes` is
  authoritative for Supersedes topology. Correct cycle-detection semantics preserved.
- Unrecognized `relation_type` strings logged and skipped (R-10 handled).
- Missing source/target node indices logged and skipped — no panic on dangling
  references.
- Pass 3 cycle detection builds a temporary Supersedes-only subgraph to avoid
  false-positive cycles from CoAccess bidirectional pairs. The `temp_nodes[&src_id]`
  direct index (which panics on missing key) is safe because `temp_nodes` is populated
  from the same `graph.node_index.keys()` set that supplies node IDs to edges, and all
  edges in `graph.inner` were added only after confirming both endpoints exist in
  `node_index`. This invariant is maintained by the construction passes.

---

## Blast Radius Assessment

**Worst case if this fix has a subtle bug:**

The most dangerous silent failure mode is `use_fallback` remaining `false` while
`TypedRelationGraph` contains incorrect edges (e.g., bootstrap-only edges not filtered,
or non-Supersedes edges contaminating `graph_penalty`). This would cause search
re-ranking to silently penalize valid entries — affecting all `context_search` results,
but not causing data corruption or privilege escalation. The failure mode is degraded
search quality, not data loss or security breach.

The worst-case error path (migration failure) is fail-safe: if the v12→v13 migration
aborts, `schema_version` stays at 12 and the server fails to start cleanly rather than
running with a partial schema.

The `use_fallback=true` cold-start behavior means an empty graph always degrades safely
to `FALLBACK_PENALTY` rather than silently penalizing entries.

---

## Regression Risk

**What existing functionality could break:**

1. `context_search` — directly depends on `graph_penalty` and `find_terminal_active`
   via `TypedRelationGraph`. The search path change removes the per-query
   `build_supersession_graph` call and reads the pre-built graph from the handle.
   The 25+ existing graph unit tests are ported to `graph_tests.rs` and cover the
   penalty semantics with Supersedes-only graphs — this provides strong regression
   coverage.

2. Schema version assertions — `server.rs` tests that checked `schema_version = 12`
   are correctly updated to `schema_version = 13`. A test that was missed would fail
   immediately on CI.

3. `SupersessionState` / `SupersessionStateHandle` rename — the architecture prohibits
   type aliases; the compiler enforces the ~20 call site rename. No partial rename risk
   at compile time.

4. Background tick sequencing — the compaction DELETE now runs before the graph rebuild.
   If compaction fails, the tick proceeds to rebuild (non-fatal, documented). This
   matches the architecture's non-fatal compaction contract.

---

## Dependency Safety

No new external crate dependencies are introduced. The `petgraph` crate (existing) gains
a new import: `petgraph::visit::{EdgeRef, IntoEdgeReferences}`. These are
standard petgraph traits for edge iteration — no security concern.

`migration.rs` visibility changed from `pub(crate)` to `pub`, exposing
`CURRENT_SCHEMA_VERSION` externally. This is a read-only constant; no security concern.

---

## Secrets Check

No hardcoded secrets, API keys, tokens, or credentials in any crt-021 implementation
file. Test helpers use in-memory SQLite databases (`tempfile::TempDir`). All agent IDs
and source labels in bootstrap rows are literal strings (`"bootstrap"`,
`"entries.supersedes"`, `"co_access"`) — not credentials.

---

## OWASP Concern Coverage

| OWASP Category | Assessment |
|---------------|-----------|
| Injection (SQL) | No raw string interpolation in any new SQL; all parameterized via sqlx `bind()` |
| Injection (path traversal) | No file path operations added |
| Broken access control | No new MCP tools, no new authorization gates; existing gate structure unchanged |
| Security misconfiguration | `migration.rs` promoted to `pub` exposes `CURRENT_SCHEMA_VERSION` — not sensitive |
| Vulnerable components | No new dependencies |
| Data integrity failures | `UNIQUE` constraint + `INSERT OR IGNORE` ensures idempotent writes; weight NaN guard |
| Deserialization risks | `GraphEdgeRow` deserialized from SQLite via sqlx typed reads — type errors surface as `StoreError` |
| Input validation gaps | `relation_type` and `weight` validated at point of use in graph builder and drain task |

---

## PR Comments

- Posted findings comment on PR #316 (see below)
- Blocking findings: no

---

## Knowledge Stewardship

- nothing novel to store — the string-based error discrimination pattern (Finding 1)
  is feature-specific to the cycle error path and has not appeared as a cross-feature
  anti-pattern yet. The GraphEdgeRow duplication is an intentional consequence of the
  crate dependency ordering, documented in the architecture. Neither rises to a
  generalizable lesson at this time.
