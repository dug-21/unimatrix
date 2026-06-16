## ADR-005 (vnc-037): JSON totals as a nested `edge_totals` object (OQ-01); flat ranked markdown list with the `↔` glyph (OQ-02); symmetric edges counted once

> **UPDATED under the next-hop reframe.** The prior decision split markdown into
> Author-asserted / Inferred sub-groups and capped at 10. The reframe **drops the
> sub-split** (the D-09 ranking already front-loads authored), adds the **`↔` glyph** for
> canonicalized symmetric edges (D-10), and makes all totals **count a symmetric edge once**
> (D-05 / D-10).

> **AMENDED 2026-06-16 (Gate 3a OQ-02 carry-over, Stage 3b).** The totals bucket structure
> is now **THREE buckets** `{inbound, outbound, both}` — `↔` gets its OWN bucket; it is **no
> longer folded into `inbound`**. This revises OQ-01's 2-key JSON shape to 3 keys and is the
> deciding change for the symmetric-count digest form. Rationale and the full TOTALS BUCKET
> CONTRACT are pinned in the new sections below. (Unimatrix MCP was disconnected at decision
> time — the `context_correct` of the stored ADR entry is **DEFERRED**; re-sync required.)

### Context

Two locked decisions need a final rendering call:

- **OQ-01 — JSON total field shape.** D-05 mandates an exact, uncapped count split,
  **counting a symmetric edge once** (D-10). Shape choice: a nested object vs. scalar
  top-level keys. Either satisfies D-05; pick the one consistent with existing response
  naming. *(2026-06-16: the object now carries THREE keys — see the TOTALS BUCKET CONTRACT.)*

- **OQ-02 carry-over — the symmetric count cannot be expressed.** Gate 3a deferred the exact
  summary-digest byte form. The proposed digest `edges: 5↑ 2↓ ↔3 (2 authored)` needs the
  symmetric count *split out*, but the validated `EdgeTotals{inbound, outbound}` folded `↔`
  into `inbound`, so there was no `↔` bucket to read. This forced a false choice (widen an
  ad-hoc third field, or drop `↔N`). The root fix is the bucket structure itself: a `↔` edge
  is semantically **neither** inbound nor outbound, and folding it into `inbound` **corrupts
  the #744/#745 inbound-degree observability signal the split exists to serve** (a node with
  5 `CoAccess` + 0 true inbound would read as `inbound:5`, a false high-inbound degree).
- **OQ-02 — Markdown grouping.** Originally: when a provenance sub-group (Author-asserted /
  Inferred) is empty, render "Inferred: none" or omit it? **Under the reframe this question
  is moot**: with cap-3 and authored-first ranking (D-09), the sub-grouping itself is
  dropped — so the call becomes *how to render a single flat ranked list with the `↔` glyph*.

### Decision

**OQ-01 — nested `edge_totals` object, now THREE keys.** Get JSON carries
`"edge_totals": {"inbound": N, "outbound": M, "both": S}` (a sibling of the `edges` array),
not scalar keys. Rationale (nesting): the existing JSON response surface consistently nests
grouped sub-structures under a named key — `format_status_report` emits
`co_access: { total_pairs, active_pairs, … }`, `correction_chains: { … }`,
`security: { … }`. A nested `edge_totals` matches that house style and keeps the counts
unambiguous. The counts are **post-canonicalization (D-10 / ADR-007): a `↔` edge contributes
once** — to the `both` bucket, never split across inbound/outbound and never folded into
inbound. Both `edges` and `edge_totals` appear **iff** edges were surfaced (zero-edge:
`edges: []`, `edge_totals: {"inbound":0,"outbound":0,"both":0}` — D-06 explicit); on opt-out
*neither* appears (D-07).

---

### TOTALS BUCKET CONTRACT (locked 2026-06-16 — binding on all three formats)

This is the single source of truth that resolves Gate 3a's deferred OQ-02. It supersedes
the two-bucket `{inbound, outbound}` shape everywhere it appeared in vnc-037 artifacts.

**1. Bucket structure — THREE buckets.** `EdgeTotals { inbound, outbound, both }` and its
store sibling `EdgeCountSplit { inbound, outbound, both }`. All `usize`, all **uncapped**,
each edge counted **exactly once**:

| Bucket | What it counts | Direction value of the canonical row |
|--------|----------------|--------------------------------------|
| `outbound` | asymmetric edges anchored at `source_id` (`Prerequisite`/`Supports` where reader is source) | `'outbound'` (renders `→`) |
| `inbound` | asymmetric edges anchored at `target_id` (`Prerequisite`/`Supports` where reader is target) | `'inbound'` (renders `←`) |
| `both` | canonicalized symmetric edges (`Contradicts`/`CoAccess`/`Informs`), counted ONCE | `'both'` (renders `↔`) |

