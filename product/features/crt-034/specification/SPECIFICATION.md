# Specification: crt-034 — Recurring co_access → GRAPH_EDGES Promotion Tick

## Objective

The v12→v13 migration bootstrapped `co_access` pairs with `count >= 3` into `GRAPH_EDGES`
as `CoAccess`-typed edges, but every co-access pair written after that migration is
permanently invisible to Personalized PageRank (PPR). This feature adds a recurring
background tick step — `run_co_access_promotion_tick` — that promotes qualifying
`co_access` pairs into `GRAPH_EDGES` as `CoAccess` edges and refreshes the weight of
already-promoted edges when the normalized weight has drifted beyond a threshold. The
result is that PPR's co-access signal stays current on every tick cycle rather than
staying frozen at the bootstrap snapshot.

---

## Functional Requirements

**FR-01** — On each background tick, the system shall identify all `co_access` pairs where
`count >= CO_ACCESS_GRAPH_MIN_COUNT` and promote any such pair not yet present in
`GRAPH_EDGES` as a `CoAccess` edge, up to the configured per-tick cap.

**FR-02** — The candidate selection query shall order pairs by `count DESC` so that the
highest-signal pairs are selected first when the per-tick cap is reached.

**FR-03** — For each qualifying pair already present in `GRAPH_EDGES` as a `CoAccess` edge,
the system shall update the stored `weight` if and only if the absolute difference between
the new normalized weight and the stored weight exceeds `CO_ACCESS_WEIGHT_UPDATE_DELTA`.
Pairs within the delta boundary shall produce zero database writes.

**FR-04** — Weight normalization shall use `MAX(count)` computed over ALL qualifying pairs
(all pairs where `count >= CO_ACCESS_GRAPH_MIN_COUNT`), not only the capped batch, so that
weights remain on the same scale as bootstrapped edges across all ticks.

**FR-05** — Inserted `GRAPH_EDGES` rows shall have: `relation_type = 'CoAccess'`,
`source = 'co_access'`, `created_by = 'tick'`, `bootstrap_only = 0`.

**FR-06** — The promotion tick shall be positioned in `run_single_tick` after step 2
(orphaned-edge compaction) and before step 3 (`TypedGraphState::rebuild()`), so freshly
promoted edges are visible to PPR within the same tick cycle.

**FR-07** — The promotion tick shall run unconditionally on every tick — it is not gated
on NLI availability, feature flags, or any external condition.

**FR-08** — When `co_access` is empty or no pairs meet the threshold, the tick shall
complete as a no-op with no error and no warning — except: if `qualifying_count == 0`
AND `current_tick < PROMOTION_EARLY_RUN_WARN_TICKS`, the tick SHALL emit a `warn!` log
(SR-05 early-tick signal-loss detection, authorized per ADR-005). Outside that window,
zero qualifying pairs produces no warning.

**FR-09** — The promotion tick shall be infallible (`async fn ... -> ()`). Individual
write errors shall be logged at `warn!` and the tick shall continue to the next pair.
Errors shall never propagate to the tick caller.

**FR-10** — After completing the batch, the function shall emit a `tracing::info!` log
with the count of edges inserted and the count of edges updated in that tick.

**FR-11** — `InferenceConfig` shall gain a new field `max_co_access_promotion_per_tick:
usize` with serde default 200, validation range [1, 10000], and participation in
project-level config merge, following the exact same pattern as
`max_graph_inference_per_tick`.

**FR-12** — The public constant `CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3` shall be exposed
from `unimatrix-store`, matching the bootstrap threshold used in the v13 migration, so
the promotion tick and the migration share a single authoritative value.

**FR-13** — The public constant `EDGE_SOURCE_CO_ACCESS: &str = "co_access"` shall be
exported from `unimatrix-store` alongside the existing `EDGE_SOURCE_NLI`.

**FR-14** — The internal constant `CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1` shall be
defined as a named constant within the promotion module. It is not a config field.
The type is `f64` (not `f32`) because sqlx fetches SQLite `REAL` columns as `f64`;
comparing a fetched weight against an `f32` delta introduces implicit cast precision
noise (`0.1f32` as `f64` is `0.100000001490116...`). See ADR-003.

