# vnc-042 — context_get resolves superseded entries to current by default

## Problem Statement
`context_get(id)` fetches an entry by exact integer ID with no status filter and no
supersession follow (`crates/unimatrix-server/src/mcp/tools.rs:978` →
`self.entry_store.get(id)`). When the ID points at a **deprecated** entry, the agent
silently receives stale content with no signal that a corrected version exists.

This is a latent correctness hazard, not a regression — it has always behaved this way.
It bites whenever an ID comes from a **durable source**: a memory file, a stored doc,
another entry's edge, a prior session — exactly where staleness accumulates. It bit live
this session (nan-022 design docs reference capability C0 as deprecated #5191;
`context_get(5191)` returns the old C0, not corrected #5304).

The resolution capability already exists elsewhere: `context_search` deprioritizes
deprecated entries, and `context_graph` mode `current` resolves any ID to its active
terminal and returns the full `EntryRecord`. `context_get` simply does not route to it.

Tracked by **GH Issue #843** — the product behavior and AC-1..AC-7 are **LOCKED**. This
SCOPE validates that behavior against the codebase and resolves the one open design
question (parameter naming). It does not relitigate the locked behavior.

## Goals
1. `context_get` resolves a requested deprecated ID to its active terminal **by default**,
   returning that terminal's full content in the same shape as a direct get.
2. Emit a one-line resolution notice **only when a hop occurred**; clean passthrough
   (no notice) when the requested ID is already the active terminal.
3. Preserve a deliberate escape hatch that returns the entry **exactly as stored** for any
   status (lookback / provenance / audit), with a small deprecated-pointer footer.
4. Fail loud, never silent/empty, when the chain terminates on a non-active entry
   (orphaned deprecated / quarantined).
5. Reuse existing supersession-resolution machinery — no new chain-walk implementation.
6. Land the ADR for the default-behavior change to the most-used read tool, and resolve
   the parameter name (see Open Questions — the primary human decision).

## Non-Goals
Pulled directly from the issue's Out of Scope plus scope-tightening:
- **NG-1** No resolution of **stale neighbor targets** inside the `include_edges` view.
  The edge/discovery list (`get_edges.rs`) still returns deprecated targets' old id+title
  unresolved. Separate, lower-priority follow-up. Only the **requested** entry resolves.
- **NG-2** No multi-entry / chain / evolution view on `context_get`. `context_graph` mode
  `chain` already covers lookback across the whole chain. Do not overload `context_get`.
- **NG-3** No change to `context_search` or `context_lookup` (search already deprioritizes
  deprecated).
- **NG-4** No change to `context_graph` or its `resolve_supersessions` parameter, its
  default, or its semantics. (Naming *consistency* is discussed, but the graph tool is not
  modified in this feature.)
- **NG-5** No schema / storage change. `supersedes` / `superseded_by` columns already exist
  and are written atomically by `context_correct`.

## Background Research

### Current handler (the surface to change)
- `context_get` handler: `tools.rs:950-1052`. Entry fetched at `tools.rs:978` via
  `self.entry_store.get(id)` — a raw by-ID read, no status check, no follow.
- `GetParams` struct: `tools.rs:246-274`. Existing params: `id`, `agent_id`, `format`,
  `feature`, `helpful`, `session_id`, `include_edges`. The new resolution knob is added
  here (`#[serde(default)]`, backward-compatible like `include_edges`).
- Response is built by `format_single_entry(&entry, ctx.format, edges_view.as_ref())`
  (`tools.rs:1000`). Precedent for attaching a note exists:
  `format_store_success_with_note` (`tools.rs:936`) already prepends/appends a notice —
  the notice/footer of AC-2/AC-3 follows this established pattern.

### Reuse target (confirms AC-5 — no new chain-walk)
- `follow_to_current(store, id) -> Option<u64>`
  (`graph_read_neighbors.rs:36-55`, canonical production copy). 50-hop cap; returns
  `Some(terminal)` when it reaches an **Active** terminal; returns `None` on: orphaned
  deprecated terminal (`superseded_by IS NULL`, `status != Active`), chain > 50 hops, or
  store error. Caller uses the original ID as fallback (ADR-005 R-10). **This is the exact
  primitive `context_get` should call.** AC-5 reuse claim is SOUND.
- `query_current_terminal(pool, id) -> Result<Option<EntryRecord>>`
  (`graph_queries.rs:161-201`). SQL recursive CTE with a mandatory `AND e.status = 0`
  (Active) terminal filter (`graph_queries.rs:180`) — guards orphaned deprecated terminals
  (R-20). Returns the **full EntryRecord**, so a resolved get needs no second fetch shape.
- `handle_current` (`graph_read_supersession.rs:86-103`) wraps `query_current_terminal` but
  **returns `Err` on orphaned/non-existent**. This is the wrong primitive for AC-4, which
  requires "return what's found with a loud flag, never empty." Use `follow_to_current`
  (+ fallback fetch), NOT `handle_current`. See Open Question OQ-2.

### Consistency landscape (the naming decision)
- `context_graph` already exposes a boolean `resolve_supersessions: Option<bool>`, **default
  `false`** (`graph_read.rs:84`; unwrap default `graph_read_subgraph.rs:158`,
  `graph_read_path.rs:135`; tool doc `tools.rs:96,104`). Same underlying concept
  (follow supersession to terminal) across neighbors/subgraph/path modes. On `chain` mode
  it is rejected as semantically circular (`graph_read_validation.rs:61`).
- **Default tension (must surface):** graph defaults **false** (no follow); vnc-042 decided
  `context_get` defaults to **follow-to-current**. Same concept, opposite defaults across
  two tools — central to the naming decision below.

### Supporting knowledge (Unimatrix briefing)
- #4468 — supersession chain traversal must use SQL recursive CTE, never in-memory
  `find_terminal_active`. (`query_current_terminal` complies.)
- #4538 — a graph mode with an unconditional `status=0` guard makes deprecated-only params
  meaningless; the `status=0` terminal filter is load-bearing for the orphaned-terminal
  guard.
- #4494 — when substituting a deprecated node for its terminal, insert/track carefully to
  avoid dropping the substitution (relevant to `include_edges` interaction — see NG-1).
- #3728 — a prior MCP integer-serialization bug made `context_get` fail on quoted `id`;
  `GetParams.id` already uses `deserialize_i64_or_string`. New param must not reintroduce
  type-coercion fragility (keep it a plain `Option<bool>` / string enum, `#[serde(default)]`).

## Proposed Approach
Add one resolution parameter to `GetParams` (name TBD — see Open Questions). Handler logic:

1. Resolve the effective id. If resolution is **on** (default): call
   `follow_to_current(&self.store, id)`.
   - `Some(terminal)` and `terminal != id` → fetch terminal, build response, **prepend**
     the one-line notice `↻ Requested #{id} (deprecated) → returning current version
     #{terminal}.` (AC-2).
   - `Some(terminal)` and `terminal == id` → clean passthrough, **no** notice (AC-2).
   - `None` (orphaned / quarantined terminal / >50 hops) → fall back, fetch the found
     entry, attach a **loud** "no active successor found" flag; never empty/silent (AC-4).
2. If resolution is **off** (escape hatch): fetch exactly as today; if the entry is
   deprecated, append the non-intrusive footer
   `deprecated; superseded by #{X} (pass <off-value> to follow)` (AC-3).
3. `format` (null/markdown/json) and `include_edges` are **orthogonal** — resolution picks
   *which* entry; `format` renders it; `include_edges` surfaces the resolved entry's edges
   for free (AC-7). Confirmed orthogonal: format handling (`tools.rs:1000`) and edge
   assembly (`tools.rs:988-997`) act on whichever entry/id resolution selects.

This is a **surgical single-tool contract change** in `tools.rs` reusing existing store
helpers. No schema, no new SQL, no changes to other tools.

## Acceptance Criteria
(Carried verbatim from GH #843 — LOCKED. Renumbered to AC-IDs for pipeline tracing.)
- **AC-01** `context_get(id)` where `id` is deprecated returns the **full content of the
  active terminal**, identical in shape to a direct get of that terminal.
- **AC-02** When resolution hops, the response carries the one-line
  `↻ Requested #X (deprecated) → returning current version #Y` notice; when no hop occurs,
  there is no notice (clean passthrough).
- **AC-03** The escape-hatch value returns the entry **exactly as stored** for any status,
  with the deprecated-footer pointer when the requested entry is deprecated.
- **AC-04** A chain terminating on a non-active entry (orphaned / quarantined) returns a
  result with a **loud non-active flag** — never empty, never silent. Mirrors `current`-mode
  R-20 guard.
- **AC-05** Resolution reuses `follow_to_current` (hop cap + fallback); **no new chain-walk**
  implementation is added.
- **AC-06** Resolution is **on by default** when the parameter is omitted (the contract
  change is the new default).
- **AC-07** The parameter composes correctly with `format` and `include_edges` (orthogonal).

## Constraints
- **C-1** Reuse `follow_to_current` / `query_current_terminal`; do not reimplement
  chain-walking (AC-5; Unimatrix #4468).
- **C-2** Backward-compatible deserialization: new field `#[serde(default)]` so pre-vnc-042
  callers that omit it deserialize to the default-on path (mirrors `include_edges`, AC-6).
- **C-3** 50-hop cap and orphaned-terminal (`status=0`) guard are load-bearing — must not be
  weakened (R-20 / #4538).
- **C-4** No `.unwrap()` in non-test code; errors via project error type + `.map_err`
  (rust-workspace rules). Post-primary-read failures stay FAIL-LOUD, consistent with the
  existing `include_edges` FR-19 handling (`tools.rs:984-987`).
- **C-5** Tool-description strings for `context_get` (`tools.rs:947-948`) must be updated to
  document the new default and parameter — a description that lies to agents is a known
  hazard (#4303).
- **C-6** **Requires an ADR** — this changes the default behavior of the most-used read
  tool. Architect authority (issue Notes). The ADR must explicitly rule on graph-vs-get
  naming/consistency (see OQ-1).

## Dependencies
- **Depends on:** existing `follow_to_current` (`graph_read_neighbors.rs`),
  `query_current_terminal` (`graph_queries.rs`), `supersedes`/`superseded_by` columns
  (`schema.rs:67,69`; `db.rs:554-555`) written by `context_correct`. All present — no
  upstream feature blocks this.
- **Blocks / enables:** the deferred follow-up for stale neighbor targets in `include_edges`
  (NG-1) can build on the resolution helper wired here.
- **ADR:** required before delivery (C-6).

## Open Questions

### OQ-1 (PRIMARY — human decision) Parameter name + shape
The issue proposed a `version` enum (`"current"` default | `"exact"`). The human's objection
stands: **"version" implies multiple versions of the `context_get` command** and is
ambiguous. The MEANING and defaults are correct; only the label is contested.

**Recommendation: `follow_supersessions: Option<bool>`, default `true`
(true = resolve to current; false = exact as stored).**

Rationale:
- **Binary concept → boolean is the honest shape.** The behavior is exactly two-valued:
  follow-to-current vs as-stored. The enum's only cited future value, `"chain"`, is
  **explicitly Out of Scope** for `context_get` (NG-2 — chain lookback lives in
  `context_graph` mode `chain`). With that extension ruled out, the enum's extensibility
  advantage buys nothing here; a boolean carries no false promise of a third mode.
- **Shares the `supersessions` concept-word with `context_graph`'s `resolve_supersessions`**,
  so an agent that greps/learns the supersession knob finds consistent vocabulary across the
  MCP surface (the issue's "one concept, named consistently" intuitiveness win).
- **`follow_*` (not `resolve_*`) is a deliberate signal that the default differs.** Matching
  the graph name *exactly* (`resolve_supersessions`) while flipping the default false→true is
  the worst trap: identical name, opposite behavior when omitted. A distinct-but-related verb
  avoids the same-name/opposite-default footgun while keeping the shared noun.
- **Affirmative and self-documenting:** `follow_supersessions=false` reads naturally as
  "don't follow — give me exactly what I asked for."

Alternatives considered:
- **`version: "current" | "exact"` (issue's original):** rejected per human — implies command
  versioning; ambiguous. Enum extensibility is moot (NG-2).
- **`resolve_supersessions: bool = true` (exact graph name-match):** maximal vocabulary
  consistency, but introduces a same-name/opposite-default divergence (graph defaults false).
  Viable only if the ADR explicitly documents the default divergence and accepts the trap.
  Listed as the close runner-up if the human prioritizes exact cross-tool name identity over
  default-safety.
- **Bare `follow: bool`:** too vague on its own ("follow what?"); loses the `supersessions`
  concept link. Not recommended.

The ADR (C-6) must record the chosen name **and** the graph-vs-get consistency ruling:
(a) accept the divergence with rationale, or (b) standardize later.

### OQ-2 On the AC-4 non-active-terminal path, which entry is returned?
`follow_to_current` returns `Option<u64>` and yields **`None`** (not the stop-id) for
orphaned/quarantined terminals. So on `None`, the cheap path fetches the **originally
requested** id and flags it — it does **not** return the non-active *terminal* it stopped at
(that id is discarded by the helper). AC-4's wording ("returns that entry") is ambiguous:
- (a) return the originally-requested entry with the loud flag — pure `follow_to_current`,
  no new walk, AC-5-clean. **Recommended.**
- (b) return the non-active terminal it stopped at — requires surfacing the stop-id, i.e. a
  helper tweak or a second query, brushing against AC-5's "no new chain-walk."

Recommend (a) for scope tightness; architect/spec to confirm against AC-4 intent.

### OQ-3 Notice/flag rendering across `format` values
AC-2/AC-3/AC-4 notices are specified as human-readable one-liners. For `format="json"`, does
the notice become a JSON field (e.g. `resolution_notice`) or a prepended string? Orthogonality
(AC-7) holds either way, but the JSON shape should be pinned in the spec so downstream
programmatic callers get a stable contract.

## Tracking
- GH Issue: #843 (LOCKED product decision; AC-1..AC-7).
- Feature ID: vnc-042. ADR required before delivery (C-6).
