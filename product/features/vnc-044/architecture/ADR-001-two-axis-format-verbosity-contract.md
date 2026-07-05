## ADR-001: Suite-Wide Two-Axis Output Contract — `format` (serialization) and `detail` (verbosity)

**Feature:** vnc-044 | **Status:** active | **Scope:** all context tools (contract); `context_graph` (first implementation, this feature)

### Context

The context-tool suite overloads a single `format` parameter with two orthogonal
concerns:

- **Serialization** — `markdown` vs `json` — *how* output is rendered.
- **Verbosity** — `summary` (lean) vs full — *how much* per-entry content is returned.

`ResponseFormat::{Summary, Markdown, Json}` (`response/mod.rs:59-81`, default `Summary`)
collapses both onto one enum, so `summary` is a false peer of `markdown`/`json`: a caller
cannot ask for *lean JSON* or *full markdown* — the combination the enum forbids is exactly
the one agents want. `context_graph` shows the failure most sharply: `format` is parsed and
then **discarded** (`handle_graph` binds `_ctx`, `graph_read.rs:251`); every mode serializes
the full `EntryRecord` via `serde_json::to_string`, byte-identical for `summary` and
`markdown`. The GH #913 vision-root pull returns 75 nodes / 82 edges = ~135KB regardless of
`format`, overflowing an agent's context window.

Prior art — **crt-057 / GH #894** (`context_cycle_review`, entry #5434) — already recognised
half the problem: it made `format` **render-only** (`markdown | json`, `summary` →
`ERROR_INVALID_PARAMS`). It got the *serialization* axis right but **dropped** the verbosity
concern instead of relocating it, leaving summarized content unreachable on that tool and the
suite internally inconsistent (most tools accept `summary|markdown|json`; `context_cycle_review`
rejects `summary`).

The suite needs one contract that (a) fixes `format` to serialization, (b) introduces a
separate verbosity axis, (c) preserves lean output as a first-class option everywhere, and
(d) generalizes crt-057's render-axis rather than contradicting it. This ADR is that contract.
It binds every context tool; **vnc-044 implements it for `context_graph` only** (see
ADR-002). Other tools migrate in follow-up features that reference this ADR.

### Decision

**Two orthogonal axes on every context tool's parameter surface.**

**1. `format` = serialization only.** Values: exactly `markdown | json`. `format` never
selects verbosity. This is crt-057's render-axis, generalized to the suite. (Per-tool: a tool
with no renderer for a given serialization MAY reject it loudly — see ADR-002, where
`context_graph` rejects `markdown` until a graph-markdown renderer exists. Rejection is a
per-tool capability statement, not a change to the axis's meaning.)

**2. `detail` = verbosity.** Ratified name/values (resolving SCOPE OQ-4):

> **`detail`**, with values **`summary | full`.**

Chosen over `verbosity`/`view` (ambiguous with UI) and over `lean` (does not map cleanly to
the legacy `format=summary` alias). `detail=summary` is the lean projection; `detail=full` is
the complete record.

**3. Default verbosity = `summary`** (suite norm). A call with no `detail` returns the lean
projection — the minimum-context default. Agents opt into `detail=full` explicitly, or pull a
single entry in full via `context_get`. Per-tool default divergence is permitted **and must be
documented in that tool's description** during the migration window (tools not yet migrated
keep their current default; `context_graph` flips `full`→`summary` as an accepted behavior
change — ADR-002).

**4. Legacy `format=summary` = deprecated alias for `detail=summary`.** When a caller passes
`format=summary`, resolve it to `detail=summary` with serialization `json`. This keeps every
currently-working caller functional. A tool MAY reject `format=summary` when an explicit
`detail` is *also* supplied (the two would conflict) — that is the ADR-recommended resolution
and what ADR-002 adopts.

**5. The `summary` field set (the suite definition of "lean").** A summary entry projects:

> `{ id, title, category, tags, status, confidence, content_preview, content_truncated }`

and (where a tool returns graph edges) an edge projects `{ source_id, target_id, relation_type, depth }`.
All other `EntryRecord` fields — full `content`, `content_hash`/`previous_hash`,
`embedding_dim`, timestamps, `created_by`/`modified_by`, version, counts — are **omitted from
the wire** (not from the DB read; see point 8).

- **`status` is the lifecycle enum** — `active | deprecated | proposed | quarantined`
  (`EntryRecord.status`). **It is NOT capability delivery status** (`missing | partial |
  proven | claimed`), which lives inside the capability entry's `content` blob and is not a
  first-class field. This distinction is load-bearing and is restated in point 7.
