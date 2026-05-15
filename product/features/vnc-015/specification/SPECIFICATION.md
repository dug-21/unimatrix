# SPECIFICATION: vnc-015 — Typed Edge Write Path

**Feature**: vnc-015
**Phase**: Vinculum
**Status**: Specification (Revised — Phase 2b scope deltas applied)
**Research basis**: ASS-055 (dependency tracking), ASS-057 (research domain fit analysis)

---

## Objective

vnc-015 delivers the agent-facing write path for typed graph edges in Unimatrix. It adds an
`edges: Option<Vec<EdgeInput>>` parameter to `context_store` and `context_correct`, defines 10
new `RelationType` variants covering the full SDLC and research domain taxonomy, and adds a
`context_edge` MCP tool for standalone edge lifecycle management (add, remove, redirect) on
existing entries. A target validation pipeline (unknown type, self-reference, missing target,
quarantined target) fails the entire call on any violation. Bidirectional `Contradicts` storage,
stale-dependency observability in `context_status`, and a `DependencyOnDeprecated` detection rule
in `context_cycle_review` complete the feature. No schema migration is required.

---

## Functional Requirements

### FR-01: EdgeInput Parameter on context_store

`context_store` (`StoreParams` in `tools.rs`) must accept an `edges: Option<Vec<EdgeInput>>`
parameter. `EdgeInput` is a struct with two fields: `edge_type: String` and `target_id: u64`,
derived with `Deserialize` and `JsonSchema`. Omitting the parameter (`None`) produces identical
behavior to the current implementation (backward compatible). An empty vec (`Some([])`) is treated
as no edges and must not cause any error or behavior change.

Testable: submit a `context_store` call with a valid `edges` list and confirm rows appear in
`GRAPH_EDGES`; submit without `edges` and confirm no behavioral change.

### FR-02: EdgeInput Parameter on context_correct

`context_correct` (`CorrectParams` in `tools.rs`) must accept the same `edges: Option<Vec<EdgeInput>>`
parameter with the same structure. Edges declared on `context_correct` attach to the new (corrected)
entry only — the source is the corrected entry's new `id` as returned by `StoreService.correct`.
No edges from the deprecated original are transferred or carried forward.

Testable: invoke `context_correct` with an `edges` list; confirm edge rows reference the new
entry id, not the deprecated entry id; confirm the deprecated entry has no new outgoing edges.

### FR-03: 10 New RelationType Variants

The following 10 new variants must be added to the `RelationType` enum in
`crates/unimatrix-engine/src/graph.rs`, each with all four mandatory change sites complete:

| Variant | `as_str()` output | Semantic meaning | Primary SDLC use | Primary research use |
|---|---|---|---|---|
| `Advances` | `"Advances"` | Source entry advances or contributes toward the target goal or strategic objective | Feature/Pattern/Decision → Goal | Finding/Thesis/POC → Goal |
| `Cites` | `"Cites"` | Source entry cites or references the target as a primary source | (none dominant) | Finding → Source |
| `Asserts` | `"Asserts"` | Source entry makes or contains the target claim | (none dominant) | Finding → Claim |
| `Mentions` | `"Mentions"` | Source entry mentions the target entity | Decision/Pattern → Concept | Finding/Thesis/Claim → Entity |
| `Refutes` | `"Refutes"` | Source entry provides evidence that contradicts or falsifies the target | Lesson → Pattern ("this lesson shows this pattern fails") | Claim → Thesis |
| `Tests` | `"Tests"` | Source entry tests or experimentally evaluates the target thesis or claim | (none dominant) | POC → Thesis |
| `DerivedFrom` | `"DerivedFrom"` | Source entry is derived from or originated in the target | Pattern → Feature | Insight → Finding/Claim/Thesis |
| `Motivates` | `"Motivates"` | Source entry is the motivation or rationale behind the target decision | Lesson → Decision ("this lesson is why this ADR was written") | Insight → Thesis |
| `About` | `"About"` | Source entry concerns or governs the target entity or concept | Decision/Pattern → Concept | Thesis → Entity |
| `RelatedTo` | `"RelatedTo"` | Weak semantic relatedness with no more specific type available | General fallback, bidirectional | General fallback |

Testable: all 10 variant string forms must parse successfully through `RelationType::from_str()`
and round-trip through `as_str()` without loss; all 10 must survive `build_typed_relation_graph`
Pass 2b without being dropped by the R-10 guard.

### FR-04: Four Mandatory Change Sites per Variant (SR-01)

Each of the 10 new RelationType variants requires exactly 4 coordinated code changes. Any missing
change causes a compile error (enum body) or silent row-drop (from_str). All four sites must be
updated atomically:

| Site | Location | Failure mode if missing |
|---|---|---|
| 1. Enum body | `graph.rs` enum declaration | Compile error |
| 2. `as_str()` match arm | `graph.rs` impl | Compile error (exhaustive match) |
| 3. `from_str()` match arm | `graph.rs` impl | Silent row-drop at Pass 2b R-10 guard |
| 4. PPR/BFS set (RelatedTo only) | `graph_ppr.rs`, `graph_expand.rs` | Invisible to traversal scoring |

The 10×4 compliance matrix (FR-10 has the full checklist). Sites 1, 2, and 3 apply to all 10
variants. Site 4 applies only to `RelatedTo`. `Advances` and `Motivates` are write-only in this
feature — their PPR semantics are deferred to Phase 2 (see ADR-006).

### FR-05: Validation Pipeline — Pre-Insert, All-or-Nothing

Before any entry is inserted into the store, all edges in the `edges` vec must be validated in
sequence. If any validation check fails, the entire `context_store` or `context_correct` call
must fail: no entry is written, no edges are written. The error must be returned to the caller
with a descriptive message identifying which edge failed and why.

Three validation checks, evaluated in this order for each edge:

1. **Unknown edge type**: `EdgeInput.edge_type` must map to a known `RelationType` variant via
   `RelationType::from_str()`. An unrecognized string fails the check.
2. **Self-referential edge**: The resolved `source_id` (the id of the entry being created/corrected)
   must not equal `EdgeInput.target_id`. No variant has valid reflexive semantics.