**FR-15** — The promotion function shall use `write_pool_server()` directly for all
database writes. The `AnalyticsWrite::GraphEdge` analytics drain path shall not be used,
as it supports only `INSERT OR IGNORE` and cannot express the conditional UPDATE semantics
required for weight refresh.

**FR-16** — The new module `services/co_access_promotion_tick.rs` shall stay under 500
lines. No rayon pool shall be used; the work is pure SQL with no CPU-bound ML inference.

**FR-17** — Sub-threshold `co_access` pairs (count below `CO_ACCESS_GRAPH_MIN_COUNT`)
that already have `CoAccess` edges in `GRAPH_EDGES` shall not be removed by this tick.
GC of stale `CoAccess` edges belongs to GH #409.

---

## Non-Functional Requirements

**NFR-01 — Tick latency.** The promotion tick must not materially extend the total tick
cycle time under normal load. Given the `co_access` table is approximately 0.34 MB, the
MAX(count) read query and the batch fetch are expected to complete in under 10 ms each on
a warm SQLite connection. This assumption should be validated by the architect and
confirmed in test.

**NFR-02 — Write pool contention.** All writes go through `write_pool_server()`. Under
a busy tick, individual INSERTs or UPDATEs may time out. Per the infallible tick contract
(FR-09), timeouts are logged at `warn!` and the pair is retried on the next tick. There
is no per-tick retry loop.

**NFR-03 — Cap behavior.** At the configured cap limit (default 200), only the
`max_co_access_promotion_per_tick` highest-`count` pairs are processed. Remaining
qualifying pairs are deferred to subsequent ticks, converging to full promotion over
multiple tick cycles.

**NFR-04 — Idempotency.** The tick is structurally idempotent: `INSERT OR IGNORE`
ensures a no-op on already-promoted edges, and the delta guard ensures a no-op on
edges whose weight has not drifted. Running the tick multiple times against the same
data state produces the same `GRAPH_EDGES` result.

**NFR-05 — Observability.** Tick results (inserted count, updated count) are emitted
via `tracing::info!`. Per-pair write failures are emitted via `tracing::warn!`. No
metrics counters or new instrumentation endpoints are introduced by this feature.

**NFR-06 — Module size.** `co_access_promotion_tick.rs` must stay under 500 lines per
workspace convention.

**NFR-07 — Compatibility.** No changes to the `GRAPH_EDGES` table schema, no new
migration, no changes to PPR or `TypedGraphState`. The feature is a pure behavioral
addition to the tick loop.

---

## Acceptance Criteria

**AC-01** — A `co_access` pair with `count >= CO_ACCESS_GRAPH_MIN_COUNT` that does not
yet have a `CoAccess` edge in `GRAPH_EDGES` is promoted to `GRAPH_EDGES` on the next
background tick after the count threshold is crossed.
_Verification: unit test — insert a qualifying pair into `co_access`, run the promotion
function, assert a matching row exists in `GRAPH_EDGES`._

**AC-02** — A `co_access` pair already present in `GRAPH_EDGES` as `CoAccess`, whose
normalized weight has changed by more than `CO_ACCESS_WEIGHT_UPDATE_DELTA` (0.1), has
its `weight` updated within one tick.
_Verification: unit test — insert a bootstrapped edge with a stale weight, run the
promotion function, assert the weight column is updated._

**AC-03** — A `co_access` pair already present in `GRAPH_EDGES` as `CoAccess`, whose
normalized weight delta is <= `CO_ACCESS_WEIGHT_UPDATE_DELTA`, produces zero database
writes (no UPDATE executed).
_Verification: unit test — assert `rows_affected == 0` for the UPDATE path when delta
is within threshold._

**AC-04** — When the number of qualifying pairs exceeds `max_co_access_promotion_per_tick`,
only the top-N pairs by `count DESC` are processed; lower-count pairs are skipped until
the next tick.
_Verification: unit test — seed more pairs than the cap, set cap to a small value,
assert only the N highest-count pairs appear in `GRAPH_EDGES` after one tick._

**AC-05** — The promotion step executes after `maintenance_tick` (which calls
`cleanup_stale_co_access`) and after orphaned-edge compaction, and before
`TypedGraphState::rebuild()` in `run_single_tick`.
_Verification: code review of `background.rs` — the call to `run_co_access_promotion_tick`
appears between orphaned-edge compaction and `TypedGraphState::rebuild()`._

