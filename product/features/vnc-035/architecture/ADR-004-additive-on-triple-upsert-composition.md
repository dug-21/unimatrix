## ADR-004: Additive-on-Triple Upsert Composition via the UNIQUE Constraint

### Context

SCOPE OQ-01 / AC-08 settle composition: carry-forward is the **baseline**; passed
`params.edges` **upsert** on the full triple `(source, target, relation_type)`. Required
behaviors:

- **Idempotent exact re-pass** — passing an edge identical to a carried edge does not
  double-write and does not error.
- **Additive new edge** — passing a genuinely new edge adds it.
- **Changed target, same relation** — produces a **second** edge (correct for legitimately
  multi-target relations like `Advances`), not a replacement.
- **Removal is only via the shed path** (`context_edge remove`), never via omission from
  `edges`.

This must hold without caller changes (OQ-04: existing callers that re-pass `edges` must
neither double-write nor conflict). The composition mechanism must be defined so it cannot be
implemented as ad-hoc diff/merge logic that could get any of these cases wrong.

### Decision

Composition is realized **entirely by the `graph_edges` UNIQUE constraint**
`UNIQUE(source_id, target_id, relation_type)` and `INSERT OR IGNORE` — **no diff logic, no
pre-read-and-merge**. Both writers target the same id B:

- Step 8b writes `params.edges` onto B (`validate_and_write_edges`).
- Step 8b′ writes A's eligible outgoing edges onto B (`run_carry_forward_loop`).

Each write is `INSERT OR IGNORE` keyed on the full triple. Therefore:

| AC-08 case | Mechanism | Result |
|------------|-----------|--------|
| Exact re-pass (`edges` triple == carried triple) | Second insert hits UNIQUE → `false`, ignored | One edge; idempotent; not double-counted |
| New edge (triple not present) | Insert succeeds → `true` | Edge added |
| Changed target, same relation (`B→X` carried, `B→Y` passed) | Different triple → both insert | **Two** edges (correct, multi-target) |
| Removal | No insert path removes; omission from `edges` is a no-op | Edge persists; remove only via shed (`context_edge remove`) |

"Additive on the full triple" is precisely the semantics of a unique index on that triple under
`INSERT OR IGNORE`. The order of 8b vs 8b′ does not change the final edge set (idempotent
insert is commutative); it only changes which writer gets the `true` for counting — and per
ADR-001/ADR-003 carry writes second so `edges_carried` counts only edges the caller did *not*
re-supply.

Back-compat (OQ-04) is automatic: a legacy caller that re-passes A's full edge list during
correction now hits UNIQUE conflicts against the carried baseline → idempotent, no double-write,
no error. No caller audit, no caller changes.

#### Carried edge metadata (OQ-03 / FR-11 — settled)

A carried edge is written via the **normal agent edge-write path** (`write_graph_edge` with
`source = EDGE_SOURCE_AGENT`), making it **byte-for-byte indistinguishable from a
freshly-declared edge**. The original edge's row metadata is **not** preserved:

| Field | Carried-edge value | Preserved from source row? |
|-------|--------------------|----------------------------|
| `relation_type` | source row's `relation_type` (the carry is meaningless otherwise) | **Yes** — load-bearing |
| `target_id` | source row's `target_id` | **Yes** — load-bearing |
| `created_at` | **`now`** — the correction's timestamp | **No** |
| `created_by` / `source` | `"agent"` (`EDGE_SOURCE_AGENT`) | **No** — always re-stamped as agent |
| `weight` | `1.0` | **No** |
| `bootstrap_only` | `0` | n/a |
| `metadata` | `""` | **No** |

**No provenance marker.** There is nothing on the carried row that distinguishes it from an
edge the agent declared directly during this correction. This is the **"simplest" OQ-03
resolution**: the carry path does not branch from the standard edge-write path and stores no
extra carry-origin field, so there is no metadata to keep consistent, migrate, or reason
about. Awareness that edges were carried comes **only from the `edges_carried` response ack**
(AC-11), never from row metadata. Any consumer needing carry-origin provenance must capture
it from the ack at correction time.

This is why `query_outgoing_edges` reads only `target_id` and `relation_type` for the write
(`OutgoingEdgeRow.created_at` is used, if at all, for ordering/observability — it is **not**
written onto B).

### Consequences

Easier: AC-08's four cases all fall out of one DB constraint — no custom merge code to get
wrong, no read-modify-write race. Idempotency and back-compat are structural. The "changed
target = two edges" case (the subtle one) is automatic and correct for multi-target relations.

Harder: composition relies on the UNIQUE index existing exactly as `(source_id, target_id,
relation_type)` — if a future migration alters it, composition semantics shift silently. The
index is pre-existing and load-bearing for the whole edge subsystem, so this is low risk but
worth a test that pins the changed-target two-edge case (AC-08 mandates it).

Related: ADR-001 (8b/8b′ order → counting), ADR-003 (count keys off `true`), pattern #4041
(rows-affected). OQ-01/OQ-03/OQ-04.
