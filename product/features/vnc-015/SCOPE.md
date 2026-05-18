# vnc-015: Typed Edge Write Path

## Problem Statement

Unimatrix's knowledge graph has typed edge infrastructure (GRAPH_EDGES table, RelationType enum, PPR,
graph_expand) but no agent-facing write path for typed edges. Agents cannot declare semantic
relationships such as "my ADR depends on this other ADR" or "this lesson motivates this decision."
The graph stays sparse because the only edges that currently populate it are auto-generated
(NLI-inferred Supports/Contradicts, CoAccess promotion, S1/S2 structural vocabulary). The full
PPR graph traversal value — serendipitous discovery, dependency tracing, Goal traceability — cannot
materialize without agent-declared typed edges.

This feature delivers two complementary write surfaces:

1. **`edges` parameter** on `context_store` and `context_correct` — declare edges at entry
   creation/correction time, attached to the newly created entry.
2. **`context_edge` tool** — standalone edge lifecycle management on existing entries: add a
   new relationship, remove one that no longer holds, or redirect one when a target entry is
   superseded. This is the tool the `edges` parameter cannot cover: updating edges without
   creating a new version of the source entry.

Together they close the full lifecycle. Also in scope: 10 new RelationType variants, bidirectional
Contradicts, `RelatedTo` PPR expansion, and stale-dependency observability.

No schema migration is required. The existing `write_graph_edge` function is the implementation
reference.

This is Phase 1 of the ASS-057 roadmap. Phase 2 (`context_graph` traversal tool) depends on this
feature having populated the graph.

## Goals

1. Expose an `edges: Option<Vec<EdgeInput>>` parameter on `context_store` and `context_correct`
   allowing agents to declare typed relationships at entry creation/correction time.
2. Add 10 new RelationType variants: `Advances`, `Cites`, `Asserts`, `Mentions`, `Refutes`,
   `Tests`, `DerivedFrom`, `Motivates`, `About`, `RelatedTo`.
3. Store `Contradicts` edges bidirectionally (both A→B and B→A in one atomic call) to match
   the CoAccess precedent and prevent asymmetric contradictions.
4. Validate target entries on all edge writes: target must exist and must not be quarantined.
   Deprecated targets are allowed — the `DependencyOnDeprecated` rule surfaces them. A declared
   edge to a non-existent or quarantined target is an unambiguous caller error; fail the call.
5. Add `stale_dependency_edges` count to `context_status` output: Prerequisite/depends_on
   edges whose source entry has status=Deprecated.
6. Add a `DependencyOnDeprecated` detection rule to `context_cycle_review` that fires when
   any Prerequisite edge in the current cycle's entries points to a deprecated source.
7. Gate all edge writes behind the existing `Capability::Write` gate — no new capability required.
8. Add `RelatedTo` to PPR positive edge types in `graph_ppr.rs` and to positive BFS traversal
   in `graph_expand.rs`. `RelatedTo` enables broad associative discovery immediately useful for
   serendipitous retrieval. The remaining 9 new variants (including `Advances` and `Motivates`)
   are write-only in this feature — traversal behavior deferred to Phase 2. Estimated ~6 lines.
9. Reject self-referential edges (`source_id == target_id`) at validation with a clear error.
   No valid semantic meaning exists for any variant reflexively applied.
