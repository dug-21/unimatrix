# vnc-044 — Specification

> Source: `product/features/vnc-044/SCOPE.md` (SETTLED, 2026-07-05) and `product/features/vnc-044/SCOPE-RISK-ASSESSMENT.md`.
> Scope: implement the suite-wide two-axis response model **for `context_graph` only**. The suite-wide ADR (Phase-2a, uni-architect) is a companion deliverable; this spec covers the `context_graph` adoption.
> Axis-name: **RATIFIED by ADR-001 §2**. The suite-wide spelling is field `detail`, values `summary | full`, default `summary`, shared constant `CONTENT_PREVIEW_BYTES = 256`, summary field set `{id,title,category,tags,status,confidence,content_preview,content_truncated}` — matching SCOPE's recommendation this spec was written against. Every use of `detail`, `summary`, `full` below is the ratified spelling; no placeholder remains.

## Objective

The `context_graph` `format` parameter overloads two orthogonal concerns — serialization (`markdown|json`) and verbosity (`summary` vs full) — into one enum, and then discards the parsed value: every graph mode emits identical full-content JSON regardless of `format`. This feature splits `format` (serialization only, `markdown|json`) from a new verbosity axis (`detail`, `summary|full`), wires both end-to-end to the serializer, and adds a lean summary node projection so a large traversal (e.g. the #913 vision-root subgraph: 75 nodes / 82 edges / ~135KB) drops to a few KB and becomes agent-consumable in a single call. Default verbosity becomes `summary` (an accepted behavior change from today's full output).

## Functional Requirements

Each requirement is testable; verification is stated per acceptance criterion below.

### FR-1 — Two axes exposed on `context_graph`
`context_graph` accepts two independent parameters:
- **`format`** — serialization axis. Valid values: `json`, `markdown`. Selects *how* output is rendered.
- **`detail`** — verbosity axis (additive to `GraphParams`; ADR-ratified spelling). Valid values: `summary`, `full`. Selects *how much* per-node content is returned.

The two axes are orthogonal: any `(detail, format)` combination is expressible at the parameter layer. Rejections (FR-8) are enforced at resolution, not by forbidding the combination at parse.

### FR-2 — Axes honored end-to-end (fix parse-and-drop)
The resolved `(detail, format)` pair reaches serialization in every node-bearing mode. This fixes the current parse-and-drop seam where `handle_graph` binds `_ctx: &ToolContext` and never reads it (`crates/unimatrix-server/src/mcp/graph_read.rs:251`) and each mode arm serializes format-blind via `serde_json::to_string`. `format` MUST no longer influence verbosity, and `detail` MUST no longer be ignored on node-bearing modes.

### FR-3 — Default verbosity = `summary`
When no verbosity axis is supplied, `context_graph` returns the **lean summary projection**. This is an accepted behavior change for `context_graph`, which returns full `EntryRecord` output today. (SCOPE D-2; SR-04.) Default serialization remains `json`.

### FR-4 — Summary node projection field set
With `detail=summary`, each node in a node-bearing mode serializes to **exactly** this field set, and no other fields:

```
{ id, title, category, tags, status, confidence, content_preview, content_truncated }
```

- `status` is the lifecycle `EntryRecord.status` (`active | deprecated | proposed | quarantined`) — see FR-6 and Domain Models. It is **not** capability delivery status.
- All other `EntryRecord` fields are omitted from summary output: full `content`, `content_hash`, `previous_hash`, `embedding_dim`, timestamps, `created_by`, `modified_by`, and all counts.

### FR-5 — Summary edge projection field set
With `detail=summary`, each edge serializes to **exactly**:

```
{ source_id, target_id, relation_type, depth }
```

`EdgeRecord`'s remaining fields (including `metadata`) are omitted from summary output.

### FR-6 — `content_preview` semantics
`content_preview` is derived from the node's full `content`:
- It is the longest prefix of `content` whose byte length is **≤ 256 bytes**, floored to a UTF-8 character boundary (never splitting a multibyte codepoint). Naive `&content[..256]` is prohibited; a `floor_char_boundary`-style flooring is required (SR-02).
- The result is **always valid UTF-8**.
- **No ellipsis** (`…`) or any marker is appended. Truncation is signaled only by `content_truncated`.
- `256` is the ADR-defined shared preview-cap constant, single-sourced (SR-03); this spec references it symbolically and does not restate it as a magic number in downstream artifacts.

### FR-7 — `content_truncated` semantics
`content_truncated` is a boolean:
- `true` **iff** the full `content` byte length exceeded the 256-byte cap (real content was elided).
- `false` when the entire `content` fit within the cap (including empty content).

It is a machine-readable signal that the agent should `context_get` the full node when the preview is insufficient. Boundary behavior MUST be verified for: empty content, content exactly 256 bytes, content of 257 bytes, and multibyte content straddling byte 256 (FR-6 + FR-7 interaction).

### FR-8 — Serialization rejections and accept-and-ignore
- **`format=markdown` on any graph mode is rejected** with `ERROR_INVALID_PARAMS`. No silent JSON fallback. The error message MUST name the reason (no graph-markdown renderer exists yet) and point the caller to `format=json` (SR-05).
- On modes that carry **no node bodies** (`neighbors`, `path`), the `detail` axis is **accepted and ignored** — a supplied `detail` value does not error and does not change output.

### FR-9 — Legacy `format=summary` alias
Legacy `format=summary` remains accepted and maps to `detail=summary` with default serialization `json`. No currently-working `context_graph` caller breaks. This alias is documented as deprecated (superseded by `detail=summary`). (SCOPE D-2, AC-07.)

### FR-10 — `detail=full` fidelity
With `detail=full`, `context_graph` returns the current full-`EntryRecord` (and full `EdgeRecord`) payload **byte-for-byte** identical to today's output for the same query. No field is added, removed, reordered, or reformatted. (AC-04.)

### FR-11 — Modes in scope for projection
The summary projection applies to every node-bearing graph mode, all of which currently carry `EntryRecord` payloads and MUST be projected consistently:

| Mode | Node payload today | Summary projection applies |
|------|--------------------|----------------------------|
| `subgraph` | `SubgraphResponse.nodes: Vec<EntryRecord>` | yes |
| `chain` | `ChainResult.entries: Vec<EntryRecord>` | yes |
| `current` | `CurrentResponse.entry: EntryRecord` | yes |
| `inverse` | `Vec<EntryRecord>` | yes |
| `filter` | `Vec<EntryRecord>` | yes |
| `neighbors` | edges only (`EdgeRecord`) | n/a — `detail` accept-and-ignore |
| `path` | edges only (`PathHop`) | n/a — `detail` accept-and-ignore |

### FR-12 — Tool description documents both axes
The `context_graph` tool description MUST document both axes, the default-summary behavior (calling out the per-tool default divergence during the suite migration window — SR-04), the `format=markdown` graph exception (SR-05), and MUST state that projected `status` is **lifecycle** status, not capability delivery status (SR-09).

## Non-Functional Requirements

### NFR-1 — No regression for `detail=full`
`detail=full` output is byte-for-byte identical to the pre-vnc-044 full payload for the same query (measurable: golden-payload/byte-equality comparison). (Restates FR-10 as a regression guarantee.)

### NFR-2 — Shared types unchanged for non-graph callers
`ResponseFormat`, `parse_format`, `EntryRecord`, and `EdgeRecord` behavior MUST be unchanged for all non-graph callers (`context_get`, `context_search`, `context_lookup`, `context_status`, mutation tools, `context_briefing`). The vnc-044 code change is graph-local:
- No `skip_serializing_if` may be added to `EntryRecord` or `EdgeRecord` (shared `unimatrix-store`, wire-locked by ADR-003/ADR-004 — leaks into every serializer suite-wide; SR-07).
- The projection MUST be a **distinct type** (or a graph-local `serde_json::Value` builder), not a mutation of a shared struct.
- The shared `ResponseFormat` enum's behavior for its other callers MUST NOT change (SR-06). If any change touches the shared enum, the full call-site set MUST be enumerated (`cargo test --workspace --no-run`) before building.

### NFR-3 — `GraphParams` layout-locked (additive only)
`GraphParams` field layout invariant (ADR-003 vnc-018/019, entries #4490/#4491) holds: the verbosity field is **additive**; no existing field is removed, retyped, or reordered. `format` is retained and re-scoped, never removed (SCOPE Non-Goal 6). Cross-mode validation for the new field follows FR-8 (accept-and-ignore on neighbors/path).

### NFR-4 — Payload size is the win, not DB cost
The lean projection reduces wire bytes and agent-context size, **not** DB read or hydration cost: `content_preview` requires the full `content` column to be read (no SQL content-drop; SCOPE D-6, SR-01). Any performance claim in downstream artifacts MUST be scoped to payload/context size, not query cost. Measurable target: the #913 reproduction drops from ~135KB to a few KB under default summary (AC-06).

### NFR-5 — File-size limit
Rust 500-line/file limit holds. `graph_read_subgraph.rs` is already near the limit; the projection type SHOULD live in its own module (Pattern #4518, SR-08).

### NFR-6 — Traversal semantics unchanged
BFS, `max_depth`, `max_nodes`, `resolve_supersessions`, edge filtering, and truncation behavior (vnc-018/019/020) are unchanged. This is a projection/serialization change only.

## Acceptance Criteria

Mirrors and refines SCOPE AC-01..AC-09. AC-01 (the ADR) is a Phase-2a companion deliverable owned by uni-architect and is listed for traceability; AC-02..AC-09 are the `context_graph` implementation criteria this spec governs.

| AC-ID | Criterion | Verification method |
|-------|-----------|---------------------|
| **AC-01** | A suite-wide ADR defines the two-axis contract (`format`=serialization `markdown|json`; verbosity axis `detail` `summary|full`; default `summary`; legacy `format=summary`→`detail=summary`), the summary field set, the shared 256-byte `content_preview` constant, and records how it reconciles crt-057 (GH #894). | Artifact review of the ADR (Phase-2a, uni-architect). Out of this spec's implementation scope; referenced for trace. |
| **AC-02** | `context_graph` exposes both axes and honors them end-to-end; resolved values reach serialization (parse-and-drop at `graph_read.rs:251` fixed); `format` no longer selects verbosity. | Integration test: same query with `format=json`+`detail=summary` vs `format=json`+`detail=full` yields different payloads; code inspection confirms `handle_graph` reads the resolved axes. |
| **AC-03** | With `detail=summary`, a subgraph node serializes to exactly `{id,title,category,tags,status,confidence,content_preview,content_truncated}` and edges to `{source_id,target_id,relation_type,depth}`; `content`, hashes, `embedding_dim`, timestamps, `created_by`/`modified_by`, counts omitted; `status` is lifecycle `EntryRecord.status`. | Serialization test asserting the exact JSON key set (present keys AND absent keys) for a summary node and edge. |
| **AC-03b** | `content_preview` = first ≤256 bytes of `content` floored to a UTF-8 char boundary (always valid UTF-8, never splits a multibyte char). `content_truncated` = `true` iff `content` exceeded 256 bytes, `false` otherwise. No ellipsis appended. | Table-driven unit test with cases: (a) empty content → preview empty, truncated `false`; (b) content < 256B → full preview, `false`; (c) content exactly 256B → full preview, `false`; (d) content 257B ASCII → 256B preview, `true`; (e) multibyte content whose codepoint straddles byte 256 → preview floored below 256B on a char boundary, valid UTF-8, `true`. Assert no `…` present. |
| **AC-04** | With `detail=full`, `context_graph` returns the current full-`EntryRecord` payload byte-for-byte (no regression). | Golden/byte-equality test comparing `detail=full` output to the pre-change full payload for a fixed query. |
| **AC-05** | Default verbosity is `summary`: a `context_graph` call with no verbosity axis returns the lean projection (accepted behavior change). | Test: call without `detail` → output equals `detail=summary` output and differs from `detail=full`. |
| **AC-06** | The #913 reproduction (vision-root subgraph, 75 nodes / 82 edges) drops from ~135KB to a few KB by default and is valid parseable JSON. **The result carries lifecycle `status` only — a capability subgraph shows `active` for every node and does NOT deliver a delivery-status tally** (SR-09); this limitation is documented so the result is not misread as answering orientation-status. | Reproduction test measuring payload size < an agreed KB threshold and asserting valid JSON; description/tool-doc review confirming the lifecycle-vs-delivery caveat is stated. |
| **AC-07** | Legacy `format=summary` remains accepted and maps to `detail=summary` (default serialization `json`); no working caller breaks. | Test: `format=summary` request produces byte-identical output to `detail=summary` and is accepted (no error). |
| **AC-08** | `format=markdown` on any graph mode is rejected with `ERROR_INVALID_PARAMS` naming the reason and pointing to `format=json`; no silent JSON fallback. `detail` on `neighbors`/`path` is accepted and ignored. | Test: `format=markdown` on each mode → `ERROR_INVALID_PARAMS` with reason string; `detail=summary` and `detail=full` on `neighbors`/`path` → identical, non-erroring output. |
| **AC-09** | `GraphParams` field layout invariant holds — new axis additive, existing fields unmoved (ADR-003 preserved); tool description documents both axes and states `status` is lifecycle status. | Code inspection / existing GraphParams layout test; tool-description review. |

## Domain Models / Ubiquitous Language

- **Serialization axis (`format`)** — *how* graph output is rendered on the wire. Exactly `json` or `markdown`. For `context_graph`, only `json` is currently serviceable; `markdown` is rejected (no graph-markdown renderer). Distinct from verbosity.
- **Verbosity axis (`detail`)** — *how much* per-node content is returned. `summary` (lean projection) or `full` (complete records). Default `summary`. Additive field on `GraphParams`. (Ratified by ADR-001 §2.)
- **Summary projection (lean projection)** — the reduced node/edge field sets returned under `detail=summary`: node `{id,title,category,tags,status,confidence,content_preview,content_truncated}`, edge `{source_id,target_id,relation_type,depth}`. A distinct graph-local type, not a mutation of `EntryRecord`/`EdgeRecord`.
- **Content preview (`content_preview`)** — the first ≤256 bytes of a node's `content`, floored to a UTF-8 char boundary, no ellipsis. Gives the gist without the full blob.
- **`content_truncated`** — boolean flag, `true` iff content exceeded the preview cap. The machine-readable signal to `context_get` the full node. Replaces any `…` marker.
- **Lifecycle status** — `EntryRecord.status`: `active | deprecated | proposed | quarantined`. A first-class column (`schema.rs:57`). This is what the summary projection carries.
- **Capability delivery status** — `missing | partial | proven | claimed` (per `.claude/skills/uni-capability/SKILL.md`). A domain field stored **inside the capability entry's `content` blob**, NOT a first-class column. The summary projection does **not** surface it; every capability node shows lifecycle `active`. The distinction (SR-09) is the single most important nuance: the projection answers "what is the graph structure and each node's lifecycle state," not "what is each capability's delivery state." Surfacing delivery status is a named dependent follow-up (SCOPE Tracking #3), out of scope here.
- **Node-bearing mode** — a graph mode whose result contains node bodies (`subgraph`, `chain`, `current`, `inverse`, `filter`). The projection applies to these. `neighbors`/`path` return only edges/hops and accept-and-ignore `detail`.

## User Workflows

- **Single-call orientation (the #913 driver).** An agent issues `context_graph(mode=subgraph, seed_ids=[...], direction=incoming, edge_types=[...], max_depth=N)` with no verbosity axis. It receives the lean summary projection (goal→capability structure: ids, titles, categories, tags, lifecycle status, confidence, edges) in a few KB — consumable directly in-context, replacing the multi-step goal→capability→status choreography. When a node's `content_truncated` is `true` and the preview is insufficient, the agent pulls that single node's full record via `context_get`.
- **Deep inspection.** An agent needing complete records passes `detail=full` and receives today's full `EntryRecord` payload unchanged.
- **Legacy caller.** An existing caller passing `format=summary` continues to work unchanged (aliased to `detail=summary`, `json`).
- **Serialization mistake.** A caller passing `format=markdown` on a graph mode receives `ERROR_INVALID_PARAMS` naming the missing graph-markdown renderer and pointing to `format=json`, rather than silently getting JSON.

## Constraints (technical)

- **C-1** — `GraphParams` layout locked (ADR-003, #4490/#4491): verbosity field additive only; no field removed/retyped/reordered.
- **C-2** — `EdgeRecord` wire shape locked (ADR-004 vnc-018): no `skip_serializing_if`, `metadata` always serializes on the full record; the edge summary projection MUST be a separate type.
- **C-3** — Do NOT add `skip_serializing_if` to `EntryRecord` (shared `unimatrix-store` type); projection MUST be a distinct type or a graph-local `serde_json::Value` builder.
- **C-4** — `ResponseFormat`/`parse_format` are suite-shared (`response/mod.rs`); vnc-044's code change is graph-scoped — do not alter shared-enum behavior for non-graph callers (SR-06). ~45-site blast radius if the shared enum is touched (pattern #4831).
- **C-5** — Graph output is JSON-only today; `format=markdown` on graph is rejected until a renderer ships (Non-Goal 2).
- **C-6** — Capability delivery status is not a first-class field; projection carries lifecycle status only (D-3).
- **C-7** — Max 500 lines/file; projection likely needs its own module (Pattern #4518).
- **C-8** — Per-mode change is coordinated (Pattern #4500): a cross-mode projection must touch each node-bearing arm consistently or be centralized in `handle_graph`.
- **C-9** — `256` preview cap is an ADR-single-sourced constant (SR-03); reference symbolically, do not restate as a literal in the issue body or scattered code.

## Dependencies

- **Crates / modules** — `unimatrix-server` (`mcp/graph_read.rs`, `mcp/graph_read_subgraph.rs`, `mcp/response/mod.rs`, `mcp/tools.rs`); `unimatrix-store` (`schema.rs`: `EntryRecord`, `EdgeRecord`, `Status`). `serde_json` for serialization.
- **Existing components** — `handle_graph` dispatch and the seven mode handlers; `GraphParams`, `ResponseFormat`/`parse_format`; `fetch_nodes_batch` (retains full `ENTRY_COLUMNS` fetch + tag hydration — preview computed from full `content`).
- **Companion deliverable** — the Phase-2a suite-wide two-axis ADR (**ADR-001**, uni-architect), §2 of which has ratified the axis name/values (`detail: summary|full`, default `summary`), the summary field set, and the shared `CONTENT_PREVIEW_BYTES = 256` constant this spec references.
- **Prior art** — vnc-018/019/020 (graph modes, `GraphParams`/`EdgeRecord` locks); crt-057/vnc-011 = GH #894 (render-axis precedent on `context_cycle_review`).

## NOT in Scope (explicit exclusions)

1. **Migrating the other context tools** (`context_get`, `context_search`, `context_lookup`, `context_status`, mutation tools, `context_briefing`) to the two-axis model — deferred to follow-up features. vnc-044 must not change the shared `ResponseFormat` enum's behavior for those callers. (Suite-wide *model consistency* is delivered by the ADR; suite-wide *implementation* is deferred.)
2. **A markdown rendering of graph structure** (node/edge tables, DOT, etc.). `format=markdown` on graph is rejected loudly until such a renderer ships.
3. **A schema change to `EntryRecord`** — including promoting capability delivery status to a first-class column. Projection carries lifecycle `status` only; delivery-status surfacing is a named dependent follow-up (SCOPE Tracking #3).
4. **Changing traversal semantics** — BFS, `max_depth`, `max_nodes`, `resolve_supersessions`, edge filtering, truncation all stay as shipped (vnc-018/019/020).
5. **New graph modes or new edge fields.** `EdgeRecord` wire shape is locked.
6. **Removing or renaming `format`.** `GraphParams` layout is locked; the new axis is additive; `format` is retained and re-scoped.
7. **Folding `context_cycle_review` (crt-057) onto the ADR's `detail` axis** — a named dependent follow-up, not this feature.

## Open Questions

- **OQ-A (verbosity-axis spelling) — RESOLVED by ADR-001 §2.** The suite-wide spelling is ratified as field `detail`, values `summary | full`, default `summary` — exactly the spelling this spec was written against (FR-1, FR-3, FR-4, FR-9 legacy alias target, FR-10, AC-02/AC-03/AC-04/AC-05/AC-07, and the `GraphParams` additive field name in NFR-3/C-1 all use the ratified spelling). No open question remains; no semantic change from ratification.
- **OQ-B (architect — projection placement).** Whether the summary projection is a dedicated type in its own module vs a graph-local `serde_json::Value` builder, and where it lives given the 500-line limit on `graph_read_subgraph.rs` (Pattern #4518). Spec requires only that it be graph-local and not mutate shared types (C-3/C-7); the concrete placement is an architecture decision.
- **OQ-C (value of the 256 constant and its single source) — RESOLVED by ADR-001 §2.** The shared cap is ratified as `CONTENT_PREVIEW_BYTES = 256`; the spec references it symbolically as this ADR-single-sourced constant. Remaining detail — the precise module location of the constant — is an implementation-placement decision for the architect/developer, not an open spec question.
