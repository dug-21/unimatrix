## ADR-001: Validate Edge Types and Self-Reference Before Entry Insert

### Context

The `context_store` and `context_correct` handlers accept an `edges: Option<Vec<EdgeInput>>`
parameter. Two of the three validation checks — unknown edge type and self-referential edge —
can be performed without any DB access: `RelationType::from_str()` is a pure enum lookup, and
the `source_id == target_id` check is a scalar comparison. A third check (confidence floor)
requires one DB read.

The question is whether to fail the call before the entry insert (validate-first) or to insert
the entry and then validate edges (insert-first, rollback on failure).

SR-01 and SR-03 from the scope risk assessment identify the partial-write blast radius as an
accepted risk: if edge writes fail after entry insert, the entry exists with no edges and no
notification to the caller. To minimize this risk, validations that can be performed before
insert should be.

The SCOPE.md Proposed Approach says: "Before the entry insert, validate all edges up-front."

### Decision

Validate edge type resolution and self-referential checks before the entry insert. These are
pure/static checks requiring no DB access. If any edge in the `edges` vec fails either check,
return an error immediately — no entry is written, no edges are written.

The confidence floor check (dynamic, requires the source entry's confidence value) is a
distinct concern addressed in ADR-002.

Validation order within the static phase:
1. For each `EdgeInput`, call `RelationType::from_str(&edge.edge_type)`. If `None`, return error.
2. For each `EdgeInput`, check `source_id != edge.target_id`. If equal, return error.

These checks run before `StoreService.insert()`.

The three-case contract for the `write_graph_edge` bool return (true = new insert,
false+Ok = UNIQUE conflict idempotent, false+Err = SQL error) must be stated in the
implementation spec before any loop body is written (pattern #4041).

### Consequences

Easier: unknown edge types and self-referential edges are caught before any state is written.
The handler is fail-fast for the most common input errors. No partial-write from type errors.

Harder: the validation function must have access to the `source_id` (the entry being inserted)
before the insert happens. For `context_store` this is the entry ID that will be assigned by the
DB (auto-increment). This creates a sequencing tension: the `source_id` is only known after
insert. Resolution: the self-referential check uses the target_id from the `EdgeInput` compared
against the would-be source — for `context_store`, the source is the new entry's ID which is
not yet known. Therefore the self-referential check must run post-insert using the actual
assigned entry ID, not pre-insert. The unknown-edge-type check CAN run pre-insert. This ADR's
decision to "validate first" applies specifically to type resolution; self-referential checks
run immediately after insert but before the duplicate guard.

Supersedes: none.
Related: ADR-002 (confidence floor posture), ADR-003 (partial-write posture).