3. **Target validation**: Each `target_id` must reference an existing entry (DB lookup). If the
   entry does not exist, the entire call fails. If the entry has status Quarantined, the entire
   call fails. If the entry has status Deprecated, the call proceeds — deprecated targets are
   allowed and surfaced by the `DependencyOnDeprecated` rule.

All three failures cause the entire call to fail: no entry written, no edges written.

Testable: for each of the three failure modes, submit a call that triggers it; confirm the entire
call is rejected with no DB writes; confirm a corrected call succeeds.

### FR-06: Edge Write Placement in Handler Pipeline

Edge writes are inserted as step 7a in the `context_store` 10-step pipeline, after the duplicate
guard (step 7) and before confidence recompute (step 8). For duplicate entries
(`insert_result.duplicate_of.is_some()`), no edges are written and the duplicate response is
returned immediately.

For `context_correct`, edge writes occur after `correct_result.corrected_entry` is available,
using the new entry's id as `source_id`.

Testable: trigger a duplicate insert with edges; confirm no new GRAPH_EDGES rows appear; confirm
the duplicate response is returned normally.

### FR-07: Edge Write Mechanics

Each validated edge in the `edges` vec is written by calling `write_graph_edge` (the existing
`pub(crate)` function in `nli_detection.rs:78–118`). The `source` field is set to a new constant
`EDGE_SOURCE_AGENT` (analogous to `EDGE_SOURCE_COSINE_SUPPORTS`). Attribution uses
`created_by` (the calling agent's id). Session tracking on edge rows is deferred; the `&None`
`session_id` from `context_correct` does not block the write path.

`write_graph_edge` uses `INSERT OR IGNORE` against `GRAPH_EDGES` on the UNIQUE constraint
`(source_id, target_id, relation_type)`. Re-asserting an existing triplet is idempotent — no
error, no duplicate row.

`write_graph_edge` returns `bool` via `rows_affected() > 0` (SR-02). The edge-write loop must
key off the `bool` return, not treat the call as `Result`. The three-case contract:

| Return value | Meaning | Action |
|---|---|---|
| `true` | Row inserted | Continue |
| `false` | `INSERT OR IGNORE` hit UNIQUE — already exists | Continue (idempotent) |
| Err(_) | Infrastructure error | Log, do not roll back entry, do not surface to caller |

Testable: verify GRAPH_EDGES row exists after a write; verify re-assertion produces no error and
no duplicate row; verify `rows_affected()` return is checked.

### FR-08: Bidirectional Contradicts Insert

When an `EdgeInput` with `edge_type = "Contradicts"` is submitted, two rows must be written in
the same write sequence:
- `(source_id, target_id, "Contradicts")`
- `(target_id, source_id, "Contradicts")`

Both writes must occur before the handler returns (not deferred to a background tick). Both are
`INSERT OR IGNORE` and are individually idempotent. The CoAccess bidirectional pattern
(`migration.rs:632–665`) is the implementation precedent.

Note: `RelatedTo` is semantically symmetric but bidirectional storage for `RelatedTo` is at
the architect's discretion (SCOPE.md does not mandate it). `Contradicts` bidirectionality is
mandated by AC-06.

Testable: write a `Contradicts` edge and confirm both `(A, B)` and `(B, A)` rows exist in
`GRAPH_EDGES`.

### FR-09: Fix query_contradicts_edges_for_entry Asymmetry (AC-16)

`query_contradicts_edges_for_entry` in `read.rs:1529–1532` currently uses
`WHERE target_id = ?1` — asymmetric, misses edges FROM an entry. Once bidirectional storage is
in place, both `(A, B)` and `(B, A)` rows exist; the function must be updated to use
`WHERE source_id = ?1` (or a bidirectional query) to correctly surface all `Contradicts` edges
for a given entry.

SR-06 mandates that all existing callers of this function are identified and verified before
writing new tests. Existing tests that assert the old asymmetric behavior must be updated.

Testable: write a `Contradicts` edge from A to B; confirm `query_contradicts_edges_for_entry(A)`
returns the edge; confirm `query_contradicts_edges_for_entry(B)` also returns the edge.

### FR-10: Variant × Site Compliance Checklist

The following 10×4 matrix must be verified at Gate 3a. A missing cell is the highest-probability
implementation defect (SR-01). Verification method: grep for each variant string in each file.

| Variant | Enum body (graph.rs) | as_str() (graph.rs) | from_str() (graph.rs) | PPR/BFS (ppr+expand) |
|---|---|---|---|---|
| `Advances` | required | required | required | NO (write-only) |
| `Cites` | required | required | required | NO (write-only) |
| `Asserts` | required | required | required | NO (write-only) |
| `Mentions` | required | required | required | NO (write-only) |
| `Refutes` | required | required | required | NO (write-only) |
| `Tests` | required | required | required | NO (write-only) |
| `DerivedFrom` | required | required | required | NO (write-only) |
| `Motivates` | required | required | required | NO (write-only) |
| `About` | required | required | required | NO (write-only) |
| `RelatedTo` | required | required | required | YES |

### FR-11: RelatedTo PPR and graph_expand Inclusion

`RelatedTo` must be added to:
1. `positive_out_degree_weight` in `graph_ppr.rs:168–187` — the hardcoded set of 4 positive edge
   types (Supports, CoAccess, Prerequisite, Informs) becomes 5.
2. The positive BFS set in `graph_expand.rs:123–136` — same 4 types become 5.

`Advances` and `Motivates` are write-only in this feature. Their directed-edge PPR semantics
(which entry accumulates authority — the goal or the advancing entry?) require careful design
that is deferred to Phase 2. See ADR-006.

The remaining 8 new variants (`Cites`, `Asserts`, `Mentions`, `Refutes`, `Tests`, `DerivedFrom`,
`About`, `Advances`, `Motivates`) are write-only in this feature — they appear in `GRAPH_EDGES`
and in the `TypedRelationGraph` but have no effect on PPR mass flow or BFS expansion until
Phase 2.

Weight for `RelatedTo` in `positive_out_degree_weight`: same weight as the existing four
positive types (equal treatment, consistent with all prior positive-type additions).

Testable: after adding `RelatedTo` edges to the graph, run PPR seeded on an entry and confirm
mass flows through `RelatedTo` edges to associated entries; confirm the existing 4 positive type
behaviors are unchanged; confirm `Advances` and `Motivates` edges have no effect on PPR mass.

### FR-12: stale_dependency_edges in context_status

`context_status` output must include a new scalar field `stale_dependency_edges: u64`. This is
the count of rows in `GRAPH_EDGES` where:
- `relation_type = 'Prerequisite'`
- The source entry (JOIN on `entries.id = GRAPH_EDGES.source_id`) has `status = 1` (Deprecated)

The field is added to `GraphCohesionMetrics` or computed as a separate scalar in
`compute_graph_cohesion_metrics()` in `read.rs`. The query follows the existing JOIN-to-entries
style in that function.

Testable: write a `Prerequisite` edge; deprecate the source entry; call `context_status` and
confirm `stale_dependency_edges >= 1`.

### FR-13: DependencyOnDeprecated Detection Rule

A 23rd `DetectionRule` implementation named `DependencyOnDeprecated` must be added to
`unimatrix-observe/src/detection/`. Rule properties:

- `name()` returns `"dependency_on_deprecated"`
- `category()` returns the appropriate warning category
- `detect()` fires a `HotspotFinding` with severity `Warning` when any `Prerequisite` edge in
  the current cycle's entries points to a source entry with `status = Deprecated`

Because the `DetectionRule` trait is synchronous and must not perform blocking I/O in `detect()`,
the rule must receive pre-queried stale Prerequisite edge data via constructor injection —
following the `PhaseDurationOutlierRule` pattern. The `context_cycle_review` handler pre-queries
stale data from the Store and injects it at construction time. No change to the `DetectionRule`
trait interface is required.

SR-05 notes this is the first rule requiring injected Store data. The architect should define a
typed injection interface (not just an untyped `Vec` of pre-queried rows) to make the pattern
reusable for future rules.

`default_rules()` in `detection/mod.rs` must register `DependencyOnDeprecated` as the 23rd rule.
The existing test `test_default_rules_has_22_rules` must be updated to assert 23 rules.

Testable: write a `Prerequisite` edge; deprecate the source entry; invoke `context_cycle_review`;
confirm a `DependencyOnDeprecated` finding appears in the output with severity `Warning`.

### FR-14: Edge-Write Helper Extraction

The edge-write logic (validation loop, write_graph_edge calls, Contradicts bidirectional handling)
must be extracted into a dedicated helper function, not inlined in both `context_store` and
`context_correct` handlers. This is required by the 500-line limit on `tools.rs` (SR-04). The
target module for the helper is at the architect's discretion.

Testable: verify the helper function exists and is called from both handlers; verify `tools.rs`
does not exceed 500 lines after the change.

### FR-15: EDGE_SOURCE_AGENT Constant

A new named constant `EDGE_SOURCE_AGENT` must be added to the crate (analogous to
`EDGE_SOURCE_COSINE_SUPPORTS` in `col-029`). This constant is the value written to
`GRAPH_EDGES.source` for all agent-declared edges. The exact string value is at the architect's
discretion (e.g., `"agent"`), but it must be a named constant — not a magic string inline.

Testable: verify the constant exists; verify all agent-declared edge writes use this constant in
the `source` column.

### FR-16: context_edge MCP Tool

A 13th MCP tool `context_edge` must be added to `unimatrix-server`. It provides standalone edge
lifecycle management on existing entries — the operation that `edges` param on `context_store`
and `context_correct` cannot cover: updating edges without creating a new version of the source
entry.

**Parameters:**

| Parameter | Type | Required | Notes |
|---|---|---|---|
| `mode` | `"add"` \| `"remove"` \| `"redirect"` | Always | Determines operation |
| `source_id` | `u64` | Always | Existing entry id |
| `edge_type` | `String` | Always | Must parse via `RelationType::from_str()` |
| `target_id` | `u64` | Always | Current target entry id |
| `new_target_id` | `u64` | redirect only | New target entry id; rejected if present on add/remove |

**Validation pipeline (evaluated in order):**

1. `Capability::Write` gate — unenrolled or read-only agents receive the existing permission error
2. Source entry fetch — source entry must exist
3. Source entry status: source must not be quarantined; source must not be deprecated
4. Self-referential rejection: `source_id != target_id` (and `source_id != new_target_id` for redirect)
5. Edge type resolution: `edge_type` must parse via `RelationType::from_str()`
6. Target validation (add/redirect): `target_id` (and `new_target_id` for redirect) must exist and must not be quarantined; deprecated targets allowed

**Mode semantics:**

- **add**: Calls `write_graph_edge` with the given triplet. `INSERT OR IGNORE` — idempotent on
  re-assertion. Same target validation as FR-05 check 3.
- **remove**: `DELETE FROM GRAPH_EDGES WHERE source_id=? AND target_id=? AND relation_type=?`.
  For `Contradicts`, both `(A, B)` and `(B, A)` are deleted atomically (both directions in one
  DB operation or tight sequence before handler returns). Idempotent — success returned even if
  the edge did not exist.
- **redirect**: Atomically removes the old edge `(source_id, target_id, edge_type)` and inserts
  `(source_id, new_target_id, edge_type)`. For `Contradicts`, all four direction rows are managed
  atomically: remove `(A, B)`, remove `(B, A)`, insert `(A, B')`, insert `(B', A)`. New target
  validated per FR-05 check 3.

**Pure graph operation**: No embedding recompute, no confidence update, no duplicate detection is
triggered by `context_edge` under any mode.

**Error codes** (new, in addition to the existing permission error):

| Condition | Error |
|---|---|
| Source entry is quarantined or deprecated | `SourceFrozen` |
| `target_id` not found | `TargetNotFound` |
| `target_id` is quarantined | `TargetQuarantined` |

Testable: integration tests for each mode (add, remove, redirect); integration tests for each
validation failure; verify no embedding or confidence side-effects occur.

### FR-17: Target Validation on All Edge Write Surfaces

All three edge write surfaces — `edges` param on `context_store`, `edges` param on
`context_correct`, and `context_edge` — must enforce the same target validation rule:

- `target_id` must reference an existing entry → entire call fails if not found (`TargetNotFound`)
- `target_id` must not be quarantined → entire call fails if quarantined (`TargetQuarantined`)
- Deprecated `target_id` is allowed; the `DependencyOnDeprecated` detection rule surfaces these

This validation is the primary quality gate for the write path (replaces the dropped confidence
floor). Validation applies to every edge in the `edges` vec, and to all relevant target ids in
`context_edge` (target_id for add/remove/redirect; new_target_id for redirect).

Testable: for each write surface, submit an edge to a non-existent id (fails), to a quarantined id
(fails), and to a deprecated id (succeeds). Confirm no writes occur on failure.

---

## Non-Functional Requirements

### NFR-01: Backward Compatibility

Omitting the `edges` parameter on `context_store` or `context_correct` must produce bit-for-bit
identical behavior to the current implementation. No change in response structure, no change in
entry write semantics, no change in timing or error codes for calls that do not use `edges`.

### NFR-02: No Schema Migration

The `GRAPH_EDGES` table uses `relation_type TEXT NOT NULL`. New RelationType variant strings are
valid immediately. Schema stays at its current version. No migration script is required or
permitted.

### NFR-03: Idempotent Edge Writes

Re-asserting an identical `(source_id, target_id, relation_type)` triplet must be a no-op — no
error, no duplicate row. `INSERT OR IGNORE` provides this guarantee. Both directions of a
`Contradicts` edge are independently idempotent.

### NFR-04: Validation Before Any Write

All validation (unknown type, self-reference, target validation) must complete before the entry
insert begins. A call that fails validation must not produce any side-effects in any table.

### NFR-05: Sequential Edge Writes, No Batch Optimization

`write_graph_edge` uses `write_pool_server()` (single-connection serialization point). Multiple
edge writes per call are sequential. For typical `edges` slice sizes (1–5 edges), this is
acceptable. No batch optimization or concurrent write path is required.

### NFR-06: Partial-Write Blast Radius

If the entry insert succeeds but a subsequent edge write fails (infrastructure error), the entry
is NOT rolled back. The infrastructure error is logged. The caller is not notified of edge write
failures. This is the accepted partial-write contract for this feature (aligned with SCOPE.md
Proposed Approach).

This is a design-level decision with implications for data integrity. The architect must document
the partial-write blast radius explicitly. SR-03 flags this as a medium-severity risk.

### NFR-07: tools.rs Line Count

`tools.rs` must not exceed 500 lines after this feature. The edge-write helper extraction
(FR-14) is the mechanism for compliance. The architect must verify the current line count before
design.

### NFR-08: Capability Gate — No New Capability

`Capability::Write` is the required capability for both existing tools (`context_store`,
`context_correct`) and the new `context_edge` tool. No new capability is introduced. The existing
Write gate extends naturally to all edge-write code paths. Unenrolled or read-only agents receive
the existing permission error.

### NFR-09: context_edge is a Pure Graph Operation

`context_edge` must not trigger embedding recompute, confidence update, or duplicate detection
under any mode (add, remove, redirect). The only side-effect is mutation of `GRAPH_EDGES` rows.

### NFR-10: Contradicts Direction Management is Atomic

For all surfaces that produce Contradicts edges (add, remove, redirect via `context_edge`; insert
via `edges` param), both direction rows must be managed within the same write sequence before the
handler returns. They must not be split across background ticks or deferred.

---

## Acceptance Criteria

### AC-01: context_store edges parameter
**Requirement**: FR-01
`context_store` accepts `edges: Option<Vec<{edge_type: String, target_id: u64}>>`. Omitting the
parameter produces identical behavior to current.
**Verification**: Integration test — call without `edges`, compare response and DB state to
baseline; call with `edges`, confirm GRAPH_EDGES rows written.

### AC-02: context_correct edges parameter
**Requirement**: FR-02
`context_correct` accepts the same `edges` parameter. Edges attach to the new (corrected) entry.
No edges from the deprecated original are transferred.
**Verification**: Integration test — correct an entry with `edges`; confirm GRAPH_EDGES rows
reference new entry id; confirm deprecated entry has no new outgoing edges.

### AC-03: 10 new RelationType variants defined
**Requirement**: FR-03, FR-04
All 10 variants (`Advances`, `Cites`, `Asserts`, `Mentions`, `Refutes`, `Tests`, `DerivedFrom`,
`Motivates`, `About`, `RelatedTo`) are present in the `graph.rs` enum body with `as_str()` and
`from_str()` arms.
**Verification**: Unit tests for each variant: `from_str(v.as_str()) == Ok(v)` round-trip; grep
verification of all 4 change sites per variant.

### AC-04: Existing 6 RelationType variants unchanged
**Requirement**: FR-03 (regression)
`Supersedes`, `Contradicts`, `Supports`, `CoAccess`, `Prerequisite`, `Informs` — unchanged. No
regression in PPR, graph_expand, graph_penalty, or `build_typed_relation_graph` behavior.
**Verification**: All existing graph-related tests pass without modification.

### AC-05: GRAPH_EDGES row written per edge
**Requirement**: FR-07
Each validated edge produces a `GRAPH_EDGES` row with the correct `(source_id, target_id,
relation_type)` triplet. `source = EDGE_SOURCE_AGENT` (`"agent"`). `created_by = EDGE_SOURCE_AGENT`
(`"agent"`). Agent identity is traceable via `ENTRIES.created_by` on the source entry (JOIN).
**Verification**: Integration test — store entry with edges; query GRAPH_EDGES; assert row fields.

### AC-06: Bidirectional Contradicts
**Requirement**: FR-08
For a `Contradicts` edge, both `(A, B, Contradicts)` and `(B, A, Contradicts)` rows are present
in GRAPH_EDGES after the call. Both rows were written before the handler returned.
**Verification**: Integration test — store entry with `Contradicts` edge to target B; query
GRAPH_EDGES for both direction rows; assert both present.

### AC-07: Target validation rejects entire call on missing or quarantined target
**Requirement**: FR-05, FR-17
If any target_id does not reference an existing entry, the entire call fails (no entry written,
no edges written). If any target_id references a quarantined entry, the call fails. Deprecated
targets are allowed — the call proceeds normally.
**Verification**: Integration test (three cases):
1. Write edge to a non-existent id → call fails, no DB writes
2. Write edge to a quarantined entry id → call fails, no DB writes
3. Write edge to a deprecated entry id → call succeeds, GRAPH_EDGES row written

### AC-08: Self-referential edge rejected
**Requirement**: FR-05
If any edge has `source_id == target_id`, the entire call fails before entry insert. Clear error
returned. Applies to both `edges` param on `context_store`/`context_correct` and to
`context_edge` (all modes).
**Verification**: Integration test — submit `context_store` with `target_id` equal to the
would-be source id (known ahead of time in test setup); assert call fails; assert no entry
inserted. Separate test for `context_edge`.

### AC-09: Duplicate entries — no edge writes
**Requirement**: FR-06
If `insert_result.duplicate_of.is_some()`, no edges are written. Duplicate response is returned.
**Verification**: Integration test — re-insert an existing entry with `edges`; confirm
GRAPH_EDGES row count unchanged; confirm duplicate response returned.

### AC-10: INSERT OR IGNORE idempotency
**Requirement**: FR-07
Re-asserting an identical `(source_id, target_id, relation_type)` triplet is a no-op.
**Verification**: Integration test — write the same edge twice; query GRAPH_EDGES; assert exactly
1 row for that triplet; assert no error on second write.

### AC-11: stale_dependency_edges in context_status
**Requirement**: FR-12
`context_status` output includes `stale_dependency_edges: u64` counting `Prerequisite` edges
whose source entry is Deprecated.
**Verification**: Integration test — write Prerequisite edge; deprecate source; call
`context_status`; assert `stale_dependency_edges >= 1`.

### AC-12: DependencyOnDeprecated finding fires
**Requirement**: FR-13
`context_cycle_review` emits a `DependencyOnDeprecated` finding (severity Warning, rule name
`dependency_on_deprecated`) when a Prerequisite edge in the current cycle's entries points to a
Deprecated source.
**Verification**: Integration test — setup as AC-11; call `context_cycle_review`; assert finding
present with correct rule name and severity.

### AC-13: 23rd rule registered
**Requirement**: FR-13
`default_rules()` registers `DependencyOnDeprecated`. `test_default_rules_has_22_rules` updated
to assert 23 rules.
**Verification**: Unit test — `default_rules().len() == 23`.

### AC-14: All 10 variants survive Pass 2b
**Requirement**: FR-03, FR-04
All 10 new variant string forms are accepted by `RelationType::from_str()` and therefore survive
the R-10 guard in `build_typed_relation_graph` Pass 2b without being silently dropped.
**Verification**: Unit test per variant — write a GRAPH_EDGES row with the variant string; call
`build_typed_relation_graph`; assert the edge appears in the resulting `TypedRelationGraph`.

### AC-15: Write capability required
**Requirement**: NFR-08
Unenrolled or read-only agents receive the existing permission error when calling `context_store`,
`context_correct`, or `context_edge`.
**Verification**: Existing capability gate tests cover `context_store` and `context_correct`
implicitly. New test confirms `context_edge` rejects agents without `Capability::Write`.

### AC-16: query_contradicts_edges_for_entry bidirectional
**Requirement**: FR-09
`query_contradicts_edges_for_entry` in `read.rs` uses a bidirectional query (both `source_id =
?1` and `target_id = ?1` covered). All existing callers verified to handle both directions.
**Verification**: Integration test — write `Contradicts` edge A→B (bidirectional); call
`query_contradicts_edges_for_entry(A)` and `query_contradicts_edges_for_entry(B)`; assert both
return the edge. Existing test inventory confirms no false-pass callers.

### AC-17: RelatedTo in PPR and graph_expand
**Requirement**: FR-11
`RelatedTo` appears in `positive_out_degree_weight` (graph_ppr.rs) and the positive BFS set
(graph_expand.rs). PPR mass flows through `RelatedTo` edges after the feature ships. `Advances`
and `Motivates` are NOT added to PPR in this feature (write-only; deferred to Phase 2).
**Verification**: Unit test — build a TypedRelationGraph with `RelatedTo` edges; run PPR seeded
on entry; assert associated entries surface with positive mass; assert `Advances` and `Motivates`
edges do not affect PPR mass; assert existing 4 positive types unchanged.

### AC-18: Edge attribution uses created_by
**Requirement**: FR-07, FR-15
Agent-declared edges use `created_by` (agent_id) for attribution. The `&None` session_id in
`context_correct` does not block the write path. `EDGE_SOURCE_AGENT` constant used in `source`
column.
**Verification**: Integration test — store entry with edges via an enrolled agent; query
GRAPH_EDGES; assert `created_by` matches the calling agent id; assert `source` matches
`EDGE_SOURCE_AGENT`.

### AC-19: context_edge tool exists with correct parameter signature
**Requirement**: FR-16
`context_edge` is the 13th MCP tool with parameters `mode`, `source_id`, `edge_type`,
`target_id`, and `new_target_id` (redirect only; rejected on add/remove).
**Verification**: Tool registration test confirms 13 tools. Schema inspection confirms parameter
types. Integration test: submit add/remove/redirect calls and confirm correct behavior for each.

### AC-20: context_edge is a pure graph operation
**Requirement**: FR-16, NFR-09
`context_edge` does not trigger embedding recompute, confidence update, or duplicate detection
under any mode.
**Verification**: Integration test — call `context_edge(add)`; confirm no embedding job queued,
no confidence delta, no duplicate detection log entry.

### AC-21: context_edge requires Capability::Write
**Requirement**: FR-16, NFR-08
Unenrolled or read-only agents calling `context_edge` receive the existing permission error.
**Verification**: Integration test — call `context_edge` with an agent missing `Capability::Write`;
assert permission error returned.

### AC-22: No ownership check on context_edge
**Requirement**: FR-16
No ownership check is performed on `context_edge`. `agent_id` is not a reliable ownership anchor
in this RBAC model. Any `Capability::Write` agent may operate on any non-frozen source entry.
The security gate is `Capability::Write` plus source entry status (not quarantined, not deprecated).
**Verification**: Integration test — enroll two agents A and B; agent A stores entry; agent B
calls `context_edge` on that entry with `Capability::Write`; assert the operation succeeds.

### AC-23: context_edge rejects frozen sources
**Requirement**: FR-16
`context_edge` rejects operations where the source entry is quarantined or deprecated. `SourceFrozen`
error returned in both cases.
**Verification**: Integration test — quarantine an entry; call `context_edge` on it; assert
`SourceFrozen`. Repeat for deprecated entry.

### AC-24: context_edge add mode writes edge with target validation
**Requirement**: FR-16, FR-17
`context_edge(mode: "add")` writes the edge row. Idempotent on re-assertion. Target validation
applies (non-existent target fails, quarantined target fails, deprecated target allowed).
**Verification**: Integration test — add edge to valid target; confirm GRAPH_EDGES row. Add to
non-existent id; confirm `TargetNotFound`. Add to quarantined entry; confirm `TargetQuarantined`.
Add to deprecated entry; confirm success. Re-assert same edge; confirm no error, no duplicate row.

### AC-25: context_edge remove mode deletes edge idempotently
**Requirement**: FR-16
`context_edge(mode: "remove")` deletes the `(source_id, target_id, relation_type)` row. For
`Contradicts`, both `(A, B)` and `(B, A)` are deleted atomically. Returns success even if the
edge does not exist.
**Verification**: Integration test — add a `Contradicts` edge; remove it; confirm both direction
rows deleted. Call remove again on the same edge; confirm success (idempotent). Confirm
non-Contradicts remove deletes only the specified row (not the reverse).

### AC-26: context_edge redirect mode atomically retargets edge
**Requirement**: FR-16
`context_edge(mode: "redirect")` removes the old `(source_id, target_id)` edge and inserts
`(source_id, new_target_id)`. For `Contradicts`, all four direction rows managed atomically.
New target validated per AC-07.
**Verification**: Integration test — add edge A→B; redirect to B'; confirm A→B deleted, A→B'
present. For Contradicts: add A↔B; redirect to B'; confirm all four rows updated. Redirect to
non-existent id; confirm `TargetNotFound`, original rows unchanged.

---

## Domain Model

### EdgeInput

```
EdgeInput {
    edge_type: String   // Must parse via RelationType::from_str(); case-sensitive
    target_id: u64      // Target entry id; must not equal resolved source_id
}
```

Defined inline in `tools.rs` (or a types module at architect's discretion). Derived:
`Deserialize`, `JsonSchema`.

### EdgeParams (context_edge)

```
EdgeParams {
    mode:          "add" | "remove" | "redirect"
    source_id:     u64
    edge_type:     String    // Must parse via RelationType::from_str()
    target_id:     u64
    new_target_id: u64       // Required for redirect; rejected for add/remove
}
```

### RelationType Enum (complete, post-feature)

16 variants total. All must have enum body, `as_str()`, and `from_str()` defined in
`crates/unimatrix-engine/src/graph.rs`.

**Existing 6 variants (unchanged):**

| Variant | `as_str()` | Storage direction | PPR/BFS positive |
|---|---|---|---|
| `Supersedes` | `"Supersedes"` | A→B (A supersedes B) | No |
| `Contradicts` | `"Contradicts"` | A↔B (bidirectional) | No |
| `Supports` | `"Supports"` | A→B (A supports B) | Yes |
| `CoAccess` | `"CoAccess"` | A↔B (bidirectional) | Yes |
| `Prerequisite` | `"Prerequisite"` | A→B (A is prereq of B) | Yes |
| `Informs` | `"Informs"` | A→B | Yes |

**10 new variants (this feature):**

| Variant | `as_str()` | Storage direction | PPR/BFS positive |
|---|---|---|---|
| `Advances` | `"Advances"` | A→B | No (write-only; Phase 2) |
| `Cites` | `"Cites"` | A→B | No |
| `Asserts` | `"Asserts"` | A→B | No |
| `Mentions` | `"Mentions"` | A→B | No |
| `Refutes` | `"Refutes"` | A→B | No |
| `Tests` | `"Tests"` | A→B | No |
| `DerivedFrom` | `"DerivedFrom"` | A→B | No |
| `Motivates` | `"Motivates"` | A→B | No (write-only; Phase 2) |
| `About` | `"About"` | A→B | No |
| `RelatedTo` | `"RelatedTo"` | A→B (bidirectionality at architect's discretion) | Yes (added this feature) |

### Validation Failure Modes and Error Codes

| Failure mode | Trigger condition | Behavior | Error message guidance |
|---|---|---|---|
| `UnknownEdgeType` | `RelationType::from_str(edge_type)` returns `Err` | Reject entire call; no writes | "Unknown edge type '{type}' — valid types are: [list]" |
| `SelfReferentialEdge` | `source_id == target_id` | Reject entire call; no writes | "Self-referential edge rejected: source_id equals target_id ({id})" |
| `TargetNotFound` | `target_id` does not reference an existing entry | Reject entire call; no writes | "Target entry {id} not found" |
| `TargetQuarantined` | `target_id` has status Quarantined | Reject entire call; no writes | "Target entry {id} is quarantined and cannot be referenced" |
| `SourceFrozen` | Source entry is quarantined or deprecated (context_edge only) | Reject call | "Source entry {id} is frozen (quarantined or deprecated)" |
| `DuplicateEntry` | `insert_result.duplicate_of.is_some()` | Suppress edge writes; return duplicate response | (not an error — normal duplicate response) |
| `InfrastructureEdgeFailure` | `write_graph_edge` returns `Err` | Log error; do not roll back entry; do not surface to caller | (logged internally only) |

Error codes: use the existing MCP error code convention. The exact numeric codes for
`TargetNotFound`, `TargetQuarantined`, and `SourceFrozen` are at the architect's discretion,
consistent with existing patterns.

### Ubiquitous Language

- **EdgeInput**: The user-facing struct declaring a typed relationship at entry creation time.
- **EdgeParams**: The user-facing struct for `context_edge` standalone edge lifecycle operations.
- **EDGE_SOURCE_AGENT**: Named constant identifying agent-declared edges in GRAPH_EDGES (as
  opposed to NLI-inferred, CoAccess-promoted, or structural vocabulary edges).
- **Target validation**: The check that a `target_id` references an existing, non-quarantined
  entry. The primary quality gate for all edge write surfaces.
- **Bidirectional write**: Inserting both `(A, B, type)` and `(B, A, type)` for symmetric edge
  types. Required for `Contradicts` in this feature.
- **SourceFrozen**: An error state where the source entry is quarantined or deprecated, making it
  ineligible for edge mutation via `context_edge`.
- **redirect**: The `context_edge` mode that retargets an existing edge to a new entry without
  creating a new version of the source entry. Primary use case: supersession.
- **stale_dependency_edges**: Count of GRAPH_EDGES rows where `relation_type = 'Prerequisite'`
  and the source entry has `status = Deprecated`. Surfaces unreviewed dependency staleness.
- **Write-only variant**: A new RelationType variant that can be written to GRAPH_EDGES but whose
  traversal scoring behavior (PPR weight, graph_expand BFS inclusion) is deferred to Phase 2.

---

## User Workflows

### Workflow 1: Declare edges at entry creation

Agent calls `context_store` with content and an `edges` list:

```
context_store(
  title: "ADR-022: Use SQLite for embedded storage",
  content: "...",
  category: "decision",
  edges: [
    { edge_type: "Prerequisite", target_id: 101 },  // depends on ADR-015
    { edge_type: "Advances",     target_id: 5   },  // advances strategic Goal-5
    { edge_type: "Motivates",    target_id: 88  }   // motivated by Lesson-88
  ]
)
```

The validation pipeline runs before insert. All three edges pass target validation. Entry is
inserted. Three GRAPH_EDGES rows are written. All three edges are stored but `Advances` and
`Motivates` are write-only in this feature — they will not flow through PPR until Phase 2.
`RelatedTo` edges, had any been declared, would participate in PPR immediately.

### Workflow 2: Correct an entry and re-declare relationships

Agent calls `context_correct` on an ADR whose dependencies have changed:

```
context_correct(
  original_id: 200,
  title: "ADR-022 v2: ...",
  content: "...",
  edges: [
    { edge_type: "Prerequisite", target_id: 101 },
    { edge_type: "Prerequisite", target_id: 105 },  // new dependency
    { edge_type: "Advances",     target_id: 5   }
  ]
)
```

Old entry (id=200) is deprecated. New entry (id=201) is created. Edges attach to id=201 only.
The old `Prerequisite` edge from id=200 to id=101 is not transferred and remains in GRAPH_EDGES
pointing from the deprecated entry.

### Workflow 3: Observe stale dependencies

Agent calls `context_status` after an ADR has been deprecated:

Response includes `stale_dependency_edges: 3`. The agent knows 3 Prerequisite edges point from
deprecated source entries — signaling that dependent decisions may need review.

### Workflow 4: Cycle review detects stale dependency

During a feature cycle, `context_cycle_review` returns a finding:

```
{
  rule: "dependency_on_deprecated",
  severity: "Warning",
  message: "Entry 201 ('ADR-022 v2') has a Prerequisite edge to entry 101 which is Deprecated."
}
```

Agent reviews and either re-asserts on the successor entry or accepts the stale state.

### Workflow 5: Supersession retargeting via context_edge redirect

Goal A (entry id=50) is superseded: an agent calls `context_correct` on it, producing A' (entry
id=51). Entry id=50 is now deprecated. Several entries in the knowledge base had declared
`Advances → A` (target_id=50). Any agent with `Capability::Write` may retarget these
relationships to A' without creating new versions of the source entries.

An agent calls:

```
context_edge(
  mode:          "redirect",
  source_id:     <their entry id>,
  edge_type:     "Advances",
  target_id:     50,
  new_target_id: 51
)
```

The old `(source_id, 50, Advances)` row is deleted. A new `(source_id, 51, Advances)` row is
inserted. The author's entry is unchanged — no new version required. The graph now correctly
reflects that those entries advance Goal A' rather than the deprecated Goal A.

This is the primary motivating use case for `context_edge`. The `edges` param on `context_store`
and `context_correct` cannot perform this operation because they always create a new entry version
as a side-effect.

---

## Constraints

1. **No schema migration.** `GRAPH_EDGES.relation_type TEXT NOT NULL` accepts new string values
   immediately. Schema version unchanged.

2. **context_edge is the 13th MCP tool.** Any test asserting an exact tool count must be updated.

3. **pub(crate) visibility on write_graph_edge.** The function is `pub(crate)` in
   `nli_detection.rs` within `unimatrix-server`. All calling code is in the same crate. No
   visibility change required.

4. **DetectionRule trait is synchronous.** `detect()` must not perform blocking I/O. Store data
   must be injected at construction time via the constructor injection pattern from
   `PhaseDurationOutlierRule`.

5. **context_correct session_id is &None.** Edge attribution uses `created_by` (agent_id).
   Session tracking on edges is out of scope and does not block the write path.

6. **Bidirectional Contradicts writes are fire-and-forget sequential, not a DB transaction.**
   Both rows are written before the handler returns, within the same write sequence. They are not
   wrapped in an explicit transaction boundary unless the architect decides otherwise (SR-03).

7. **tools.rs 500-line limit.** Edge-write logic must be extracted to a helper, not inlined in
   both handlers. Current line count must be verified before design begins.

8. **PPR and graph_expand positive type expansion is limited to RelatedTo.** All 9 remaining
   new variants (including `Advances` and `Motivates`) are write-only in this feature. Their
   traversal scoring behavior is deferred to Phase 2 (context_graph). See ADR-006.

9. **write_graph_edge returns bool, not Result.** The edge-write loop must key off the bool
   return value (SR-02). The three-case contract (true=inserted, false=ignored, Err=infra) is
   defined in FR-07.

10. **No confidence floor.** The confidence floor check and `StoreConfig.edge_confidence_floor`
    are not implemented. Target validation (FR-05 check 3, FR-17) is the sole quality gate for
    edge writes.

---

## Dependencies

### Crates

| Crate | Dependency type | Usage |
|---|---|---|
| `unimatrix-engine` | Modified | `RelationType` enum in `graph.rs` — 10 new variants |
| `unimatrix-server` | Modified | `tools.rs` (StoreParams, CorrectParams, context_edge handler), edge-write helper, `EDGE_SOURCE_AGENT` constant |
| `unimatrix-store` | Modified | `read.rs` — `stale_dependency_edges` query in `compute_graph_cohesion_metrics`; target validation DB lookups |
| `unimatrix-observe` | Modified | `detection/` — new `DependencyOnDeprecated` rule |
| `nli_detection.rs` (in unimatrix-server) | Reused | `write_graph_edge` function — reference implementation for all edge writes |

### External Services

None. All changes are within the Rust workspace.

### Existing Components Referenced

- `write_graph_edge` — `nli_detection.rs:78–118` (reference implementation, no modification)
- `EDGE_SOURCE_COSINE_SUPPORTS` — naming precedent for `EDGE_SOURCE_AGENT` constant
- `GraphCohesionMetrics` / `compute_graph_cohesion_metrics` — `read.rs:1751–1779` (modified)
- `DetectionRule` trait — `unimatrix-observe/src/detection/mod.rs:15`
- `PhaseDurationOutlierRule` — constructor injection pattern reference
- `positive_out_degree_weight` — `graph_ppr.rs:168–187` (modified)
- `graph_expand.rs:123–136` — positive BFS set (modified)
- `build_typed_relation_graph` Pass 2b — R-10 guard in `graph.rs` (reads from_str; all 10 variants must be present)
- `query_contradicts_edges_for_entry` — `read.rs:1529–1532` (fixed for bidirectionality)

---

## NOT in Scope

The following are explicitly excluded from vnc-015 to prevent scope creep:

- `context_graph` traversal tool (Phase 2 — depends on this feature populating the graph)
- Schema migration of any kind
- `context_batch_write` tool (HNSW atomicity problem unsolved — OSC-6 in ASS-057)
- Edge auto-transfer when a dependency target is superseded (risky if content changes)
- `metadata` column on entries (Thesis lifecycle gap — separate migration decision)
- NLI contradiction detection scoped to Claims only (Phase 3 intelligence work)
- PPR or graph_expand positive-type expansion beyond `RelatedTo` (`Advances`, `Motivates`, and all other 8 new variants are write-only; deferred to Phase 2)
- Config extension for `ppr_positive_edge_types` or `symmetric_edge_types` (design artifact only)
- Source ownership validation on any edge write surface — `agent_id` is not a reliable ownership anchor in this RBAC model; security gate is `Capability::Write` plus source entry status
- `context_graph` traversal modes: neighbors, subgraph, path, chain, inverse, filter
- `resolve_supersessions` parameter (Phase 2 traversal tool concern)
- `cycle_anchor_category` config extension
- As-of timestamp support
- Thesis status lifecycle (metadata column not in scope)
- `RelatedTo` bidirectional storage (left to architect — not mandated by SCOPE.md)
- `context_edge` bulk/batch variant (single edge operation per call)
- Auto-retarget on `context_correct` (edge absence is useful signal; auto-retarget pollutes the correction chain)
- `StoreConfig.edge_confidence_floor` field (confidence floor dropped entirely)

---

## Open Questions for Architect

**OQ-01: Partial-write blast radius (Medium priority)**
SR-03 flags that entry insert + edge writes are not in a single DB transaction. If the entry is
written but an edge write fails, the entry exists with no edges. The SCOPE.md accepts this as an
infrastructure error. The architect must document: under what conditions does this actually occur,
how often is it recoverable (re-assertion via `context_correct`), and whether wrapping in an
explicit transaction is feasible given the `write_pool_server()` serialization constraint.

**OQ-02: CLOSED — Edge attribution (ADR-008)**
`EDGE_SOURCE_AGENT = "agent"` for both the `source` and `created_by` columns. ADR-008 is
authoritative. No open question.

**OQ-03: CLOSED — EntryStatus accessibility**
`Status` enum is in `unimatrix_store::schema` (pub) and is importable from `edge_write.rs`.
Use `Status::Quarantined` and `Status::Deprecated` directly.

**OQ-04: RelatedTo bidirectionality (Low priority)**
ASS-057 notes `RelatedTo` as semantically symmetric (and lists `symmetric_edge_types =
["Contradicts", "RelatedTo"]` in the research-domain.toml sketch). SCOPE.md only mandates
bidirectionality for `Contradicts`. Architect should decide whether `RelatedTo` gets
bidirectional storage in this feature or defers to Phase 2.

**OQ-05: CLOSED — redirect transaction pattern**
`write_pool.begin()` is confirmed at 4 callsites in `write.rs`. The RAII pattern is established.
The architect may use the same `write_pool.begin()` approach for redirect's remove+insert sequence.

**OQ-06: context_edge atomic remove+insert for redirect (Medium priority)**
The redirect mode performs remove old + insert new. For `Contradicts`, this is 4 row operations.
The SCOPE.md calls these "atomic" but `write_graph_edge` uses `write_pool_server()` (single
connection, serialized). The architect must confirm whether "atomic" here means within-transaction
or within-handler (sequential, no interleaving). If within-transaction, a new helper wrapping
both operations under an explicit transaction is required.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 11 relevant entries. Key hits: ADR-005
  (edge_write.rs module extraction), ADR-001 (validate before insert), ADR-002 (confidence floor
  failure posture — now superseded by scope change dropping confidence floor), ADR-003 (partial
  write blast radius), ADR-004 (DependencyOnDeprecated constructor injection), ADR-008
  (EDGE_SOURCE_AGENT convention). All directly inform the revision. Confidence floor ADR (entry
  4419 / ADR-002) is now superseded — the spec removes all confidence floor requirements per
  approved scope delta.