10. Add a `context_edge` MCP tool for standalone edge lifecycle management on existing entries:

    ```
    context_edge(
      mode:         "add" | "remove" | "redirect",
      source_id:    u64,
      edge_type:    String,
      target_id:    u64,
      new_target_id: u64   // redirect only
    )
    ```

    - **add**: asserts a new typed relationship from an existing entry. Same target validation as Goal 4.
    - **remove**: retracts an existing relationship. For `Contradicts`, removes both directions atomically.
    - **redirect**: retargets an edge from `target_id` to `new_target_id` atomically (remove + add).
      For `Contradicts`, both directions are updated. Primary use case: supersession — Goal A corrected to
      A', authors of entries that `Advances → A` retarget without creating new versions of their entries.
    - **Pure graph operation**: no embedding recompute, no confidence update, no duplicate detection.
    - **Allowed edge types** (all 10 new variants + existing agent-meaningful variants):
      `Advances`, `Cites`, `Asserts`, `Mentions`, `Refutes`, `Tests`, `DerivedFrom`, `Motivates`,
      `About`, `RelatedTo`, `Prerequisite`, `Contradicts`, `Supports`.
    - **Blocked edge types with specific errors**:
      - `Supersedes` → `"Supersedes is not a graph relationship — use context_correct to create
        a new entry version"`. `Supersedes` is a correction chain mechanism, not a typed edge.
      - `CoAccess` → `"CoAccess is auto-generated from co-access patterns and cannot be
        agent-declared"`.
      - `Informs` → `"Informs is auto-generated and cannot be agent-declared"`.
11. Enforce source entry validity on `context_edge`: source entry must not be quarantined or
    deprecated. Editing edges on a frozen or invalid entry is rejected. No ownership check —
    `agent_id` is not a reliable ownership anchor in this RBAC model. Security gate is
    `Capability::Write` plus source entry status.

## Non-Goals

- No `context_edge` bulk/batch variant (single edge operation per call).
- No auto-retarget on supersession: `context_correct` does not cascade-redirect edges from the
  deprecated original. Agents call `context_edge(mode: "redirect")` explicitly. This keeps the
  correction chain and the graph mutation as intentional, separated operations.
- No `context_graph` traversal tool (Phase 2 — depends on this feature populating the graph).
- No schema migration. GRAPH_EDGES `relation_type TEXT NOT NULL` already accepts new string values.
- No PPR or graph_expand positive-type expansion beyond `RelatedTo`. All other 9 new variants
  (including `Advances` and `Motivates`) are write-only in this feature; traversal behavior
  deferred to Phase 2.
- No `context_batch_write` tool (OSC-6 in ASS-057: HNSW atomicity problem unsolved, 7–10 days).
- No edge auto-transfer when a dependency target is superseded (ASS-055 Q3: risky if A' changes
  the decision; agents re-assert explicitly on successor entries).
- No metadata column on entries (Thesis lifecycle gap from ASS-057 — separate migration decision).
- No NLI contradiction scoped to Claims only (Phase 3 intelligence work).
- No `context_graph` traversal modes (neighbors, subgraph, path, chain, inverse, filter).
- No config extension for `ppr_positive_edge_types` or `symmetric_edge_types` (design artifact).

## Background Research

### Existing graph write pattern (reference implementation)
`write_graph_edge` in `crates/unimatrix-server/src/services/nli_detection.rs:78–118` is the
authoritative write function. It executes `INSERT OR IGNORE` against `GRAPH_EDGES` via
`store.write_pool_server()`, is idempotent on the UNIQUE constraint
`(source_id, target_id, relation_type)`, and is already used 6 times by the NLI/cosine
pipeline. Agent-declared edges follow the same path with `source` set to a new constant
`EDGE_SOURCE_AGENT` (analogous to `EDGE_SOURCE_COSINE_SUPPORTS`).

