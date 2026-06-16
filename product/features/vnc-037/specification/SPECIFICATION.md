# vnc-037 — Specification

A **next-hop navigation affordance**: surface an entry's most-relevant, **ranked, capped (≤3)**
depth-1 typed graph edges on `context_get`, with honest **uncapped** split totals.

> Status: Specification (REVISED for the next-hop reframe). Inputs LOCKED — SCOPE.md decisions
> D-01…D-10 and acceptance criteria AC-01…AC-12 are formalized here, not relitigated. Scope
> risks SR-01…SR-14 are reflected as constraints and verifiable acceptance criteria.
>
> **Reframe note.** This revision supersedes the prior edge-dump framing: the display cap is
> **3, not 10**; **ranking (D-09) is the core of the feature** — with 3 slots, *which 3* is the
> whole question; symmetric edges are **canonicalized to one `↔`** before ranking and counting
> (D-10); the markdown author/inferred sub-split is **dropped** (ranking front-loads authored);
> a per-call **latency budget** (AC-12) is added.

---

## Objective

`context_get` returns an entry's fields but not its typed graph relationships. Having read an
entry, an agent gets **no signal about what to read next** — to see what an entry depends on,
contradicts, or is supported by requires a separate `context_graph` neighbors call. This feature
surfaces, **by default** on every `context_get`, a **small ranked set (≤3)** of the entry's
depth-1 typed edges as a next-hop affordance — *not* a complete edge dump — alongside honest
**uncapped** totals split inbound/outbound. It is **read-path only**: no new edge type, no schema
migration, no multi-hop, and no payload change to any tool that does not opt in.

---

## Design Guardrails (governing principles)

1. **Affordance, not dump.** `context_get` edges are a **next-hop navigation affordance**: at
   most **3** genuinely-related entries worth pulling next. Ranking decides which 3 (D-09). The
   full graph remains the province of `context_graph`; the markdown render points there.
2. **Discovery list, not detail view.** The per-edge payload is **exactly**
   `{ edge_type, direction, target_id, target_title, authored }` — only enough for a reader to
   decide whether to read a related entry. **No** metadata, weights, depth, raw `source` string,
   or `source_id` is exposed.