**AC-06** — `InferenceConfig.max_co_access_promotion_per_tick` has a serde default of 200,
is validated to the range [1, 10000] by `InferenceConfig::validate()`, and participates
in project-level config merge with the same override semantics as
`max_graph_inference_per_tick`.
_Verification: unit tests in `config.rs` — (a) default deserializes to 200, (b) value 0
returns a validation error, (c) value 10001 returns a validation error, (d) project-level
override replaces global value._

**AC-07** — `CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3` is exported as a public constant from
`unimatrix-store` and is used by both the promotion tick and (via re-export or import)
the v13 migration, eliminating the duplicate literal.
_Verification: compile-time — the migration constant `CO_ACCESS_BOOTSTRAP_MIN_COUNT`
is either removed and replaced or set equal to `CO_ACCESS_GRAPH_MIN_COUNT`._

**AC-08** — `EDGE_SOURCE_CO_ACCESS: &str = "co_access"` is exported from `unimatrix-store`
alongside `EDGE_SOURCE_NLI`.
_Verification: unit test or doc test — the constant exists and has the correct value._

**AC-09** — When `co_access` is empty or no pairs meet `CO_ACCESS_GRAPH_MIN_COUNT`, the
promotion tick completes without error, without warning, and with a log line showing
0 inserted and 0 updated.
_Verification: unit test — run the function against an empty or sub-threshold `co_access`
table, assert no error and no warn! output._

**AC-10** — `InferenceConfig::validate()` rejects `max_co_access_promotion_per_tick = 0`
with a clear error message identifying the field name and the valid range.
_Verification: unit test — assert the validation error message contains the field name._

**AC-11** — When a single `write_pool_server()` call fails (simulated timeout or
constraint error), `run_co_access_promotion_tick` logs the error at `warn!`, continues
processing remaining pairs, and returns `()` without panicking.
_Verification: unit test with injected write failure — assert the function returns and
remaining pairs are attempted._

**AC-12** — All rows inserted by the promotion tick have `bootstrap_only = 0`,
`source = EDGE_SOURCE_CO_ACCESS`, `created_by = 'tick'`, `relation_type = 'CoAccess'`.
_Verification: unit test — assert column values on a freshly promoted row._

**AC-13** — Weight normalization uses `MAX(count)` computed over ALL qualifying pairs
(count >= threshold), not only the capped batch.
_Verification: this test is future-proofing documentation rather than a live correctness
discriminator. Because candidates are selected `ORDER BY count DESC`, the capped batch
always contains the highest-count pairs, so global MAX always equals batch MAX under
this SQL strategy. The test should be framed as: "given counts [1..10], cap=3, verify
that the SQL normalization anchor is computed via global scope (subquery over all
qualifying pairs) and not inlined as a Rust-side maximum of the fetched rows." Test
authors should verify the query shape, not just the output value. The requirement is
preserved for correctness if the SQL strategy ever changes._

**AC-14** — An existing `CoAccess` edge with `INSERT OR IGNORE` applied a second time
(pair already promoted) produces zero rows affected and no spurious UPDATE check.
_Verification: unit test — run the promotion tick twice against the same qualifying pair
with unchanged co_access count, assert only one edge row exists and `weight` is
unchanged._

**AC-15** — A `co_access` pair whose count has fallen below `CO_ACCESS_GRAPH_MIN_COUNT`
after promotion is not removed from `GRAPH_EDGES` by the promotion tick.
_Verification: unit test — promote a pair, drop its count below threshold, re-run the
tick, assert the `GRAPH_EDGES` row is still present._

---

## Domain Model

### Tables

**`co_access`** (source table, read-only from the perspective of this feature)
- `entry_id_a INTEGER` — lower entry ID of the pair (by convention: `entry_id_a < entry_id_b`)
- `entry_id_b INTEGER` — higher entry ID of the pair
- `count INTEGER` — number of times these two entries have been accessed together
- `last_access INTEGER` — epoch timestamp of most recent co-access event