**Why three, not two (the deciding factors).**
- **Honesty.** A `↔` edge is semantically neither inbound nor outbound; giving it a distinct
  bucket states the truth instead of arbitrarily attributing it.
- **#744/#745 observability.** The split exists to give a *clean asymmetric-inbound degree*
  (redirect-cap / orphan detection). Folding `↔` into `inbound` corrupts exactly that signal.
  With `both` separated, `inbound` is now the **true asymmetric inbound degree** — the
  observability goal is served, not undermined.
- **Digest expressibility.** The symmetric count the digest must show (`↔3`) reads directly
  off `both`; no ad-hoc type widening, no dropping `↔N`.

**Invariants (unchanged, now exact per bucket):** symmetric counted ONCE (in `both`); totals
UNCAPPED (never reference `GET_EDGE_DISPLAY_LIMIT`); the cap is display-only. The uncapped
grand total used by the markdown `…N more` threshold is `inbound + outbound + both`.

**2. Summary-digest byte form (locked).** Appended to the entry line by the get path only:

```
#4501 | … | [tags]    | edges: 3↑ 1↓ ↔2 (2 authored)
#4502 | … | [tags]    | edges: none          ← zero-edge (all three buckets 0)
```

Exact form: `" | edges: {outbound}↑ {inbound}↓ ↔{both} ({K} authored)"`.
- `{outbound}` → `↑`, `{inbound}` → `↓`, `{both}` → `↔` (no space between `↔` and its count;
  single spaces between the three terms and before `(`).
- Each segment is **always present** even when its bucket is 0 (e.g. `0↑ 0↓ ↔4 (1 authored)`)
  — fixed arity keeps the string a stable acceptance surface (SR-04/NFR-7); the only special
  case is the all-zero collapse to `edges: none`.
- **Zero-edge sentinel:** when `inbound == 0 && outbound == 0 && both == 0`, render exactly
  `" | edges: none"` (no count terms, no authored tally).
- The counts come from the **uncapped** `EdgeTotals` (the honest split), never from the
  displayed ≤cap set.

**3. `(K authored)` tally — counts the FULL uncapped set, not the displayed ≤3.** The tally
is `authored` over the *entire* canonicalized neighbor set, not over the rendered ≤3. Why:
the digest is the *honest summary line* — every other number on it (`↑ ↓ ↔`) is uncapped, so
mixing in a cap-scoped authored count would be internally inconsistent and would make
`(K authored)` silently shrink as the cap retunes. An agent reading `0↑ 0↓ ↔9 (7 authored)`
learns "7 of my 9 symmetric relations are agent-asserted" — a real provenance signal only
meaningful against the full set. This requires the split-count query to also aggregate an
authored count (a `SUM(CASE WHEN source = 'agent' THEN 1 ELSE 0 END)` over the same
`deduped` CTE), surfaced as a fourth scalar **for the digest only** — it is **NOT** a JSON
`edge_totals` key and **NOT** rendered in markdown (those formats already carry per-edge
`authored`). See component changes below.

**OQ-02 — drop the author/inferred sub-split; render a flat ranked list with `↔`.** The
markdown `### Related` section renders the **ranked ≤3** as a single flat list — **no**
`**Author-asserted**` / `**Inferred**` sub-headers. Rationale: the D-09 ranking already
front-loads authored edges into the top slots, so a provenance sub-grouping is redundant and
re-introduces the empty-sub-group problem the old OQ-02 tried to manage. A flat ranked list
is what a next-hop affordance wants: the three best next reads, in order. Direction glyph per
edge: `→`/`←` for asymmetric types (`Prerequisite`/`Supports`), **`↔` for canonicalized
symmetric types** (`Contradicts`/`CoAccess`/`Informs`, D-10 / ADR-007) — never a spurious
`→`/`←` for a symmetric edge. When more than 3 edges exist, a single
`_…and N more — use context_graph_` pointer directs the reader to the full-graph tool
(rather than implying the get view is complete).

Canonical renderings:

summary (count digest on the entry line; D-08 — true split, `↔` for symmetric, authored
tally; **byte form LOCKED in the TOTALS BUCKET CONTRACT above**):
```
#4501 | … | [tags]    | edges: 3↑ 1↓ ↔2 (2 authored)
#4502 | … | [tags]    | edges: none          ← zero-edge (all three buckets 0)
```
(`{outbound}↑ {inbound}↓ ↔{both} ({K} authored)`; all three count segments always present;
`↔` reads the `both` bucket; `(K authored)` = authored over the **full uncapped** set, not
the displayed ≤3; all-zero ⇒ `edges: none`.)

markdown (`### Related` after the footer; flat ranked ≤3; `↔` glyph; cap affordance):
```
### Related
- Supports → #4461 "Supersedes Exclusion …"
- Prerequisite ← #4478 "EdgeRecord Type Location …"
- Contradicts ↔ #4502 "Loop-level exclusion proposal"
_…and 12 more — use context_graph_     ← only when the displayed list was capped at 3
```
zero-edge markdown:
```
### Related
No related entries.
```

