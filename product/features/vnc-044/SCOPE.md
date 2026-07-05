# vnc-044 — Split the `context_graph` `format` Overload: Serialization vs. Verbosity, add a Lean Node Projection

> Status: SETTLED — human refined direction (2026-07-05). All four OQs resolved (see Settled Decisions). The two-axis model is a **suite-wide standard captured as an ADR** (Phase-2a, uni-architect owns); **vnc-044 is the first adopter** and implements it for `context_graph` only. Other context tools migrate to the same ADR in later features. Tracks GH #913; reconciles crt-057/vnc-011 (GH #894).

## Settled Decisions

- **D-1 (the two-axis model is a cross-tool STANDARD, captured as an ADR — human, 2026-07-05):** The corrected model is a **suite-wide contract**, not a graph-local hack. The ADR (a required Phase-2a deliverable, **owned by uni-architect**) fixes, for ALL context tools: (a) `format` means **serialization only** — exactly `markdown | json`; (b) a **separate verbosity axis** (recommend `detail`, values `summary | full` — architect ratifies the exact suite-wide spelling in the ADR) selects lean-vs-full; (c) **default verbosity = `summary`** as the suite norm (D-2); (d) legacy `format=summary` is a deprecated alias for `detail=summary`. **What vnc-044 scopes is the *implementation for `context_graph` only`.** Every other tool (`context_get`, `context_search`, `context_lookup`, `context_status`, the mutation tools, `context_briefing`) adopts the same ADR in **follow-up features referencing it** (see Tracking). So suite-wide *consistency of the model* is a GOAL (delivered by the ADR); suite-wide *implementation* is deferred, not the model.
- **D-2 (default verbosity = `summary`, suite-wide norm — was OQ-2; human):** Default (no verbosity given) returns the **lean** projection. Rationale: return agents the **minimum context** by default; they opt into `detail=full` explicitly, or pull full detail for any single entry via `context_get`. This is an accepted **behavior change** for `context_graph` (which returns full today). Legacy `format=summary` maps cleanly to `detail=summary`. Default-summary is the norm "mostly consistently across tools" — documented per-tool exceptions are allowed by the ADR.
- **D-3 (lean projection carries LIFECYCLE status only, option (c) — was OQ-3; human):** The summary node projects `EntryRecord.status` (lifecycle: active/deprecated/proposed/quarantined). Capability **delivery** status (missing/partial/proven/claimed, stored in `content`) is **NOT** extracted here — consistent with minimum-context: an agent needing it pulls the capability via `context_get`. Promoting capability delivery status to a first-class projected field is a **dependent follow-up feature** (see Tracking).
- **D-4 (axis mechanics — was OQ-4; human + architect to ratify):** Recommend field `detail` with values `summary | full`; **uni-architect ratifies the exact name/values in the ADR** (must be the suite-wide spelling). Modes with no node bodies (`neighbors`, `path`): **accept-and-ignore** the verbosity axis. `format=markdown` on a graph call: **reject loudly** (`ERROR_INVALID_PARAMS`) until a graph-markdown renderer exists — **no silent JSON fallback**.
- **D-6 (summary projection includes a bounded content preview — human-approved; part of the ADR's suite-wide definition of "summary", NOT graph-local):** The summary node field set is `{id, title, category, tags, status, confidence, content_preview, content_truncated}`; edges stay `{source_id, target_id, relation_type, depth}`.
  - **`content_preview`**: the first **256 bytes** of `content`, truncated on a **UTF-8 char boundary** — floor to the nearest char boundary at/below 256 bytes, never splitting a multibyte char. `256` is a **single tunable constant defined by the ADR**, applied consistently across every tool's summary view.
  - **`content_truncated`**: boolean — `true` when `content` exceeds the preview cap (real content elided), `false` when the whole content fit. A machine-readable signal that the agent should `context_get` the full node. **No bare `…` ellipsis** — use the flag.
  - Rationale (record): balances payload size against an extra round-trip — gives the gist of the description without the full blob; the agent pulls full detail via `context_get` when the preview is insufficient. It **partially (not reliably) softens** the lifecycle-vs-delivery status gap when a node's content leads with its status, but does **NOT** change the standing risk or follow-up #3 — delivery status is still not a guaranteed field.
- **D-5 (reconcile crt-057 / vnc-011, GH #894):** `context_cycle_review` already made `format` **render-only** (`markdown|json`, rejects `summary`) — it got the *serialization axis* right but **lacks the verbosity axis** (it dropped `summary` entirely rather than relocating it). The new ADR **generalizes crt-057's render-axis** across the suite and **adds the missing verbosity axis**, so summarized content stays reachable everywhere as `detail=summary` instead of being a rejected format. crt-057 is prior art the ADR builds on, not a conflict.

## Problem Statement

The `context_graph` tool's `format` parameter (`summary | markdown | json`) is a **category error**: it collapses two orthogonal axes into one enum.

- **Serialization** — `markdown` vs `json` — *how* output is rendered.
- **Verbosity / detail** — `summary` (lean) vs full — *how much* per-node content is returned.

`summary` is not a peer of `markdown`/`json`. A caller should be able to ask for *lean JSON* or *full markdown*; today the enum forbids that combination. Worse, in the entire graph path the parameter is **parsed and then ignored** — every graph mode emits the same full-content JSON regardless of `format`.

**Motivating incident (GH #913, 2026-07-05).** A vision-root orientation pull —
`context_graph(mode=subgraph, seed_ids=[4671], direction=incoming, edge_types=["Advances"], max_depth=2)` — returns **75 nodes + 82 edges = 134,835 characters**, **byte-identical** for `format=summary` and `format=markdown`. That overflows an agent's context window, so the result must be parsed out-of-band instead of consumed directly. A lean projection (id/title/category/tags/status/confidence per node) would drop this to a few KB and enable a **one-call orientation**, replacing the current multi-step goal→capability→status choreography.

Who is affected: every agent doing multi-hop graph traversal for orientation — the exact self-learning workflow the graph tool exists to serve (goal:self-learning, #913 label).

## Central Finding: the code state contradicts #913's premises

The #913 body asserts `chain`/`current` "already return lean records" and that only subgraph is broken. **Verified false against the code.** The real state:

- `format` **is** parsed — `ResponseFormat::{Summary, Markdown, Json}`, `parse_format` (`crates/unimatrix-server/src/mcp/response/mod.rs:59-81`, default `Summary`) — but the entire graph path **discards it**. `handle_graph` binds it as `_ctx: &ToolContext` (deliberately unused, `mcp/graph_read.rs:251`).
- **Every** graph mode serializes the full record via plain `serde_json::to_string`, format-blind: chain (`ChainResult { entries: Vec<EntryRecord> }`), current (`CurrentResponse { entry: EntryRecord }`), subgraph (`SubgraphResponse.nodes: Vec<EntryRecord>`, `graph_read.rs:325`), inverse/filter (`Vec<EntryRecord>`). `neighbors`/`path` return only edges (`EdgeRecord`/`PathHop`), no node bodies.
- There is **no lean/summary node projection anywhere in the graph path.** It does not exist and must be created. `chain`/`current` are exactly as heavy as subgraph.
- `fetch_nodes_batch` (`graph_read_subgraph.rs:639-677`) selects the full `ENTRY_COLUMNS` and hydrates tags. `EntryRecord` (`crates/unimatrix-store/src/schema.rs:48-102`) carries no `skip_serializing_if`, so `content`, `content_hash`, `previous_hash`, `embedding_dim`, timestamps, `created_by`/`modified_by`, and all counts always serialize.
- Graph output is **JSON-only in practice** — every mode ends in `serde_json::to_string`. There is no markdown rendering of a graph at all; `format=markdown` on a graph call today is a silent no-op that returns the same JSON.
- `ResponseFormat` + `parse_format` are **shared across the whole context-tool suite** (context_get, context_search, context_lookup, mutations, status, …), not just `context_graph`.

**Prior art the ADR builds on (crt-057 / vnc-011, GH #894).** The overload is already partly recognized: `context_cycle_review` documents `format` as a **"Render axis … exactly 'markdown' (default) or 'json'. 'summary' is NOT a valid value — it yields ERROR_INVALID_PARAMS"** (`mcp/tools.rs:446-451`). So the suite is *already inconsistent*: most tools accept `summary|markdown|json`; `context_cycle_review` rejects `summary` and treats `format` as render-only. crt-057 got the **serialization axis right** but **lacks a verbosity axis** — it dropped `summary` rather than relocating it. The new suite-wide ADR (D-1, D-5) generalizes crt-057's render-axis to every tool AND restores summarized content as the separate `detail=summary` axis.

## Goals

1. **Establish the suite-wide two-axis standard as an ADR** (Phase-2a, uni-architect): `format` = serialization (`markdown | json`); a separate verbosity axis (`detail`, `summary | full`) selects lean-vs-full; default verbosity = `summary`; legacy `format=summary` = deprecated alias for `detail=summary`. The ADR is the contract every context tool will converge on and reconciles crt-057.
2. **Implement that standard for `context_graph` only** — the first adopter. `format` becomes serialization-only; add the verbosity axis; wire both end-to-end (today `format` is parsed and dropped at `graph_read.rs:251`).
3. **Lean node projection** for graph modes that return node bodies (subgraph, chain, current, inverse, filter — all share the `EntryRecord` payload) so a large traversal drops from ~135KB to a few KB.
4. Preserve a path to the **full record** via `detail=full` for deep inspection.
5. **Backward-compatibility:** legacy `format=summary` keeps working, mapping to `detail=summary`, so existing callers do not break.
6. Advance the #913 orientation use case: a single subgraph pull over the vision root returns the goal→capability **structure** (ids, titles, categories, tags, lifecycle status, confidence, edges) in an agent-consumable size. **Note:** the projection carries *lifecycle* `status` only; it does **not** deliver a capability *delivery*-status (missing/proven) tally — that remains a `context_get` per-entry pull or a dependent follow-up (D-3).

## Non-Goals

1. **Implementing the two-axis model in the other context tools** — deferred to follow-up features (see Tracking). Note the distinction: suite-wide *consistency of the model* is a **GOAL**, delivered by the ADR (Goal 1). What is out of scope for vnc-044 is only the *code migration* of `context_get`/`context_search`/`context_lookup`/`context_status`/mutations/`context_briefing` — they keep their current `format` handling until their own ADR-referencing features land. vnc-044 must not silently change the shared `ResponseFormat` enum's behavior for those callers.
2. **A markdown rendering of graph structure** (node/edge tables, DOT, etc.) — graph output is JSON-only today; inventing a graph-markdown renderer is a separate feature. Per D-4, `format=markdown` on a graph call is **rejected loudly** (no silent JSON fallback) until such a renderer ships. The verbosity axis is the load-bearing change.
3. **A schema change to `EntryRecord`** (e.g. promoting capability delivery status to a first-class column) — out of scope (D-3). The projection carries lifecycle `EntryRecord.status` only; surfacing capability *delivery* status is a named dependent follow-up (Tracking).
4. **Changing traversal semantics** — BFS, `max_depth`, `max_nodes`, `resolve_supersessions`, edge filtering, truncation all stay exactly as shipped (vnc-018/019/020). This is a projection/serialization change only.
5. **New graph modes or new edge fields.** `EdgeRecord`'s wire shape is locked (ADR-004 vnc-018).
6. **Removing or renaming `format`.** `GraphParams` field layout is locked (ADR-003 vnc-018/019) — the new axis is *additive*; `format` is retained and re-scoped, never removed.

## Background Research

- **Parse-and-drop seam** (`graph_read.rs:247-368`): `handle_graph` receives `_ctx: &ToolContext` and never reads `.format`. Each mode arm calls `serde_json::to_string(&result)`. Wiring the axes means threading a resolved verbosity/format into each arm and choosing a serializer. This is the single change point; the seven mode arms are structurally identical.
- **`GraphParams` is layout-locked** (ADR-003, entries #4490/#4491): fields are never removed or reordered; forward-compat fields error on misuse via `validate_no_unsupported_params`. A **new verbosity field is additive** and must slot in without disturbing existing fields, with its own cross-mode validation (per D-4: accept-and-ignore on modes that return no node bodies, e.g. neighbors/path).
- **`EntryRecord` has a first-class `status` field** (`schema.rs:57`, type `Status` = `Active | Deprecated | Proposed | Quarantined`). This is the **lifecycle** status and is trivially projectable — the projection carries THIS (D-3). It is distinct from the capability *delivery* status the #913 orientation tally would want — see next bullet. This is the single most important nuance in the feature.
- **Two different "status" concepts** (drove OQ-3, settled as D-3; stored as pattern #5505):
  - `EntryRecord.status` — lifecycle: `Active/Deprecated/Proposed/Quarantined` (first-class column).
  - **Capability delivery status** — `missing | partial | proven | claimed` (per `.claude/skills/uni-capability/SKILL.md`). This is a **domain field stored inside the capability entry's `content` blob**, deliberately *not* a first-class node field ("do not bury volatile status in the goal" — the skill keeps it in the capability's content). A lean projection of `EntryRecord.status` returns `active` for every capability and does **not** deliver a delivery-status tally. Surfacing delivery status would require content parsing or a schema field. **D-3 settles this out of vnc-044**: project lifecycle status now; delivery-status promotion is a dependent follow-up (Tracking). An agent needing delivery status pulls the capability via `context_get`.
- **Node fetch** (`fetch_nodes_batch`, `graph_read_subgraph.rs:639-677`): selects `ENTRY_COLUMNS` + hydrates tags. The lean projection is cleanest done **at serialization** (keep the existing fetch, emit the projected field set). Note D-6 means the full `content` column is **still required** from the fetch — the preview is computed from it — so a SQL-level `content`-drop optimization is **not** available; only the *output* omits full content. Tags remain a per-batch join.
- **crt-057 prior art** (`tools.rs:446`, GH #894): a "render axis" framing where `format ∈ {markdown, json}` and `summary` is rejected. It got serialization right but has no verbosity axis (it dropped `summary`). The suite-wide ADR (D-1/D-5) generalizes its render-axis AND adds the missing verbosity axis so `detail=summary` restores lean content everywhere.
- **Suite-wide blast radius**: `parse_format` + `ResponseFormat` are consumed by ~all context tools. The ADR sets the target contract for all of them; vnc-044 implements it for `context_graph` without changing the shared enum's behavior for the other callers (they migrate in their own ADR-referencing features).

## Proposed Approach

Per the Settled Decisions (D-1..D-5). The ADR (Phase-2a, uni-architect) ratifies the suite-wide axis names/values first; the implementation below is the `context_graph` adoption.

1. **ADR first (Phase-2a):** uni-architect authors the suite-wide two-axis ADR — `format` = serialization (`markdown|json`); `detail` = verbosity (`summary|full`, exact spelling ratified here); default `summary`; legacy `format=summary` → `detail=summary`; reconciles crt-057. This ADR is the contract vnc-044 and all later tool-migration features reference.
2. **Introduce the verbosity axis on `GraphParams`** — additive field (`detail` recommended), per the ADR spelling. Keep `format` as serialization-only (`markdown | json`).
3. **Composition matrix** (the corrected model, per D-2/D-4):

   | `detail` \ `format` | `json` | `markdown` |
   |---|---|---|
   | `summary` (lean, **default**) | lean node projection as JSON | **rejected** (no graph-markdown renderer yet — D-4) |
   | `full` | full `EntryRecord` JSON (today's output) | **rejected** (D-4) |

4. **Lean projection** (per #913 + D-6): node → `{id, title, category, tags, status, confidence, content_preview, content_truncated}`; edge → `{source_id, target_id, relation_type, depth}`. Drop full `content`, `content_hash`/`previous_hash`, `embedding_dim`, timestamps, `created_by`/`modified_by`, counts. `content_preview` = first 256 bytes of `content` floored to a UTF-8 char boundary; `content_truncated` = bool (true iff content exceeded the cap). `256` is the ADR-defined shared constant. Implement as a dedicated projection type serialized in place of `Vec<EntryRecord>` — do **not** add `skip_serializing_if` to `EntryRecord` (shared store type; would leak into every other serializer). `status` = lifecycle `EntryRecord.status` (D-3).
5. **Default = summary** (D-2): when `detail` is absent, return the lean projection. Legacy `format=summary` maps to `detail=summary` (default serialization `json`).
6. **Wire the seam**: thread the resolved `(detail, format)` from `handle_graph` into each mode arm (fixing the `graph_read.rs:251` parse-and-drop) and pick the serializer/projection. Keep traversal untouched.
7. **Modes touched** (all within `context_graph`): subgraph, chain, current, inverse, filter (all carry `EntryRecord` payloads — projected consistently). neighbors/path return no node bodies — verbosity is **accept-and-ignore** (D-4).

## Acceptance Criteria

- **AC-01:** A suite-wide ADR (Phase-2a) defines the two-axis contract — `format`=serialization (`markdown|json`), verbosity axis (`detail`, `summary|full`), default `summary`, legacy `format=summary`→`detail=summary` — plus the summary field set and the shared 256-byte `content_preview` constant (D-6), and records how it generalizes/reconciles crt-057 (GH #894).
- **AC-02:** `context_graph` exposes both axes and honors them end-to-end — the resolved values reach serialization (fixing the `graph_read.rs:251` parse-and-drop). `format` no longer selects verbosity.
- **AC-03:** With `detail=summary`, a subgraph node serializes to exactly `{id, title, category, tags, status, confidence, content_preview, content_truncated}` and edges to `{source_id, target_id, relation_type, depth}`; full `content`, hashes, `embedding_dim`, timestamps, `created_by`/`modified_by`, and counts are omitted. `status` is the lifecycle `EntryRecord.status`.
- **AC-03b:** `content_preview` is the first ≤256 bytes of `content`, **floored to a UTF-8 char boundary** — it is always valid UTF-8 and never splits a multibyte char (test with multibyte content straddling byte 256). `content_truncated` is `true` iff `content` exceeded the 256-byte cap and `false` when the whole content fit (test both sides of the boundary, including exactly-256-byte and empty content). No ellipsis is appended.
- **AC-04:** With `detail=full`, `context_graph` returns the current full-`EntryRecord` payload byte-for-byte (no regression for existing full consumers).
- **AC-05:** **Default verbosity is `summary`** — a `context_graph` call with no verbosity axis returns the lean projection (accepted behavior change from today's full output).
- **AC-06:** The #913 reproduction — vision-root subgraph, 75 nodes / 82 edges — drops from ~135KB to a few KB by default (summary) and is valid parseable JSON.
- **AC-07:** Legacy `format=summary` remains accepted and maps to `detail=summary` (default serialization `json`), so no currently-working caller breaks.
- **AC-08:** `format=markdown` on any graph mode is **rejected** with `ERROR_INVALID_PARAMS` naming the reason (no graph-markdown renderer yet) — no silent JSON fallback. `detail` on `neighbors`/`path` is accepted and ignored (no node bodies).
- **AC-09:** `GraphParams` field layout invariant holds — the new axis is additive, existing fields unmoved (ADR-003 vnc-018/019 preserved); the tool description documents both axes and states that `status` is lifecycle status.

## Constraints

- **`GraphParams` layout is locked** (ADR-003, #4490/#4491) — the verbosity field is additive only; no existing field may be removed or reordered.
- **`EdgeRecord` wire shape is locked** (ADR-004 vnc-018) — no `skip_serializing_if`, `metadata` always serializes; the edge projection must be a *separate* type, not a mutation of `EdgeRecord`.
- **Do not add `skip_serializing_if` to `EntryRecord`** — it is the shared `unimatrix-store` type used by every serializer; a projection must be a distinct type or a `serde_json::Value` builder local to the graph path.
- **`ResponseFormat`/`parse_format` are suite-shared** (`response/mod.rs`) — touching the enum ripples across all context tools. Per D-1 the ADR sets their target contract, but vnc-044's **code change is graph-scoped**: do not alter the shared enum's behavior for its other callers (they migrate in later features).
- **Graph output is JSON-only today** — no markdown graph renderer exists; per D-4 `format=markdown` on graph is rejected until one is built (Non-Goal 2).
- **Capability delivery status is not a first-class field** — it lives in `content`; per D-3 the projection carries lifecycle status only, delivery-status surfacing is a dependent follow-up.
- **Max 500 lines/file** (rust-workspace rule) — `graph_read_subgraph.rs` is already large; a projection type likely needs its own module (Pattern #4518: extract when graph_read files approach the limit).
- **Per-mode change is coordinated** (Pattern #4500): each graph mode is a sibling handler; a cross-mode projection must touch each arm consistently or be centralized in `handle_graph`.

## Open Questions

All four OQs are **SETTLED** (human, 2026-07-05) and recorded in Settled Decisions:

- **OQ-1 (scope boundary) → D-1:** the two-axis model is a suite-wide ADR standard; vnc-044 implements it for `context_graph` only; other tools migrate later.
- **OQ-2 (default verbosity) → D-2:** default `summary`, suite-wide norm; legacy `format=summary`→`detail=summary` (serialization `json`).
- **OQ-3 (which status) → D-3:** lifecycle `EntryRecord.status` only; capability delivery status deferred to a dependent follow-up.
- **OQ-4 (axis mechanics) → D-4:** recommend `detail: summary|full` (architect ratifies in ADR); neighbors/path accept-and-ignore; `format=markdown` on graph rejected loudly.

**One item delegated to Phase-2a (not blocking scope):** uni-architect ratifies the exact suite-wide axis **name and value spelling** in the ADR (`detail` vs `verbosity` vs `view`; `summary|full` vs `lean|full`). SCOPE recommends `detail: summary|full`.

## Tracking

- GH Issue: #913 (to be linked/relabeled to vnc-044 after Session 1).
- **Phase-2a deliverable:** suite-wide two-axis ADR (uni-architect) — the standard all context tools converge on; reconciles crt-057.
- **Dependent follow-up features** (file after ADR lands):
  1. Migrate `context_get`, `context_search`, `context_lookup`, `context_status`, the mutation tools, and `context_briefing` to the ADR's two-axis model (retire `format=summary` overload suite-wide).
  2. Fold `context_cycle_review` (crt-057) fully onto the ADR — add the `detail` axis so summarized review output is reachable rather than `summary` being rejected.
  3. **Promote capability delivery status (missing/partial/proven/claimed) to a first-class projected field** so a subgraph orientation pull can deliver a status tally in one call (the #913 "prefer surfacing as a field" ask; needs a schema/first-class-field addition — D-3).
  4. Graph-markdown renderer, if a markdown serialization of graph structure is ever wanted (unblocks `format=markdown` on graph — D-4).
- Related prior art: vnc-018/019/020 (context_graph modes, `GraphParams`/`EdgeRecord` locks), crt-057/vnc-011 = GH #894 (render-axis precedent on `context_cycle_review`).