**`graph_edges`** (sink table, write target for this feature)
- `id INTEGER PRIMARY KEY AUTOINCREMENT`
- `source_id INTEGER NOT NULL` — maps to `co_access.entry_id_a`
- `target_id INTEGER NOT NULL` — maps to `co_access.entry_id_b`
- `relation_type TEXT NOT NULL` — `'CoAccess'` for this feature
- `weight REAL NOT NULL DEFAULT 1.0` — normalized co-access strength [0.0, 1.0]
- `created_at INTEGER NOT NULL` — epoch timestamp
- `created_by TEXT NOT NULL DEFAULT ''` — `'tick'` for this feature
- `source TEXT NOT NULL DEFAULT ''` — `EDGE_SOURCE_CO_ACCESS = "co_access"`
- `bootstrap_only INTEGER NOT NULL DEFAULT 0` — always `0` for tick-promoted edges
- `metadata TEXT DEFAULT NULL` — unused by this feature
- `UNIQUE(source_id, target_id, relation_type)` — uniqueness constraint enabling `INSERT OR IGNORE` idempotency

### Constants

| Constant | Type | Value | Location | Purpose |
|----------|------|-------|----------|---------|
| `CO_ACCESS_GRAPH_MIN_COUNT` | `i64` | `3` | `unimatrix-store` (public) | Minimum co_access count to qualify for promotion; shared with v13 migration |
| `EDGE_SOURCE_CO_ACCESS` | `&str` | `"co_access"` | `unimatrix-store` (public) | Written to `graph_edges.source`; parallel to `EDGE_SOURCE_NLI` |
| `CO_ACCESS_WEIGHT_UPDATE_DELTA` | `f64` | `0.1` | `co_access_promotion_tick.rs` (module-private) | Minimum weight drift to trigger an UPDATE; suppresses churn; `f64` to match SQLite REAL fetch type (see ADR-003) |

### Config Fields

| Field | Type | Default | Range | Location |
|-------|------|---------|-------|----------|
| `max_co_access_promotion_per_tick` | `usize` | `200` | [1, 10000] | `InferenceConfig` in `config.rs` |

### Key Ubiquitous Language

- **Qualifying pair**: A `co_access` row where `count >= CO_ACCESS_GRAPH_MIN_COUNT`.
- **Promotion**: The act of inserting a qualifying `co_access` pair into `GRAPH_EDGES` as a `CoAccess` edge.
- **Weight refresh**: Updating an existing `CoAccess` edge's `weight` when the newly computed normalized weight differs from the stored weight by more than `CO_ACCESS_WEIGHT_UPDATE_DELTA`.
- **Global normalization**: Computing `MAX(count)` across all qualifying pairs (not just the capped batch) so that weights are consistent with the bootstrap scale and comparable across ticks.
- **Per-tick cap**: The `max_co_access_promotion_per_tick` limit on how many pairs are processed in a single tick invocation; candidates are selected in `count DESC` order.
- **Infallible tick contract**: All background tick functions have type `async fn ... -> ()`. Errors are logged at `warn!`, never propagated.
- **Structural idempotency**: Repeated tick execution against unchanged data produces no additional writes; `INSERT OR IGNORE` handles the insert path and the delta guard handles the update path.

### Tick Lifecycle (Relevant Excerpt)

The affected portion of `run_single_tick` in `background.rs`:

```
1. maintenance_tick()              -- calls cleanup_stale_co_access() inside StatusService
2. GRAPH_EDGES orphaned-edge compaction  -- DELETE WHERE endpoint not in entries
   >>> run_co_access_promotion_tick()  <<< INSERTED HERE (this feature)
3. TypedGraphState::rebuild()      -- reads GRAPH_EDGES, builds in-memory PPR graph
4. PhaseFreqTable::rebuild()
5. Contradiction scan (every N ticks)
6. extraction_tick()
7. maybe_run_bootstrap_promotion() -- crt-023: one-shot NLI bootstrap, idempotent
8. run_graph_inference_tick()      -- crt-029: recurring NLI Supports edge inference
```

The promotion tick must appear at the labeled position. Stale co_access rows are already
cleaned by step 1; orphaned edges are already removed by step 2; the promoted edges will
be immediately visible to PPR via step 3 within the same tick cycle.

---

## User Workflows

### Normal Operation (Background, No User Action Required)

1. Users query Unimatrix normally. The search pipeline writes co-access events to `co_access`.
2. On each background tick, `run_co_access_promotion_tick` selects qualifying pairs and
   promotes or refreshes their `GRAPH_EDGES` representation.