json (per OQ-01 above): `"edges": [ {edge_type, direction, target_id, target_title,
authored} ]` (the ranked ≤3; `direction` is `"both"` for `↔` edges) plus
`"edge_totals": {"inbound": N, "outbound": M, "both": S}` (three buckets; symmetric counted
once in `both`). The digest-only authored tally is **not** a JSON key.

### Consequences

- **Easier:** JSON totals match the established nested-group convention, so consumers and
  maintainers find them where expected; markdown is a clean flat ranked list — no sub-group
  boilerplate, the best three next reads in order, and the corrected-entry transient (sparse
  inferred) simply shows fewer/lower-ranked edges rather than an awkward empty sub-section;
  the `…N more — use context_graph` pointer keeps the get view honestly incomplete.
- **Harder:** "symmetric counted once" must be a **tested invariant on both surfaces
  independently** — the displayed list AND the split totals — because a canonicalization miss
  double-counts silently (SR-08, #4083); the `↔` glyph adds a third direction value the
  renderers must handle (the `"both"` case) so each format needs an asserted symmetric-edge
  render. The three-bucket totals add a **third JSON key** (`both`) and a **fourth aggregate**
  (the digest-only authored count) to the split-count query — both must be asserted, and the
  `both` bucket must be tested as *distinct from* `inbound` (the #744 signal regression guard:
  a `↔` edge increments `both`, never `inbound`).
- **Cross-ref:** ADR-002 (the `GetEdge` fields rendered here, incl. the `"both"` direction),
  ADR-003 (the seam that only emits these on the get-surface path), ADR-006 (the ranking that
  makes the flat list authored-first), ADR-007 (the canonicalization behind `↔` and
  symmetric-once totals).

### Stage 3b component changes required by the three-bucket contract (Wave 2)

The locked contract changes four components. Each is a small, additive delta:

1. **`store-split-count` (`graph_queries_ranked.rs::count_neighbors_split`)** — return
   `EdgeCountSplit { inbound, outbound, both }` (add `both`). Over the **same `deduped` CTE**:
   - `both` = `COALESCE(SUM(CASE WHEN direction = 'both' THEN 1 ELSE 0 END), 0)`.
   - `inbound` = `…WHEN direction = 'inbound'…` **only** (drop the old `IN ('inbound','both')`
     fold — `↔` no longer counts as inbound).
   - `outbound` unchanged (`…WHEN direction = 'outbound'…`).
   - Add a **fourth aggregate for the digest only**: `authored` =
     `COALESCE(SUM(CASE WHEN source = 'agent' THEN 1 ELSE 0 END), 0)` over `deduped`. Carry it
     out of the store as a field of `EdgeCountSplit` (e.g. `authored: usize`) **or** a sibling
     return — it feeds the digest's `(K authored)` and is NOT a JSON/markdown key.
   - Retire the "↔ bucketed into inbound" convention text and its test; replace with a
     `both`-bucket test (`↔` ⇒ `both += 1`, `inbound` unchanged) — the #744 regression guard.

2. **`EdgeTotals` (`mcp/response/edges.rs`)** — add `pub both: usize`. Serializes to the
   three-key `edge_totals` object. Update `test_edge_totals_inbound_outbound_object` to assert
   `obj.len() == 3` and the `both` key. (The digest authored tally is **not** a field of
   `EdgeTotals` — it rides in `EdgesView` or is threaded from the assembly; see below.)

3. **`EdgesView` / get-edge-assembly (`mcp/get_edges.rs`)** — project `EdgeCountSplit` →
   `EdgeTotals { inbound, outbound, both }`; thread the digest-only authored tally into
   `EdgesView` (e.g. add `pub authored_total: usize`) so the summary renderer can read it
   without re-deriving from the capped `edges` vec.

4. **`serializer-seam` (`mcp/response/edges.rs` render helpers)** —
   - `render_summary_digest`: emit `" | edges: {outbound}↑ {inbound}↓ ↔{both} ({authored_total} authored)"`;
     all-zero ⇒ `" | edges: none"`. **Removes** the prior "widen vs drop ↔" dilemma — `both`
     is read directly; `authored_total` comes from the view, not the displayed set.
   - `render_json_edges`/json branch: emit `edge_totals` with three keys
     `{inbound, outbound, both}`.
   - `render_markdown_related`: `…N more` threshold/arithmetic uses
     `total = inbound + outbound + both` (still `> GET_EDGE_DISPLAY_LIMIT`, no literal 3).
     Markdown per-edge rendering is unchanged.

Note: `RawEdgeRow` and the ranked select are **unaffected** — the `both`/authored aggregates
live only in the count query and the render path. The canonicalization CTE stays byte-shared
between the ranked select and the count (ADR-007 parity).
