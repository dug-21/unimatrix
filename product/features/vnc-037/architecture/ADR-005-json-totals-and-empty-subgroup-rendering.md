## ADR-005 (vnc-037): JSON totals as a nested `edge_totals` object (OQ-01); flat ranked markdown list with the `↔` glyph (OQ-02); symmetric edges counted once

> **UPDATED under the next-hop reframe.** The prior decision split markdown into
> Author-asserted / Inferred sub-groups and capped at 10. The reframe **drops the
> sub-split** (the D-09 ranking already front-loads authored), adds the **`↔` glyph** for
> canonicalized symmetric edges (D-10), and makes all totals **count a symmetric edge once**
> (D-05 / D-10). The JSON `edge_totals` shape (OQ-01) is unchanged.

### Context

Two locked decisions need a final rendering call:

- **OQ-01 — JSON total field shape.** D-05 mandates an exact, uncapped count split
  `inbound`/`outbound`, **counting a symmetric edge once** (D-10). Shape choice: a nested
  object `{"inbound": N, "outbound": M}` vs. two scalar top-level keys
  (`inbound_edges`/`outbound_edges`). Either satisfies D-05; pick the one consistent with
  existing response naming.
- **OQ-02 — Markdown grouping.** Originally: when a provenance sub-group (Author-asserted /
  Inferred) is empty, render "Inferred: none" or omit it? **Under the reframe this question
  is moot**: with cap-3 and authored-first ranking (D-09), the sub-grouping itself is
  dropped — so the call becomes *how to render a single flat ranked list with the `↔` glyph*.

### Decision

**OQ-01 — nested `edge_totals` object (unchanged).** Get JSON carries
`"edge_totals": {"inbound": N, "outbound": M}` (a sibling of the `edges` array), not two
scalar keys. Rationale: the existing JSON response surface consistently nests grouped
sub-structures under a named key — `format_status_report` emits
`co_access: { total_pairs, active_pairs, … }`, `correction_chains: { … }`,
`security: { … }`. A nested `edge_totals` matches that house style, keeps the paired counts
unambiguous, and reserves room for a future direction-keyed extension without new top-level
keys. The counts are **post-canonicalization (D-10 / ADR-007): a `↔` edge contributes
once**, to whichever direction its canonical row is anchored — not once to inbound and once
to outbound. Both `edges` and `edge_totals` appear **iff** edges were surfaced (zero-edge:
`edges: []`, `edge_totals: {"inbound":0,"outbound":0}` — D-06 explicit); on opt-out
*neither* appears (D-07).

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
tally):
```
#4501 | … | [tags]    | edges: 3↑ 1↓ ↔2 (2 authored)
#4502 | … | [tags]    | edges: none          ← zero-edge
```
(`↑`/`↓` = asymmetric outbound/inbound counts; `↔N` = symmetric count, each counted once;
`(K authored)` = authored tally. Exact glyph order/form and whether the authored tally
counts the displayed-3 or the full set is OQ-02 in SCOPE — confirm against existing
entry-line conventions; this ADR fixes the *vocabulary*, not the final byte form.)

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
`"edge_totals": {"inbound": N, "outbound": M}` (symmetric counted once).

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
  render; the summary digest's exact glyph form is still an SCOPE OQ-02 detail to confirm.
- **Cross-ref:** ADR-002 (the `GetEdge` fields rendered here, incl. the `"both"` direction),
  ADR-003 (the seam that only emits these on the get-surface path), ADR-006 (the ranking that
  makes the flat list authored-first), ADR-007 (the canonicalization behind `↔` and
  symmetric-once totals).