3. `TypedGraphState::rebuild()` picks up the newly promoted edges in the same tick.
4. PPR operates on the updated graph; co-access signal is reflected in re-ranking on the
   next query.

### Config Override (Operator)

An operator may set `max_co_access_promotion_per_tick` in the project-level config to
throttle or increase the per-tick budget. The setting takes effect on the next tick after
the server reads the updated config.

---

## Constraints

1. **No new schema migration.** The `GRAPH_EDGES` table schema is complete since v13.
   This feature adds no new columns, no new tables, and does not bump the schema version.

2. **Direct write pool path only.** `write_pool_server()` must be used for all writes.
   `AnalyticsWrite::GraphEdge` is shed-safe and INSERT-only; it cannot express conditional
   UPDATE semantics and must not be used for weight-refresh writes.

3. **Infallible tick contract.** The function signature is `async fn
   run_co_access_promotion_tick(...) -> ()`. No `Result` return. Errors are absorbed with
   `warn!` logging.

4. **No rayon pool.** Co_access promotion is pure SQL. No CPU-bound ML inference is
   involved. A rayon pool would add unnecessary overhead.

5. **No COUNTERS marker.** The COUNTERS-based idempotency marker (used by crt-023) is for
   one-shot bootstrap operations. This tick is explicitly recurring; idempotency is
   structural via `INSERT OR IGNORE` and the delta guard.

6. **No GC in this feature.** Sub-threshold `CoAccess` edges are not removed by this
   tick. GC is deferred to GH #409 (intelligence-driven retention).

7. **File size limit.** `co_access_promotion_tick.rs` must stay under 500 lines per
   workspace convention.

8. **One-directional edges.** v1 must match the bootstrap directionality: edges are
   written as `source_id = entry_id_a (min-id), target_id = entry_id_b (max-id)`. Writing
   reverse edges is a follow-up issue.

9. **SQL strategy is architecture's choice.** The spec requires: (a) global MAX(count)
   normalization, (b) candidates ordered by count DESC with LIMIT cap, (c) INSERT OR
   IGNORE semantics for new edges, (d) conditional UPDATE for weight refresh. Whether this
   is achieved via a two-query loop, a CTE, or a combined subquery is left to the architect
   (SR-01, SR-02 from risk assessment). The spec does not constrain the SQL shape beyond
   correctness and the direct-write-pool requirement.

---

## Dependencies

### Internal Crates
- `unimatrix-store` — `co_access` read queries, `graph_edges` write queries,
  `CO_ACCESS_GRAPH_MIN_COUNT`, `EDGE_SOURCE_CO_ACCESS`, `write_pool_server()`
- `unimatrix-server` — `InferenceConfig`, `background.rs` tick loop,
  `services/` module tree

### Existing Components
- `maintenance_tick()` / `cleanup_stale_co_access()` — must complete before the
  promotion tick runs (tick ordering constraint)
- Orphaned-edge compaction step in `run_single_tick` — must complete before the
  promotion tick runs
- `TypedGraphState::rebuild()` — must run after the promotion tick; this is what makes
  freshly promoted edges visible to PPR