- **Per-tool field-set override is explicitly allowed.** The set above is the canonical
  default. A tool whose domain needs a different lean set MAY document its own override in its
  description (SR-03). The contract fixes the *shape and the shared constants*, not a
  one-size-fits-all field list — this keeps the standard from over-fitting `context_graph`.

**6. `content_preview` and the shared 256-byte constant (single source).** `content_preview`
is the first **256 bytes** of `content`, floored to a UTF-8 char boundary — never splitting a
multibyte char. `256` is **one constant defined once** (`CONTENT_PREVIEW_BYTES`) and imported
by every adopter — never re-literalled per tool (SR-03, evidence #4975). `content_truncated`
is a boolean: `true` iff `content` exceeded 256 bytes (real content elided), `false` when the
whole content fit. **No `…` ellipsis is appended** — the flag is the machine-readable signal to
`context_get` the full entry. The flooring MUST use the codebase's established idiom
(`while end > 0 && !s.is_char_boundary(end) { end -= 1; }`), not the nightly
`str::floor_char_boundary`.

**7. Lifecycle status ≠ delivery status (make this loud — SR-09).** A summary projection of a
subgraph of capability entries returns `status: "active"` for every node, because lifecycle
status is `active` for a live capability regardless of whether it is `missing` or `proven`.
The lean projection therefore **does not deliver the #913 orientation *delivery*-status
tally.** `content_preview` only *partially* and *unreliably* softens this (when a node's
content happens to lead with its status). **This ADR and every adopting tool's description MUST
state plainly that summary `status` is lifecycle status, not delivery status.** Promoting
capability delivery status to a first-class projected field is a **named dependent follow-up**
(vnc-044 Tracking #3), not delivered here. Design and copy must not imply #913's status tally
is delivered.

**8. Serialization-time projection, not a query change.** The lean projection omits fields
from the *output* only. `content` is still read from the DB (the preview is computed from it),
so this is a wire-size / agent-context-size win, **not** a DB-read or hydration-cost win
(SR-01). No adopter may justify a SQL `content`-drop by citing this ADR.

**Single-source primitives (SR-03).** The shared, tool-agnostic pieces live in one module and
are imported by all adopters, never re-declared:

- `CONTENT_PREVIEW_BYTES: usize = 256`
- `enum Detail { Summary, Full }` + a parser
- `content_preview(&str) -> (String, bool)` (preview + truncated flag)

The *projection type* carrying the field set is per-tool (ADR-002 defines `context_graph`'s),
because field sets are per-tool overridable.

**Reconciliation with crt-057 (#5434).** crt-057's `format` = render-only (`markdown|json`,
`summary` rejected) is the serialization axis of this contract — **compatible, not
superseded.** This ADR generalizes it to the suite and *adds back* the verbosity concern as
`detail`, so summarized `context_cycle_review` output becomes reachable as `detail=summary`
instead of a rejected `format`. crt-057's ADR remains active; folding `detail` onto
`context_cycle_review` is follow-up #2. No deprecation of #5434 is performed.

### Consequences

**Easier:**
- A caller can request any (verbosity × serialization) combination the two axes allow; the
  category error is gone.
- Lean-by-default returns agents minimum context; large traversals become consumable in one
  call.
- The suite converges on one mental model; crt-057 stops being an inconsistent outlier.
- Locked values (256, axis spelling, field set) have a single source, so downstream adopters
  cannot drift them (SR-03).

**Harder:**
- The suite is **temporarily inconsistent by design**: only `context_graph` implements the
  contract in vnc-044; other tools keep their current `format` handling until their own
  migration features. Each tool's description must disclose its current default (SR-04).
- Default `full`→`summary` is a behavior change for adopting tools (accepted; disclosed in the
  tool description).
- `content_preview` UTF-8 flooring is an easy-to-get-wrong hotspot (SR-02) — a naive
  `&s[..256]` panics on a non-boundary. The shared helper and mandated boundary tests
  (exactly-256, straddle-256, empty) are non-negotiable.
- The projection's `status` answers a *different question* than #913's orientation ask
  (lifecycle vs delivery). Carried honestly as a stated limitation + named follow-up, not a
  silent gap (SR-09).

### Cross-references

- Generalizes / reconciles: crt-057 ADR-002 (#5434, GH #894) — render-only `format` on
  `context_cycle_review`.
- Implemented by: ADR-002 (vnc-044) — `context_graph` adoption.
- Locked-value drift evidence: #4975. SR-03 single-source guard.
- Constrained by: `ResponseFormat`/`parse_format` are suite-shared (`response/mod.rs`); this
  ADR sets their *target* contract but vnc-044 does not alter the shared enum's behavior for
  non-graph callers (SR-06).