3. **Cap is display-only; totals are honest.** The ≤3 cap never touches the counts. Totals are
   exact, **uncapped**, split inbound/outbound, and count a canonicalized symmetric edge **once**.
   This is what keeps the visible-empty-box feedback loop and high-degree (#744/#745) inbound
   observability intact.

These guardrails are human-directed at scope approval and are binding on every requirement below.

---

## Domain Models (Ubiquitous Language)

- **Entry** — a Unimatrix knowledge record retrieved by `context_get(id)`.
- **Anchor** — the entry being retrieved (the `id` passed to `context_get`); edge direction is
  expressed relative to it.
- **Typed edge** — a row in `graph_edges` (`db.rs:960`) relating two entries by `relation_type`
  (e.g. `Prerequisite`, `Contradicts`, `Supports`, `CoAccess`, `Informs`). `Supersedes` is
  represented separately and excluded here (D-04 / FR-7).
- **Surfaced edge** — a typed edge **rendered on a `context_get` response**, projected to the
  discovery-list shape `{ edge_type, direction, target_id, target_title, authored }`.
- **Asymmetric edge type** — stored as a **single row**; direction (`→`/`←`) is meaningful.
  The asymmetric authored types are **`Prerequisite`** and **`Supports`**.
- **Symmetric edge type** — stored as **two reciprocal rows** (A→B and B→A). Exactly three:
  **`Contradicts`** (authored), **`CoAccess`** (S8 behavioral), **`Informs`** (S1/S2 behavioral).
  After canonicalization (D-10) a symmetric edge is **one bidirectional `↔` edge**; `→`/`←` is
  **not** meaningful for it.
- **Canonicalization** — collapsing a symmetric type's two reciprocal rows into a single `↔`
  edge **before** ranking (D-09) and **before** counting (D-05). Performed in SQL.
- **`edge_type`** — the projected name for `EdgeRecord.relation_type`. One vocabulary, aligned to
  `context_graph`.
- **`direction`** — relative to the anchor:
  - **outbound (`→`)** — anchor is `source_id`; the edge points away to `target_id`.
  - **inbound (`←`)** — anchor is `target_id`; the edge points toward it from the other endpoint.
  - **bidirectional (`↔`)** — a canonicalized symmetric edge; no `→`/`←` arrow is emitted.
- **`target_id`** — the entry id at the far end; the discovery entry point into `context_graph`.
- **`target_title`** — title of `target_id`, resolved by one batched join; **`null`** when the
  target is unresolved (a *dangling* edge — retained, not dropped).
- **`authored`** — boolean. `true` iff `graph_edges.source == 'agent'` (a human or agent declared
  the relationship); `false` for all currently-live inferred sources (co-access / cosine /
  behavioral / S8). The honest binary trust split (NLI dark — ASS-037). See **Domain
  Precondition**.
- **Author-asserted edge** — a surfaced edge with `authored == true`.
- **Inferred edge** — a surfaced edge with `authored == false`.
- **Target-entry confidence** — `entries.confidence` (`db.rs:549`, `REAL NOT NULL DEFAULT 0.0`):
  a cached six-component **Bayesian Beta-Binomial composite** (`unimatrix-engine/src/confidence.rs`;
  helpfulness term is a Beta-Binomial posterior mean, cold-start α₀=3.0/β₀=3.0 — informally
  "Wilson", though not literally a Wilson interval). It is the **rank key for inferred edges** —
  joined via `target_id`. It is **not** `graph_edges.weight` (D-09 rationale, below).
- **Ranking rule (D-09)** — the selection of which ≤3 edges to display: authored first, then
  inferred filling remaining slots by descending target-entry confidence (see FR-9).
- **`EdgeRecord`** — `context_graph`'s per-hop result type (`graph_read.rs:134-144`):
  `{ source_id, target_id, relation_type, direction, depth, metadata }`, direction strings
  `"incoming"|"outgoing"`. The surfaced-edge shape is a **documented projection** (drops
  `source_id`/`depth`/`metadata`, adds `target_title`/`authored`, adds the get-only `↔` direction
  for canonicalized symmetric edges — which MUST NOT leak into the neighbors contract).
- **List-view tools** — `context_search`, `context_lookup`, `context_store`, `context_correct`.
  They share the `entry_to_json` serializer with `context_get` but **never opt in** to edges.

### Domain Precondition (SR-05)

The `authored` boolean is honest **only while all live inferred edge sources are statistical**
(co-access / cosine; NLI dark per ASS-037). The underlying `source` string MUST be kept available
beneath the boolean. If a non-statistical inferred source (e.g. NLI) revives, the boolean would
silently mislabel it — that revival is the **known, documented trigger** to revisit D-03, not a
silent regression.

---

## Functional Requirements

Each requirement is testable; the verification method is named with the mapped acceptance
criteria in the matrix below.

- **FR-1 (D-01, AC-01, AC-11).** `context_get` SHALL surface an entry's depth-1 typed edges in
  **both directions by default**, reading **live `graph_edges` via SQL** (immediate freshness; no
  in-memory snapshot staleness on a point read). A just-written or just-carried-forward (vnc-035)
  edge SHALL be visible on the next `context_get` with no tick wait.

- **FR-2 (D-01, AC-11).** `GetParams` SHALL gain an additive `include_edges: Option<bool>` field.
  Resolution: `None` ⇒ surface (default-on); `Some(true)` ⇒ surface; `Some(false)` ⇒ suppress.
  The field is backward-compatible — a pre-vnc-037 caller sending no field behaves as default-on.
  No existing `GetParams` field is removed or retyped.

- **FR-3 (D-01/D-07, AC-11).** When edges resolve to suppressed (`Some(false)`), `context_get`
  SHALL **skip the neighbor query, the confidence join, the count query, and the title join
  entirely**, pass `None` to the serializer, and produce a response carrying **no `edges` key** —
  indistinguishable from a list-view payload.

- **FR-4 (D-02, AC-02).** Each surfaced edge SHALL carry **exactly**
  `{ edge_type, direction, target_id, target_title, authored }` and nothing more. No `source_id`,
  `depth`, `metadata`, weight, raw `source` string, or any other field SHALL appear in the
  surfaced per-edge payload. (Enrichment is forbidden — guardrail.)

- **FR-5 (D-02, AC-02).** `target_title` SHALL be resolved by **one batched join** (single
  `SELECT id, title FROM entries WHERE id IN (…)`, precedent `fetch_nodes_batch`
  `graph_read_subgraph.rs:568-600`) — never N+1. An unresolved target SHALL yield
  `target_title: null` and the **edge SHALL be retained** (dangling = signal).

- **FR-6 (D-03, AC-03).** `authored` SHALL be `true` iff `graph_edges.source == 'agent'`; all
  currently-live inferred sources SHALL map to `false`. This requires adding the `source` column
  to the neighbor `SELECT` and to `RawEdgeRow` — a **read-path change only, no schema migration**.
  The `source` string SHALL remain available underneath the boolean (Domain Precondition).

- **FR-7 (D-04, AC-04).** `Supersedes` SHALL never appear among surfaced edges, inherited for
  free from the existing `relation_type != 'Supersedes'` SQL filter (ADR #4461). `supersedes` /
  `superseded_by` remain the **sole** supersession representation — no double-representation.

- **FR-8 (D-10, D-05, AC-05, AC-08; SR-08).** Symmetric edge types (`Contradicts`, `CoAccess`,
  `Informs`) SHALL be **canonicalized to a single `↔` edge in SQL BEFORE ranking (FR-9) and
  BEFORE counting (FR-10)**. The two reciprocal rows of a symmetric edge SHALL collapse to one
  surfaced edge bearing the `↔` direction and **no** `→`/`←` arrow. Asymmetric single-row types
  (`Prerequisite`, `Supports`) SHALL be unaffected and retain their directional arrow.
  Canonicalization correctness is an **invariant tested independently on the displayed set AND on
  the totals** (the two SR-08 surfaces).

- **FR-9 (D-09, AC-05, AC-08; SR-09, SR-10, SR-11).** The **≤3** displayed edges SHALL be
  selected by this rule, applied **after** canonicalization (FR-8):
  1. **Authored edges fill slots first.** `Prerequisite` / `Contradicts` / `Supports` with
     `source == 'agent'` take slots before any inferred edge.
  2. **Inferred fills the remainder only if authored < 3.** If ≥3 authored edges exist, **no**
     inferred edge is shown.
  3. **Inferred is ranked by TARGET-ENTRY confidence, descending** — `entries.confidence` of the
     *target* entry, joined via `target_id`. It SHALL **NOT** be ranked by `graph_edges.weight`
     (frozen first-write-wins, outcomes ~always `success` per ass-079 → no discriminating signal).

  The selection SHALL be expressed in SQL with **exactly** this ordering and bound:
  `ORDER BY (source = 'agent') DESC, t.confidence DESC LIMIT 3`
  where `t` is the target-entry join (post-canonicalization). The `LIMIT` value SHALL be the
  single named display-cap constant (C-12, FR-18), not a magic literal `3` inlined in the SQL. The join SHALL be a **LEFT JOIN**
  with explicit deterministic NULL-confidence ordering (e.g. `COALESCE`/`NULLS LAST`) so dangling
  or status-filtered targets are **retained** (FR-5) and rank deterministically (SR-11). This
  `ORDER BY` is locked as a requirement; a discriminating test (FR-9 verification) SHALL prove
  the higher-confidence target ranks first and that edge `weight` does **not** decide order.

- **FR-10 (D-05, D-10, AC-05; SR-08, SR-14).** The response SHALL **always report exact, uncapped
  counts split `inbound` / `outbound`**, computed by a **separate `COUNT(*)` query** run **after**
  symmetric canonicalization (FR-8) so a `↔` edge counts **once**, not twice. The count query
  SHALL be in **SQL**, not Rust-side counting of a materialized full neighbor set. An entry with
  >3 edges SHALL still report the true totals and show the "…N more — use context_graph"
  affordance (FR-12). The inbound/outbound split is load-bearing — it makes inbound degree (and
  the #744 redirect-cap question / #745 orphans) observable.

- **FR-11 (D-01, D-05; SR-14).** The ranked select (FR-9) and the count (FR-10) SHALL **bound
  hub-node fan-out**: a high-degree node SHALL return **3 ranked rows + the two counts**, and the
  full neighbor set SHALL **never** be materialized into memory. Rank-and-limit and count live in
  SQL (`LIMIT 3` / `COUNT(*)`), not in Rust slicing of an unbounded fetch.

- **FR-12 (D-06, AC-06).** A zero-edge entry SHALL render an **explicit empty state** in all three
  formats (inverting omit-at-zero — visibility is the mechanism): summary `edges: none`; markdown
  a visible "No related entries"; json `"edges": []` with direction-split totals of 0.

- **FR-13 (D-07, AC-07; SR-01).** The shared serializer SHALL gain an **optional `edges`
  argument** whose `None` value **emits no `edges` key at all**. `context_get` SHALL pass
  `Some(...)` **only when `include_edges` resolves true**. The DB queries SHALL live in the
  `context_get` handler, not the serializer. Consequently `context_search`, `context_lookup`,
  `context_store`, `context_correct` payloads SHALL remain **byte-identical** to pre-vnc-037 (no
  `edges` key) — an invariant, not a convention.

- **FR-14 (D-08, AC-08; SR-08).** The three formats SHALL render the **same honest split totals**
  and the **same ranked ≤3 set**, as:
  - **summary / null** — a count digest appended to the entry line showing the **true split**,
    distinguishing asymmetric direction from symmetric, plus an authored tally — e.g.
    `… | edges: 5↑ 2↓ ↔3 (2 authored)` (asymmetric out `↑` / in `↓` arrows, symmetric `↔` count,
    authored tally); zero-edge ⇒ `edges: none`. (Exact glyph order/form and whether the authored
    tally counts the displayed-3 or the full set: OQ-02, architect's call.)
  - **markdown** — a `### Related` section **after the footer** showing the **ranked ≤3** set
    (NOT split into Author-asserted / Inferred sub-headers — the ranking already front-loads
    authored). Each line: `- {edge_type} {→|←|↔} #{target_id} "{target_title}"` using `↔` for
    canonicalized symmetric types. When more edges exist than displayed, a single
    `_…and N more — use context_graph_` pointer (directs the reader to the full-graph tool rather
    than implying the get view is complete).
  - **json** — `"edges": [{ edge_type, direction, target_id, target_title, authored }]` (the
    ranked ≤3) plus direction-split, symmetric-once totals.

  The "more than displayed" decision and the `N more` value SHALL be derived from the **same
  named display-cap constant** that bounds the SQL `LIMIT` (FR-9) — `N = total − cap`, with no
  literal `3` in the rendering code (C-12, FR-18).

- **FR-18 (D-05; maintainability — human-directed).** The display cap SHALL be defined as a
  **single named constant** (value **3**, unchanged from D-05) and referenced everywhere the cap
  is applied: the SQL `LIMIT` in the ranked select (FR-9), the "more than displayed" comparison,
  and the `N more` arithmetic in the `…and N more — use context_graph` affordance (FR-12, FR-14).
  No magic literal `3` SHALL appear at any cap-application site. The cap SHALL be modifiable by a
  **one-line edit** to the constant's value, with no other source change required to alter the
  number of edges rendered. Tests SHALL reference the constant rather than hardcoding `3`
  (FR-18 verification, AC-13). The constant's home and name follow the established
  EDGE_SOURCE_NLI / CO_ACCESS_GRAPH_MIN_COUNT constants-location convention (architect sizes the
  exact placement and identifier).

- **FR-15 (D-02/AC-09; SR-06).** The surfaced-edge shape SHALL be documented as an **explicit
  projection of `context_graph`'s `EdgeRecord`** — same `relation_type`(→`edge_type`),
  `target_id`, and `direction` semantics — using one shared edge vocabulary. The projection drops
  `source_id`/`depth`/`metadata`, adds `target_title`/`authored`, and adds the **get-only `↔`**
  direction for canonicalized symmetric edges. The `↔` semantics MUST NOT leak into the neighbors
  contract.

- **FR-16 (AC-09; SR-02).** `context_graph`'s neighbors contract SHALL be unchanged. The
  `RawEdgeRow` / `SELECT` extension for `source` (FR-6) SHALL be **additive only**; `context_graph`
  SHALL consume the same row with **no behavior change** and no `EdgeRecord` wire change.

- **FR-17 (D-09 + vnc-035; SR-10).** Edges **carried forward** via vnc-035 and edges written via
  **`context_edge`** SHALL classify as **authored** — their `graph_edges.source` is `'agent'`, so
  the ranking (FR-9) treats them as author-asserted and gives them slot priority. This SHALL be
  locked by a named test (FR-17 verification).

- **FR-19 (OQ-A RESOLVED — FAIL LOUD; AC-14; SR-13, R-16).** On the **default-on edge path**, if
  the edge/ranked query, the split `COUNT(*)`, or the batched title join **fails AFTER the primary
  entry read has already succeeded**, `context_get` SHALL **FAIL the whole call** — propagating the
  error via the **same error mapping as the primary-read failure path** (mapped `ServerError`, no
  `.unwrap()`/`expect()` on the edge path). It SHALL **NOT** degrade-with-note and SHALL **NOT**
  silently omit edges. **Rationale (locked):** a silent-omit path is **indistinguishable from a true
  zero-edge entry** (FR-12), which would poison the very next-hop signal this feature exists to
  provide; the contract is **one consistent failure shape** — no new partial-success response shape
  is introduced for callers. This requirement is **scoped to the default-on edge path only**: the
  opt-out path (FR-3, `Some(false)`) skips the edge/count/title queries entirely and therefore
  **cannot reach this failure**. See **C-13** for the marker requirement if a degrade path is ever
  reintroduced.

---

## Non-Functional Requirements

- **NFR-1 — Hot-path query bound (SR-12, SR-14).** Default-on adds **two bounded SQL queries** to
  `context_get`: (a) a ranked `LIMIT 3` select with the target-confidence LEFT JOIN, and (b) a
  split `COUNT(*)`, plus one batched title join over ≤3 distinct targets. None SHALL materialize a
  hub node's full neighbor set. The opt-out path (FR-3) MUST add **zero** query cost.
- **NFR-2 — Per-call latency budget (AC-12, SR-12).** The default-on edge path SHALL stay within
  a stated per-call latency budget added over the edge-free `context_get` baseline — **proposed
  ≤ 5 ms p50 / ≤ 15 ms p95**, on a representative store **including a high-degree node**. The
  exact numbers SHALL be **confirmed against a measured edge-free baseline** before locking (the
  proposed figures are unbacked until measured — SR-12). `context_get` is the hottest read and
  also feeds the co-access loop, so per-call cost compounds; this budget is a first-class NFR.
- **NFR-3 — Read-pool usage.** The neighbor/ranked select, the count, and the title join SHOULD
  use the read pool (`read_pool_server()` / ADR #3595), consistent with other read-path queries,
  over indexed columns (`idx_graph_edges_source_type` / `idx_graph_edges_target_type`,
  `entries.id` for the confidence join).
- **NFR-4 — Backward compatibility (D-01).** Additive `Option<T>` field only; existing
  `context_get` callers and all list-view tool payloads are unaffected (ADR-002 vnc-020
  additive-field precedent).
- **NFR-5 — Surgical blast radius.** One new `SELECT` column (`source`), a ranked/limited/
  canonicalized read path + a count query, one serializer seam (optional `edges` arg). Workspace
  rules: no `.unwrap()` in non-test code, ≤500 lines/file.
- **NFR-6 — Cumulative test infrastructure.** New tests extend the existing `context_get` /
  response / graph fixtures; no isolated scaffolding.
- **NFR-7 — Output is an acceptance surface (SR-13).** Because default-on alters output every
  consumer sees, and the cap is 3 (every wrong edge is 1/3 of the visible affordance), the
  summary / markdown / json strings (FR-14) and the ranked set (FR-9) are a reviewable contract:
  rendering and ranking changes are acceptance-gated, not emergent.

---

## Acceptance Criteria

Each criterion carries a verification method. AC-IDs are inherited verbatim from SCOPE.md and flow
downstream.

| AC-ID | Criterion | Maps FR / Decision | Verification |
|-------|-----------|--------------------|--------------|
| **AC-01** | `context_get` surfaces depth-1 edges, both directions, **by default**, reading live `graph_edges`. | FR-1 / D-01 | Integration test: get an entry with known edges, no `include_edges` param → edges present; write a new edge, get again → it appears immediately (no tick wait). |
| **AC-02** | Each surfaced edge carries `{edge_type, direction, target_id, target_title, authored}`; titles resolve in one batched join; an unresolved target yields `target_title: null` and the edge is retained. | FR-4, FR-5 / D-02 | Test asserts exact field set per edge; dangling-target test asserts `null` title + edge retained; query-count assertion proves single batched join. |
| **AC-03** | `authored` is `true` iff `source == 'agent'`; all current inferred sources map to `false`; `source` is added to the read query **without a schema migration**. | FR-6 / D-03 | Test seeds `source='agent'` and `source='co_access'` edges → asserts `authored` true/false; confirm no migration file added. |
| **AC-04** | `Supersedes` never appears in surfaced edges; `supersedes`/`superseded_by` remain the only supersession representation. | FR-7 / D-04 | Test seeds a `Supersedes` edge → asserts it is absent from surfaced edges. |
| **AC-05** | Output renders **≤3** edges, selected by the D-09 rule (authored first; inferred fill only when authored < 3; inferred ranked by target-entry `entries.confidence`); it always reports exact, **uncapped** counts split `inbound`/`outbound` with symmetric edges counted **once** (post-canonicalization); a >3-edge entry shows the "…N more — use context_graph" affordance. | FR-8, FR-9, FR-10, FR-12 / D-05, D-09, D-10 | (a) authored-priority-under-cap: seed >3 edges with ≥3 authored → only authored show. (b) inferred-fill-only-when-authored<3: seed <3 authored + inferred → inferred tops up to 3. (c) ranking-by-target-confidence: two inferred candidates, higher `entries.confidence` target ranks first; assert edge `weight` does NOT decide. (d) totals exact, uncapped, split, symmetric-once. (e) >3 edges → affordance present. |
| **AC-06** | A zero-edge entry renders an explicit empty state in all three formats. | FR-12 / D-06 | Test gets an edge-free entry → asserts summary `edges: none`, markdown "No related entries", json `"edges": []` + zero totals. |
| **AC-07** | `context_get` JSON gains `edges` + direction-split totals; `context_search`, `context_lookup`, `context_store`, `context_correct` payloads are **byte-identical** to pre-vnc-037 (no `edges` key). | FR-13 / D-07 / **SR-01** | **Byte-identity test** (snapshot/golden) for all 4 list-view tool payloads asserts **no `edges` key** and unchanged bytes vs pre-vnc-037; `context_get` payload asserts `edges` + totals present. |
| **AC-08** | summary, markdown, and json each render per D-08 — cap-3 ranked set, `↔` glyph for canonicalized symmetric edges, honest symmetric-once split totals, and the `…and N more — use context_graph` pointer when capped. | FR-8, FR-14 / D-08, D-10 | Format tests assert each rendered string: summary digest with `↔` split, markdown `### Related` ranked-3 with `↔` lines + the single capped pointer, json shape + symmetric-once totals. Assert the markdown author/inferred sub-split is **absent**. |
| **AC-09** | The get edge shape is a documented projection of `context_graph`'s `EdgeRecord` (same `relation_type`/`target_id`/`direction`); `context_graph`'s neighbors contract is **unchanged** (additive only). | FR-15, FR-16 / D-02 / **SR-02, SR-06** | Existing `context_graph` neighbors tests pass unchanged after the `RawEdgeRow`/`SELECT` extension (run empirically); projection documented in ADR; assert `↔` does not appear in neighbors output. |
| **AC-10** | `cargo build --workspace` and `cargo test --workspace` pass; new tests cover the cases below. | FR-1..FR-19 | Build + test run green; all named cases present and asserting (see AC-10 test inventory below). |
| **AC-11** | `GetParams` gains `include_edges: Option<bool>`; `None` and `Some(true)` surface edges, `Some(false)` suppresses them (response then carries **no `edges` key**, and the neighbor/count/join queries are **skipped**); the field is additive and backward-compatible — a pre-vnc-037 caller behaves as default-on. | FR-2, FR-3 / D-01 | Test all three resolutions: assert edges present (None, Some(true)), absent + no `edges` key (Some(false)); assert queries skipped on opt-out (query-count or instrumentation). |
| **AC-12** | The default-on edge path adds two bounded SQL queries (ranked `LIMIT 3` confidence-join select + split `COUNT(*)`) and stays within a **stated per-call latency budget** added over the edge-free `context_get` baseline — **proposed ≤ 5 ms p50 / ≤ 15 ms p95** on a representative store **including a high-degree node**; the exact numbers are **confirmed against a measured baseline** before locking. | NFR-1, NFR-2, FR-11 / **SR-12, SR-14** | Benchmark/latency test: measure edge-free baseline (opt-out path), then default-on, on a representative store with a high-degree node; assert the added delta is within the (confirmed) budget. Record the measured baseline alongside the locked numbers. |
| **AC-13** | The display cap is a **single named constant** (value **3**, unchanged) referenced by the SQL `LIMIT` and the `…and N more` rendering — no magic literal `3` at any cap-application site. Changing the constant changes **only** the number of edges rendered: totals stay exact/uncapped (FR-10) and canonicalization is unaffected (FR-8). | FR-18 / C-12 / D-05 | (a) **single-source**: assert (grep/test) no literal `3` at the SQL `LIMIT` or `N more` sites; tests reference the constant, not `3`. (b) **cap-isolation**: parametrize/override the constant (e.g. cap=2) → assert the rendered set shrinks to that count while inbound/outbound totals and the `↔`-once canonicalization are **byte-unchanged**; restore. Confirms a one-line value edit is the only change needed. |
| **AC-14** | On the default-on path, an edge/ranked-query, split-`COUNT(*)`, or title-join failure that occurs **after** the primary entry read succeeds **FAILS the whole `context_get`** via the same error mapping as the primary-read failure path — never degrade-with-note, never silent edge omission. A **successful get with zero edges** (FR-12 empty state) is **DISTINCT** from a **failed get** and the two are **never conflated**. | FR-19 / OQ-A / **R-16, SR-13** | (a) **edge-query-failure-fails-loud (named, RED)**: inject a failure into the edge/ranked query *after* a successful primary read → assert `context_get` **returns an error** (mapped `ServerError`, run RED per #4876), not a success payload with omitted edges. Repeat the injection for the split `COUNT(*)` and the title join → same mapped failure. (b) **zero-edges-is-not-failure**: assert a genuine zero-edge entry returns a **success** with the explicit empty state (FR-12 — `edges: []` / "No related entries" / `edges: none`), and that this success is **structurally distinguishable** from the (a) error result — no success payload is ever produced by a failed edge path. (c) **static**: no `.unwrap()`/`expect()` on the edge path in non-test code. |

### AC-10 test inventory (named cases — discriminating, not smoke; SR-13)

- **symmetric-canonicalization (SR-08, D-10)** — a `Contradicts` pair stored as both rows
  collapses to **one `↔` edge**, not two, in the **displayed set**; and (separately) the
  inbound/outbound **totals** count it **once**. Two independently-asserted behaviors. Extend to
  `CoAccess` / `Informs`.
- **authored-priority-under-cap (SR-09, D-09)** — with >3 edges where authored ≥ 3, only authored
  edges show; no inferred edge appears.
- **inferred-fill-only-when-authored<3 (SR-09, D-09)** — with <3 authored, inferred edges top up
  to exactly 3.
- **ranking-by-target-confidence (SR-09, SR-11, D-09)** — among inferred candidates, the higher
  `entries.confidence` target ranks first; assert edge `weight` does **not** decide order; a
  dangling/NULL-confidence target is retained and ranks deterministically (NULLS LAST).
- **opt-out (AC-11)** — `include_edges:false` emits no `edges` key and skips the queries.
- **high-degree node hits SQL `LIMIT`, not memory (SR-14, FR-11)** — a node with many edges
  returns **3 rows + the two counts**; the full neighbor set is never read into memory.
- **carried-forward / `context_edge` classifies authored (SR-10, FR-17)** — an edge carried
  forward via vnc-035 or written via `context_edge` has `source='agent'` and is treated as
  authored by the ranking. Locked by name.
- **byte-identity (SR-01, AC-07)** — golden/snapshot of the 4 list-view payloads: no `edges` key,
  unchanged bytes.
- **display-cap-is-a-constant (AC-13, FR-18, C-12)** — no literal `3` at the SQL `LIMIT` / `N more`
  sites (tests reference the named constant); overriding the constant (e.g. cap=2) shrinks **only**
  the rendered set while totals and `↔`-once canonicalization stay unchanged. Confirms one-line
  modifiability.
- **edge-query-failure-fails-loud (AC-14, FR-19, OQ-A, R-16)** — inject a failure into the
  edge/ranked query (and, separately, the split `COUNT(*)` and the title join) **after** a
  successful primary read; assert `context_get` **FAILS** (returns the mapped `ServerError`, run
  **RED** per #4876), never a success payload with omitted edges. Cross-assert that a genuine
  **zero-edge** success (AC-06) is **structurally distinct** from this failure result — the two are
  never conflated. No `.unwrap()`/`expect()` on the edge path.
- existing **zero-edge** (AC-06) and **dangling-title** (AC-02) cases.

**Additional verifiable assertions promoted from scope risks:**

- **AC-07** is the SR-01 byte-identity guarantee — a golden/snapshot assertion that the 4
  list-view payloads have **no `edges` key** and are unchanged vs pre-vnc-037.
- **AC-09** is the SR-02 / SR-06 guarantee — `context_graph` neighbors contract verified
  empirically by the existing neighbors test suite passing after the additive `source` column; the
  get-only `↔` direction does not leak into neighbors output.
- **AC-05 / AC-10** lock SR-08 (symmetric-once on display AND totals — two tests), SR-09 (exact
  `ORDER BY`, discriminating rank test), SR-10/FR-17 (carried-forward authored, named test),
  SR-11 (LEFT JOIN, NULL-confidence deterministic), SR-14 (SQL LIMIT not memory).
- **AC-12** locks SR-12 — measured baseline required before numbers are final; high-degree node in
  scope of the latency case.

---

## User Workflows

1. **Agent studies an entry (default).** An agent calls `context_get(id)` with no `include_edges`.
   The response carries the entry plus its **top ≤3** depth-1 edges (ranked authored-first, then
   by target confidence) and honest direction-split totals (symmetric counted once). The agent
   reads the ≤3 `target_id`s to decide what to pull next, or follows the `…use context_graph`
   pointer for the full graph / multi-hop.

2. **Author closes the feedback loop.** An author who declared `Prerequisite`/`Contradicts`/
   `Supports` edges retrieves the entry and now *sees* them — they rank first, so they take the
   slots. A zero-edge entry renders an explicit empty box, making "asserted nothing" visible at
   the point of consumption.

3. **Latency/payload-sensitive bulk read (opt-out).** A caller doing bulk reads passes
   `include_edges: false`. The neighbor/count/title queries are skipped; the response is edge-free
   and indistinguishable from a list-view payload.

4. **Observing edge loss.** A reader notices an entry's inbound count is lower than expected; the
   direction-split total (FR-10) surfaces the effect of the #744 redirect cap (50) and #745
   historical orphans. This feature **makes that observable**; it does not fix it and is not
   blocked on it.

---

## Documented Non-Bug Behaviors

Correct-by-design behaviors that MUST be encoded as explicit test cases so "emptiness" is
asserted, not mistaken for a defect (SR-07).

- **DNB-1 — Dangling target (FR-5, AC-02).** A surfaced edge whose `target_id` does not resolve to
  a title renders with `target_title: null` and is **retained** (and ranks deterministically,
  NULLS LAST — SR-11). The null is a signal, not a dropped edge.
- **DNB-2 — Corrected-entry transient.** Immediately after `context_correct`, authored outgoing
  edges carry forward (vnc-035) but inferred edges (co-access / Informs) do **not** — they re-earn
  on the next tick. So a just-corrected entry legitimately shows its **authored edges (which now
  rank first and fill the slots)** while inferred candidates are sparse/absent. This is honest
  live state, not a bug; the emptiness MUST NOT be misread as signal. The surfaced view reads live
  `graph_edges`, so it auto-reflects post-carry-forward state (vnc-035 is benign here). *(Under the
  reframe the old author/inferred markdown sub-split is dropped, so this transient is no longer
  visible as an "empty Inferred sub-header" — it manifests only as which edges win the ≤3 slots.)*
- **DNB-3 — Visible zero (FR-12, AC-06).** A genuinely edge-free entry renders an explicit empty
  state in all three formats. The empty box is the intended mechanism.

---

## Constraints

- **C-1 — Read-path only, no schema migration.** Reuse the existing typed-edge model/storage. The
  single net-new column cost is adding `source` to the neighbor `SELECT` / `RawEdgeRow`. No DDL,
  no migration file.
- **C-2 — Do not double-represent supersession.** `supersedes` / `superseded_by` authoritative;
  `Supersedes` typed edge stays excluded (FR-7).
- **C-3 — Do not break `context_graph` neighbors.** The `RawEdgeRow`/`SELECT` change extends a
  shared query as a new consumer; it MUST NOT incompatibly change the neighbors contract (SR-02),
  and the get-only `↔` direction MUST NOT leak into neighbors output (SR-06). Re-verify
  empirically via existing neighbors tests.
- **C-4 — `None` ⇒ key absent** on the shared serializer — a **tested invariant**, not a
  convention. No existing consumer payload changes (SR-01).
- **C-5 — Minimal per-edge payload (guardrail).** Surfaced edges expose **exactly**
  `{edge_type, direction, target_id, target_title, authored}`. No enrichment. Full detail stays in
  `context_graph`.
- **C-6 — Symmetric canonicalization before rank AND count (blocker — SR-08, D-10).** Symmetric
  types (`Contradicts`/`CoAccess`/`Informs`) MUST collapse to one `↔` edge in SQL **before** the
  `ORDER BY…LIMIT 3` and **before** the `COUNT(*)`. "Counted once" is an invariant on **both** the
  displayed set and the totals, tested separately.
- **C-7 — Rank-and-limit in SQL, not Rust (SR-14, FR-11).** The ranked select (`LIMIT 3`) and the
  split `COUNT(*)` MUST execute in SQL. A "fetch-all-then-slice/count-in-Rust" approach is
  prohibited — it satisfies the output contract but violates the memory/latency intent invisibly.
- **C-8 — Locked ranking order (SR-09, D-09).** The displayed-set ordering MUST be exactly
  `ORDER BY (source='agent') DESC, t.confidence DESC LIMIT 3` (target-confidence join, post-
  canonicalization), via a LEFT JOIN with deterministic NULL-confidence ordering (SR-11). Ranking
  by `graph_edges.weight` is prohibited.
- **C-9 — Latency budget is measured, not assumed (AC-12, SR-12).** The AC-12 numbers MUST be
  confirmed against a measured edge-free `context_get` baseline (high-degree node in scope) before
  they are locked. The proposed ≤5 ms p50 / ≤15 ms p95 is provisional until measured.
- **C-10 — `authored` boolean precondition (SR-05).** Honesty depends on all live inferred sources
  being statistical (NLI dark). Keep the `source` string underneath; document revival as the
  trigger to revisit D-03.
- **C-11 — Workspace rules.** No `.unwrap()` in non-test code; ≤500 lines/file; cumulative test
  infrastructure (extend existing `context_get` / response / graph fixtures).
- **C-12 — Display cap is a single named constant, no magic literal (FR-18, AC-13; maintainability,
  human-directed).** The cap value (**3**, unchanged) MUST be defined **once** as a named constant
  and referenced by every cap-application site: the SQL `LIMIT` (FR-9), the "more than displayed"
  comparison, and the `N more` arithmetic (FR-14). No literal `3` at any cap site. Changing the cap
  MUST be a **one-line edit** to the constant; it MUST change **only** the number of edges rendered
  — never the (uncapped) totals (FR-10) and never canonicalization (FR-8). Tests reference the
  constant, not a hardcoded `3`.
- **C-13 — Fail-loud edge contract; no silent-omit, distinct-marker on any future degrade (OQ-A
  RESOLVED; FR-19, AC-14; human-directed).** On the default-on path, a post-primary-read edge/count/
  title failure MUST fail the whole `context_get` via the primary-read error mapping (FR-19). A
  silent-omit (edges absent on failure) is **prohibited** — it is indistinguishable from a true
  zero-edge entry (FR-12) and poisons the next-hop signal. There is **one** failure contract and
  **no** new partial-success response shape. **If a degrade-with-note path is ever reintroduced
  later**, it MUST carry an **explicit "edges unavailable" marker** that is **distinct from "no
  edges"** (the FR-12 empty state) so the two states remain unambiguous to callers — a bare omission
  is never acceptable.

---

## Dependencies

- **ass-076 FINDINGS** (`product/research/ass-076/SCOPE.md` + `FINDINGS.md`, origin #708) — the
  research input; every D-0x traces to an answered RQ.
- **ass-079 FINDINGS** (`product/research/ass-079/`) — the frozen-`Informs`-weight rationale
  grounding D-09's "rank by target confidence, not edge weight" (C-8).
- **Author-asserted-edge convention** (`uni-architect`, `uni-store-adr`) — the assert half; this is
  the surface half. Input, not under revision.
- **vnc-035 carry-forward** (#749) — the surfaced live-`graph_edges` view reflects post-carry edge
  state (DNB-2); carried-forward edges classify authored (FR-17, SR-10).
- **`entries.confidence`** (`db.rs:549`; `unimatrix-engine/src/confidence.rs`) — the inferred-edge
  rank key (C-8, FR-9), joined via `target_id`.
- **`context_graph` `EdgeRecord`** (ADR #4478) + **ADR #4461** (Supersedes SQL exclusion) — the
  shapes/filters this projects from and reuses.
- **`query_direct_neighbors`** (`graph_queries.rs:200-216`) → `run_outgoing_query` /
  `run_incoming_query` (`graph_queries_neighbors.rs:13-92`); **`RawEdgeRow`** gains `source`.
  *Note:* under the reframe the **plain** `query_direct_neighbors(…, Both)` call is no longer the
  sole reuse target — it returns unranked, un-canonicalized, unbounded rows. The read path needs a
  **ranked/limited/canonicalized** SQL variant plus the count query (architect sizes the exact
  SQL/seam; see Open Questions).
- **vnc-037 ADR-001 (#5009)** — predates the reframe (describes reusing the plain neighbor query);
  the architect MUST reconcile it with the revised D-01 rank-and-limit-in-SQL strategy via
  `context_correct` (see Open Questions).
- **`fetch_nodes_batch`** (`graph_read_subgraph.rs:568-600`) — batched-title precedent.
- **`format_single_entry`** (`mcp/response/entries.rs:13-36`), **`entry_to_json`**
  (`response/mod.rs:121-138`), **`format_entry_markdown_section`** (`response/mod.rs:160-191`),
  **`context_get` handler** (`tools.rs:920-1009`) — the edit sites.
- **Relates to #744 / #745** (edge-loss cluster) — this makes their effect observable; no ordering
  dependency either way.
- **GitHub Issue:** #754.

---

## NOT in Scope (explicit exclusions)

- **Multi-hop / traversal at the get layer.** Depth-1 only; `target_id`s are entry points into
  `context_graph`.
- **An edge dump.** This is a ≤3 next-hop affordance, not a complete edge list — the full graph is
  `context_graph`'s domain.
- **A new edge type or schema migration.** Read-path surfacing of existing edges only.
- **Re-representing supersession.** `Supersedes` typed edge stays excluded.
- **Backfilling historical edges.** Deferred (per #708) until assert + surface both exist.
- **Changing the edge-assertion conventions** (`uni-architect` / `uni-store-adr`).
- **Putting edges on `search` / `lookup` / `store` / `correct`.** The serializer gains the
  *capability* (optional arg), but only `context_get` opts in; list views stay byte-identical.
- **Exposing per-edge detail** (metadata, weights, depth, raw `source`, `source_id`) on the get
  payload. That remains `context_graph`'s domain (guardrail / C-5).
- **A `provenance` enum.** `authored` is a boolean for now (D-03); the `source` string is kept
  underneath for future revival, but no enum is built.
- **Ranking by edge weight.** Inferred ranking is by target-entry confidence (C-8); `weight` is a
  frozen, non-discriminating signal (ass-079).
- **The author/inferred markdown sub-split.** Dropped — ranking front-loads authored (FR-14).
- **Fixing edge loss** (#744 redirect cap, #745 orphans). This feature makes loss *observable*,
  not repaired.

---

## Open Questions

- **OQ-01 — JSON total field shape.** `{"inbound": N, "outbound": M}` object vs two scalar keys
  (`inbound_edges` / `outbound_edges`). Either satisfies D-05 / FR-10; architect picks the one
  consistent with existing response naming. **No human decision required.**
- **OQ-02 — summary digest glyph form.** D-08/FR-14 proposes `edges: 5↑ 2↓ ↔3 (2 authored)`.
  Confirm the exact glyph order/form and whether the authored tally counts the displayed-3 or the
  full set; pick a form consistent with existing entry-line conventions. **Architect's call.**
- **OQ-03 — internal-caller default (latency).** Programmatic call sites (the hook path, the
  briefing pipeline, by-ID loop fetches) pay the edge-query cost but never consume the affordance.
  Decide whether (and which) internal callers should pass `include_edges:false` by default, to
  reduce the AC-12 load. Trades latency against keeping a single default-on code path. **Architect
  recommends; touches the latency budget (AC-12) — surface to human if it changes default-on
  behavior for any agent-facing path.**
- **OQ-04 — reconcile ADR-001 (#5009) with the reframe.** ADR-001 predates the reframe and
  describes reusing the **plain** `query_direct_neighbors` (no ranking/canonicalization/limit).
  The revised D-01 requires a ranked-and-limited, canonicalized SQL path + a count query. The
  architect MUST update #5009 via `context_correct` (not deprecate+store) to reflect the new query
  strategy. **No human decision required — architect action.**

**Resolved (human-directed, this revision):**

- **OQ-A — edge/title-join failure handling: FAIL LOUD.** RESOLVED by the human. On a post-primary-
  read edge/count/title failure the whole `context_get` **fails** via the primary-read error
  mapping — no degrade-with-note, no silent edge omission. Locked as **FR-19 / C-13 / AC-14** with
  a named RED failure-path test. Rationale: silent omit is indistinguishable from a true zero-edge
  entry and would poison the feature's core signal; one consistent failure contract, no new
  partial-success shape. No longer open.

**Constraints needing a human decision:** one soft item — **the AC-12 latency numbers are
provisional until a measured baseline confirms them (C-9)**. If the measured baseline shows the
proposed ≤5 ms p50 / ≤15 ms p95 is unattainable on hub nodes with default-on, the human must
choose between relaxing the budget, mandating internal-caller opt-out (OQ-03), or revisiting
default-on. All other scope decisions D-01…D-10 are locked.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced EdgeRecord shape/placement (ADR #4478,
  direction strings `"incoming"|"outgoing"`), additive-`Option<T>`-field precedent (#4503 / #4506 /
  ADR-002 vnc-020), live-SQL vs in-memory neighbors freshness rationale (#4479), Supersedes-SQL-
  exclusion (#4461), vnc-035 carry-forward composition (#4983), and the existing vnc-037 ADR-001
  (#5009) — which **predates the reframe** and must be reconciled by the architect (OQ-04). All
  consistent with locked D-0x decisions except #5009's plain-neighbor-query strategy, flagged for
  `context_correct`.
- Queried (vnc-037-agent-2-spec, cap-constant update): `mcp__unimatrix__context_briefing` —
  surfaced the additive-field precedent (#4503), the vnc-037 ranking ADR carrying the exact
  `LIMIT 3` ordering (#5018), the serializer-seam ADR (#5011), and the crt-034 constants-location
  convention (EDGE_SOURCE_NLI / CO_ACCESS_GRAPH_MIN_COUNT co-located in read.rs). The display-cap
  named-constant requirement (FR-18 / C-12 / AC-13) is additive and consistent with all of these;
  the cap value stays **3**. Constant placement/identifier is the architect's call (follows the
  established constants-location convention).
- Queried (vnc-037-agent-2-spec, OQ-A fail-loud lock): `mcp__unimatrix__context_briefing` — no
  results (Unimatrix MCP unavailable in this environment; non-blocking per spec protocol).
  Proceeded from the in-document prior briefing findings plus the RISK-TEST-STRATEGY R-16 failure-
  path (fail-loud, mapped `ServerError`, run RED per #4876). The OQ-A resolution (FR-19 / C-13 /
  AC-14) is additive and consistent with R-16's documented architecture lean.