- `EDGE_SOURCE_NLI` (entry #3591, col-029 ADR-001) — parallel constant; `EDGE_SOURCE_CO_ACCESS`
  is modeled on the same pattern
- `max_graph_inference_per_tick` in `InferenceConfig` — the exact throttle pattern that
  `max_co_access_promotion_per_tick` mirrors

### GH Issue Dependencies
- **GH #455** — tracking issue for this feature
- **GH #409 (blocking, ship-before)** — intelligence-driven retention / `co_access`
  pruning. crt-034 must be merged and deployed before #409 ships. If #409 prunes
  qualifying pairs before this promotion runs, co-access signal crossing the threshold
  is permanently lost with no error signal (SR-05, High severity).

---

## NOT in Scope

- Changes to the v12→v13 migration SQL.
- Changes to `AnalyticsWrite::GraphEdge` or the analytics drain (no new variant).
- Changes to `co_access` write paths or the co-access staleness cleanup logic.
- Changes to PPR, `TypedGraphState`, or downstream search scoring.
- Changes to `w_coac` or the fusion scoring formula (`w_coac` was zeroed in crt-032).
- Removal or modification of `bootstrap_only = 1` edges (handled by crt-023).
- GC of `CoAccess` edges whose source `co_access` pairs have since dropped below threshold
  (deferred to GH #409).
- Bidirectional edge promotion (min→max and max→min). v1 is one-directional to match
  the bootstrap. A follow-up issue should address bidirectionality.
- New MCP tool surface or API changes.
- New schema migration or schema version bump.

---

## Known Limitations

**CoAccess edge directionality (v1 limitation).** The bootstrap writes
`source_id = entry_id_a (min-id), target_id = entry_id_b (max-id)` — one direction only.
PPR traverses `Direction::Outgoing`, so seeding the min-id entry reaches the max-id entry
but not the reverse. For a symmetric co-access signal, half the traversal paths are absent:
seeding `entry_id_b` reaches nothing via CoAccess. This is the real gap. v1 must match the
bootstrap behavior for consistency (see ADR-006, Unimatrix #3828).

The follow-up issue must:
1. Write `(entry_id_b, entry_id_a, 'CoAccess')` for newly promoted pairs — distinct from
   the existing `(entry_id_a, entry_id_b, 'CoAccess')` row under the UNIQUE constraint.
2. Back-fill ALL bootstrap-era pairs that have only one direction. Bootstrap pairs are
   identifiable via `source = 'co_access'` and `created_by = 'bootstrap'`.

**Cycle detection is not affected.** When bidirectional CoAccess edges exist (A→B and
B→A), `is_cyclic_directed` on the full graph would false-positive as a cycle (Pattern
#2429). However, Unimatrix cycle detection uses a Supersedes-only temp graph — CoAccess
edges are excluded. Reverse CoAccess edges do not break cycle detection and the follow-up
requires no changes to that logic.

**Near-threshold pair re-evaluation each tick.** Pairs hovering near
`CO_ACCESS_GRAPH_MIN_COUNT` are re-evaluated on every tick. The `INSERT OR IGNORE`
no-op and the delta guard prevent actual writes in the steady state, but the pair
is fetched and checked. This is acceptable overhead given table size, but could be
optimized in a follow-up.

**Silent signal loss if #409 ships first.** If GH #409 (co_access pruning) is deployed
before crt-034, qualifying pairs that are pruned before the first promotion tick runs are
permanently lost — there is no error, no warning, and no recovery path. Mitigation:
enforce merge ordering at the GH milestone level.

---

## Open Questions for Architect

1. **SQL shape for INSERT + conditional UPDATE (SR-02).** The scope proposes a per-pair
   loop with `INSERT OR IGNORE` followed by a conditional `UPDATE`. The risk assessment
   notes a single CTE or `INSERT OR REPLACE` could reduce write-pool round-trips. The
   architect should decide and document as an ADR whether the two-step loop or a combined
   SQL statement is used. The spec requires correctness and the conditional-update
   semantics; the SQL shape is unconstrained.

2. **MAX(count) query placement (SR-01).** The spec requires global normalization
   (MAX over all qualifying pairs). The architect should decide whether this is a separate
   read-pool query or a subquery embedded in the batch fetch. Either is acceptable; the
   normalization correctness requirement is the binding constraint.

3. **Tick anchor comment (SR-06).** The risk assessment recommends a named comment anchor
   in `background.rs` to prevent future tick steps from inadvertently being inserted
   between orphaned-edge compaction and `TypedGraphState::rebuild()`. The architect should
   decide whether to add a structured comment block or rely on code review.

4. **AC-07 migration constant unification.** `CO_ACCESS_BOOTSTRAP_MIN_COUNT` in
   `migration.rs` is currently file-private. The spec requires the new public constant
   `CO_ACCESS_GRAPH_MIN_COUNT = 3` in `unimatrix-store`. Whether the migration constant
   is removed (replaced by an import) or left as a local alias is the architect's call —
   the requirement is that there is a single authoritative `= 3` literal.

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — found entries #3822 (promotion tick
  near-threshold idempotency pattern) and #3821 (GRAPH_EDGES tick write path and ordering
  pattern). Both were directly applicable and incorporated into AC-14, AC-15, and the
  constraints section. Also confirmed entry #3591 (EDGE_SOURCE_NLI naming convention)
  and #3785 (crt-032 w_coac zeroed, making crt-034 critical for PPR signal currency).