### RelationType enum — current state
6 variants in `crates/unimatrix-engine/src/graph.rs:81–90`: `Supersedes`, `Contradicts`,
`Supports`, `CoAccess`, `Prerequisite`, `Informs`. Adding a new variant requires four
coordinated updates at two files (pattern #3950):
1. `graph.rs` enum body
2. `graph.rs` `as_str()` match arm
3. `graph.rs` `from_str()` match arm
4. (implicitly) `graph_ppr.rs` and `graph_expand.rs` if the new variant should be positive
   — but for this feature the new variants are write-only; traversal expansion is deferred.

A missing `from_str()` arm causes the R-10 guard in `build_typed_relation_graph` Pass 2b to
silently drop rows from any graph snapshot. All 10 new variants MUST have `from_str()` arms.

### PPR and graph_expand — no changes needed for write path
`positive_out_degree_weight` in `graph_ppr.rs:168–187` is hardcoded to 4 positive types
(Supports, CoAccess, Prerequisite, Informs). `graph_expand.rs:123–136` likewise hardcodes
these 4. The new variants will be written to GRAPH_EDGES but will NOT affect PPR mass flow or
BFS expansion in this feature. They will be present in the `TypedRelationGraph` but invisible
to scoring — this is an accepted gap, consistent with how `Contradicts` is currently handled.

### Bidirectional Contradicts — CoAccess precedent
`migration.rs:632–665` shows CoAccess stored bidirectionally. ASS-057 Section 2 confirms:
UNIQUE treats `(A,B)` and `(B,A)` as distinct tuples; `INSERT OR IGNORE` is idempotent per
direction. The implementation writes two `write_graph_edge` calls atomically in a single
handler transaction or within the same fire-and-forget sequence.

### Contradicts asymmetry bug
`query_contradicts_edges_for_entry` in `read.rs:1529–1532` uses `WHERE target_id = ?1` —
asymmetric, misses edges FROM an entry. Once bidirectional storage lands, this call site
should be updated to use `WHERE source_id = ?1`. This is a pre-existing bug; fixing it is
in scope as part of this feature.

### context_store handler structure (established pipeline)
`tools.rs:588–734` shows the 10-step pipeline for `context_store`:
identity → capability → validation → category → phase snapshot → title → NewEntry →
StoreService.insert → duplicate guard → confidence recompute → usage recording → format.
Edge writes insert as a new step after entry insert succeeds and after the duplicate guard
(step 7a). Edges must not be written for duplicate entries.

### context_correct handler structure
`tools.rs:827–912` shows the 9-step pipeline for `context_correct`:
identity → capability → validation → original_id extraction → original fetch → category
validation → title → NewEntry → StoreService.correct → confidence recompute → format.
Edge writes on `context_correct` attach to the NEW entry (the corrected successor), not the
deprecated original. They insert after `correct_result.corrected_entry` is available.

### Security model
Two separate surfaces, two different security profiles:

**`edges` param on context_store / context_correct:**
The source is always the new entry being created — `created_by` equals the calling agent by
construction. No ownership check needed (vacuously true). No confidence floor (the source is
new, initial confidence is always above any meaningful threshold). The `Capability::Write` gate
is sufficient.

Target validation is the real guard: `target_id` must reference an existing, non-quarantined
entry. This prevents edges to garbage or corrupt entries. Deprecated targets are allowed
(surfaced by `DependencyOnDeprecated`).

**`context_edge` tool (existing entries):**
`Capability::Write` is required. Source entry must not be quarantined or deprecated — editing
edges on frozen entries is rejected with `SourceFrozen`. No ownership check — `agent_id` is
not a reliable ownership anchor in this RBAC model.
Target validation rules apply identically: exists + not quarantined.

### stale_dependency_edges — no existing implementation
`GraphCohesionMetrics` in `read.rs:1751–1779` and `StatusAggregates` in `read.rs:1738–1744`
are the existing status data structures. `stale_dependency_edges` is a new scalar field added
to either `GraphCohesionMetrics` or a separate query in `compute_graph_cohesion_metrics()`.
SQL pattern follows existing JOIN-to-entries style: `WHERE relation_type='Prerequisite' AND
source_entry.status=1 (Deprecated)` on the GRAPH_EDGES JOIN entries join.

### DependencyOnDeprecated — detection rule structure
`DetectionRule` trait in `unimatrix-observe/src/detection/mod.rs:15`: `name()`, `category()`,
`detect(&[ObservationRecord]) -> Vec<HotspotFinding>`. Current 22 rules across 4 categories.
The new rule is a 23rd. It differs from existing rules in that it requires a Store query (to
check edge status), which the existing rules do not perform — they operate purely on
`ObservationRecord` slices. This architectural gap must be resolved in design: either the rule
receives pre-computed edge data as a parameter, or the rule framework is extended to allow
Store access. The ASS-057 blast radius estimate (~40 lines) assumed a Store-query approach
but did not resolve the interface mismatch.

### Write path is crate-local to unimatrix-server
`write_graph_edge` is `pub(crate)` in `nli_detection.rs`. The `context_store` and
`context_correct` handlers live in `tools.rs` within the same crate (`unimatrix-server`).
No visibility change required.

## Proposed Approach

**edges param on context_store / context_correct:**
Add `edges: Option<Vec<EdgeInput>>` to `StoreParams` and `CorrectParams` in `tools.rs`.
`EdgeInput` is a new struct `{ edge_type: String, target_id: u64 }`. Validation runs before
entry insert:
- Each `edge_type` must map to a known `RelationType` (unknown type → fail entire call)
- `source_id != target_id` (self-referential → fail entire call; source_id is the new entry's
  eventual ID — use a pre-insert check or validate after ID is assigned)
- Each `target_id` must reference an existing, non-quarantined entry (DB lookup → fail entire
  call if missing or quarantined; deprecated targets pass)
Any validation failure fails the entire call: no entry written, no edges written.

After insert and duplicate guard: call `write_graph_edge` per edge. For `Contradicts`, call
twice (A→B and B→A). Infrastructure errors on edge writes are logged but do not roll back the
entry.

For `context_correct`, edges attach to the new (corrected) entry. No auto-transfer.

**context_edge tool:**
New 13th MCP tool. Handler validates (6 steps):
1. `Capability::Write` gate
2. Source entry fetch (must exist)
3. Source entry status: not quarantined, not deprecated
4. `source_id != target_id` (self-referential rejection)
5. Target validation: `target_id` exists and is not quarantined

For `add` mode: call `write_graph_edge`. Idempotent (INSERT OR IGNORE).
For `remove` mode: `DELETE FROM GRAPH_EDGES WHERE source_id=? AND target_id=? AND relation_type=?`.
  For `Contradicts`: delete both `(A,B)` and `(B,A)` atomically.
For `redirect` mode: atomic remove of old target + add of new target.
  For `Contradicts`: all four rows managed atomically.
Pure graph operation — no embedding, no confidence recompute, no duplicate detection.

**stale_dependency_edges:** Add scalar to `GraphCohesionMetrics` via one SQL JOIN query.

**DependencyOnDeprecated:** Constructor injection — `context_cycle_review` handler pre-queries
stale Prerequisite data, injects at construction. No trait interface change.

**PPR expansion:** Add `RelatedTo` only to `positive_out_degree_weight` and BFS set. `Advances` and `Motivates` are write-only in this feature.

## Acceptance Criteria

### edges param (context_store / context_correct)
- AC-01: `context_store` accepts `edges: Option<Vec<{edge_type: String, target_id: u64}>>`. Omitting it is identical to current behavior (backward compatible).
- AC-02: `context_correct` accepts the same `edges` parameter; edges attach to the corrected (new) entry, not the deprecated original.
- AC-05: For each edge in the `edges` parameter, a row is inserted into GRAPH_EDGES with the correct `(source_id, target_id, relation_type)` triplet. `source` is set to `EDGE_SOURCE_AGENT` constant.
- AC-06: For `Contradicts` edges via the `edges` param, both `(A, B, Contradicts)` and `(B, A, Contradicts)` rows are inserted. Both rows are verifiable in the DB.
- AC-07: Target validation on all edge writes: if any target_id does not reference an existing entry, the entire call fails (no entry written, no edges written). If any target_id references a quarantined entry, the call fails. Deprecated targets are allowed.
- AC-08: Self-referential edge rejection: if any edge has `source_id == target_id`, the entire call fails with a clear error. Validation occurs before entry insert.
- AC-09: Edge writes for duplicate entries are suppressed. If `insert_result.duplicate_of.is_some()`, no edges are written and the duplicate response is returned.
- AC-10: `INSERT OR IGNORE` semantics: re-asserting an existing `(source_id, target_id, relation_type)` triplet is idempotent — no error, no duplicate row.

### context_edge tool
- AC-19: `context_edge` MCP tool exists with parameters `mode: "add"|"remove"|"redirect"`, `source_id: u64`, `edge_type: String`, `target_id: u64`, `new_target_id: u64` (required for redirect, rejected for add/remove).
- AC-20: `context_edge` is a pure graph operation: no embedding recompute, no confidence update, no duplicate detection is triggered.
- AC-21: `context_edge` requires `Capability::Write`. Unenrolled or read-only agents receive the existing permission error.
- AC-22: No ownership check on `context_edge`. `agent_id` is not a reliable ownership anchor in this RBAC model. Any `Capability::Write` agent may operate on any non-frozen source entry.
- AC-23: `context_edge` rejects operations on quarantined or deprecated source entries. Clear error returned.
- AC-24: `context_edge(mode: "add")` writes the edge with the same target validation as AC-07. Idempotent on re-assertion (INSERT OR IGNORE).
- AC-27: `context_edge` and the `edges` param both enforce an edge type allowlist. Allowed: all 10 new variants + `Prerequisite`, `Contradicts`, `Supports` (13 types). Attempting `Supersedes` returns an error message directing the caller to `context_correct`. Attempting `CoAccess` or `Informs` returns an error stating these are auto-generated. Unknown edge types return the existing "unknown relation type" error.
- AC-28: `Prerequisite` is a valid edge type for `context_edge` (all modes: add, remove, redirect). Agents can explicitly declare, retract, or retarget dependency relationships on existing entries.
- AC-25: `context_edge(mode: "remove")` deletes the `(source_id, target_id, relation_type)` row. For `Contradicts`, both `(A,B)` and `(B,A)` are deleted atomically. Returns success even if the edge did not exist (idempotent removal).
- AC-26: `context_edge(mode: "redirect")` atomically removes the old `(source_id, target_id)` edge and inserts `(source_id, new_target_id)`. For `Contradicts`, all four direction rows are managed atomically. New target validated per AC-07.

### RelationType variants
- AC-03: All 10 new RelationType variants are defined in `graph.rs` with `as_str()`, `from_str()`, and enum body entries: `Advances`, `Cites`, `Asserts`, `Mentions`, `Refutes`, `Tests`, `DerivedFrom`, `Motivates`, `About`, `RelatedTo`.
- AC-04: The existing 6 RelationType variants are unchanged. No regression in PPR, graph_expand, graph_penalty, or build_typed_relation_graph behavior.
- AC-14: All new edge types pass `build_typed_relation_graph` Pass 2b without being dropped by the R-10 guard (all 10 new variants have valid `from_str()` arms).
- AC-17: `RelatedTo` is added to `positive_out_degree_weight` in `graph_ppr.rs` and the positive BFS set in `graph_expand.rs`. PPR mass flows through `RelatedTo` edges after this feature ships. `Advances` and `Motivates` are NOT added to PPR in this feature.

### Observability
- AC-11: `context_status` output includes `stale_dependency_edges: u64` — count of GRAPH_EDGES rows where `relation_type='Prerequisite'` and the source entry has `status=1` (Deprecated).
- AC-12: `context_cycle_review` fires a `DependencyOnDeprecated` finding when any Prerequisite edge in the current cycle's entries points to a deprecated source. Severity: Warning. Rule name: `dependency_on_deprecated`.
- AC-13: `default_rules()` registers `DependencyOnDeprecated` as the 23rd rule. Test `test_default_rules_has_22_rules` updated to `23`.

### Existing fix
- AC-15: `Capability::Write` is required for all edge writes; no new capability is introduced.
- AC-16: `query_contradicts_edges_for_entry` in `read.rs` is updated to bidirectional query (both `source_id = ?1` and `target_id = ?1`).
- AC-18: Edge attribution uses `created_by` (agent_id). `&None` session_id in `context_correct` does not block; session tracking on edges is deferred.

## Constraints

- No schema migration. Schema stays at current version. `relation_type TEXT NOT NULL` in
  GRAPH_EDGES already accepts arbitrary strings; new variants are valid immediately.
- `write_graph_edge` is `pub(crate)` in `unimatrix-server`. The handlers that call it are in
  the same crate. No visibility change and no crate boundary crossing required.
- `DependencyOnDeprecated` rule must not perform blocking I/O in `detect()` — the current
  `DetectionRule` trait is synchronous. Store data must be injected at construction time
  (constructor injection pattern from `PhaseDurationOutlierRule`).
- `context_edge` is the 13th MCP tool. The tool count in any test that asserts exact tool count
  must be updated.
- `context_correct` passes `&None` for `session_id` (tools.rs:840). Edge attribution uses
  `created_by` (agent_id) — session tracking on edges is out of scope. The `&None` does not
  block the write path.
- Bidirectional Contradicts insert must be atomic in the sense that both rows are written in the
  same fire-and-forget sequence before the handler returns — they must not be split across
  background ticks.
- The `write_graph_edge` function uses `write_pool_server()` (single-connection serialization
  point). Multiple edge writes per call are sequential, not concurrent. For typical `edges`
  slice sizes (1–5 edges) this is acceptable; no batch optimization is needed.
- Max 500 lines per file (rust-workspace.md rule). `tools.rs` is already large; the edge-write
  logic should be extracted to a helper function, not inlined in each handler.

## Design Decisions (closed)

- **Confidence floor**: Dropped entirely. The source is always the newly-created entry (initial
  confidence well above any threshold). Vacuous check removed. Target validation replaces it as
  the quality gate.
- **Target validation**: Target must exist and not be quarantined. Deprecated targets allowed —
  `DependencyOnDeprecated` surfaces them. A declared edge to a non-existent or quarantined target
  is an unambiguous caller error; fail the entire call.
- **Failure posture**: Any validation failure (unknown type, self-referential, missing/quarantined
  target) fails the entire call — no entry written, no edges written. Consistent across all checks.
- **Source ownership on context_store/context_correct**: Dropped (vacuous — source is the new
  entry, owned by caller by construction). No real security provided.
- **context_edge tool added**: Standalone edge lifecycle (add/remove/redirect) on existing entries.
  No ownership check — `agent_id` is not a reliable anchor in this RBAC model. Security gate:
  `Capability::Write` + source entry status (not quarantined, not deprecated).
- **context_edge source validation**: Not quarantined, not deprecated. Editing edges on frozen
  entries is rejected with `SourceFrozen`.
- **No auto-transfer on context_correct**: Explicit re-declaration required. Edge absence is
  useful signal. Auto-retarget pollutes the correction chain with bookkeeping noise.
- **DependencyOnDeprecated Store access**: Constructor injection matching `PhaseDurationOutlierRule`.
  No change to the synchronous `DetectionRule` trait.
- **Self-referential edges**: Rejected. No valid semantic meaning for any variant.
- **PPR expansion**: `RelatedTo` only added to positive types in `graph_ppr.rs` and
  `graph_expand.rs`. `Advances` and `Motivates` are write-only in this feature — PPR expansion
  for directed semantic edge types deferred to Phase 2. All 9 remaining new variants are
  write-only.
- **context_edge allowed types**: 13 agent-meaningful variants allowed (all 10 new + `Prerequisite`,
  `Contradicts`, `Supports`). `Supersedes` blocked with redirect to context_correct. `CoAccess`
  and `Informs` blocked as auto-generated. This allowlist also applies to the `edges` param on
  context_store/context_correct — same validation, same errors.
- **session_id on context_correct**: Does not block. `created_by` used for edge attribution.

## Tracking

https://github.com/dug-21/unimatrix/issues/595
