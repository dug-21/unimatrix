# Risk-Based Test Strategy: vnc-037

> **REVISED under the next-hop reframe.** The feature is no longer an edge *dump* capped
> at 10 in Rust — it is a **ranked, capped (≤3) next-hop affordance** whose core is the
> selection rule. That reframe shifts the risk surface: **which 3** is now the acceptance
> surface, and three new failure modes dominate — **symmetric canonicalization** (FR-8,
> the SR-08 blocker: a miss double-renders AND double-counts), **ranking correctness**
> (FR-9, SR-09: wrong `ORDER BY` silently degrades), and **default-on latency** on the
> hottest read (FR-11/NFR-2, SR-12). Under cap-3 every canonicalization/ranking/classification
> defect is **1/3 of the visible affordance** (SR-13), so the relevant tests must be
> **discriminating, not smoke**.

This strategy identifies what could fail in THIS revised design — the SQL rank-and-limit
+ confidence-JOIN + canonicalization path (ADR-001/006/007), the additive neighbor
`SELECT`/`RawEdgeRow` (ADR-004), the shared serializer seam (ADR-003), the batched title
join (ADR-002), and default-on output — and maps each risk to concrete, discriminating
test scenarios with coverage requirements, prioritized by severity × likelihood.

> **Historical grounding (applied below):**
> - **#3886** (crt-034) — *an `ORDER BY ... LIMIT` ranking test is non-discriminating
>   unless the discriminating value falls OUTSIDE the cap.* Directly governs R-04
>   (ranking-by-target-confidence): the higher-confidence target that proves the rank key
>   must be seeded so a batch-local / weight-instead-of-confidence bug produces a *different*
>   visible top-3, else the test passes for both correct and buggy code.
> - **#3645** (col-030) — *ranking expected values cannot be intuited; trace the algorithm.*
>   The cap-3 selection scenarios (authored-priority, inferred-fill, tie-break) need an
>   explicit per-edge rank trace in the test plan.
> - **#3621** (col-029) — *JOIN-heavy SQL must be traced against explicit scenarios before
>   Gate 3a.* Governs the confidence-LEFT-JOIN + canonicalization query (R-04, R-11).
> - **#1044** (crt-018) — risk-based strategy predicted and caught a COUNT bug at
>   implementation time; the split `COUNT(*)` here (R-03) is the same shape.
> - **#1268** (real serializer path, not hand-crafted), **#4876** (error propagation
>   verified empirically), **#4166/#4162** (allowlist/status JOIN on graph_edges must guard
>   BOTH endpoints / ALL passes), **#4247** (SQLite COUNT-DISTINCT separator collision) —
>   elevate R-02, R-06, R-09, R-12.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | **Symmetric canonicalization miss (SR-08 blocker, FR-8, D-10).** `Contradicts`/`CoAccess`/`Informs` store as two reciprocal rows; `query_direct_neighbors(Both)` `extend`s with no dedup. If canonicalization is absent, wrong, or applied only to one surface, the edge **double-renders** (consumes 2 of 3 slots) AND **double-counts** the split totals. Two independent surfaces (display + totals), each can fail alone. | High | High | **Critical** |
| R-02 | **Ranking ORDER BY silently wrong (SR-09, FR-9, D-09).** A wrong key — ranking by `graph_edges.weight` instead of `t.confidence`, dropping `(source='agent') DESC`, or a batch-local vs global selection — produces a plausible-but-wrong top-3. No error, no build signal; under cap-3 a single bad edge is 1/3 of the surface. #3886: the test is non-discriminating unless the proof value sits OUTSIDE the cap. | High | High | **Critical** |
| R-03 | **Split COUNT divergence / canonicalization mismatch (SR-08, FR-10).** Totals computed off the capped Vec (undercount), not split by direction, or counted **before** canonicalization (so `↔` counts twice) — defeating the honest-totals + #744 inbound-degree observability goal. The count query and the rank query must canonicalize **identically**. | High | Med | **Critical** |
| R-04 | **Rank-and-limit done in Rust, not SQL (SR-14, FR-11, C-7).** A "fetch all neighbors, sort/slice/count in Rust" implementation satisfies the output contract but pulls a hub node's full fan-out into memory — violating the memory/latency intent invisibly (no functional failure to catch it). | High | Med | **Critical** |
| R-05 | **Carried-forward / `context_edge` edges mis-classified as inferred (SR-10, FR-17, D-09).** If a carry-forward (vnc-035) or `context_edge` write stamps `source` as anything but `'agent'`, those edges fall out of the authored bucket and **lose slot priority** — silently degrading the affordance for exactly the corrected entries. No build signal. | High | Med | High |
| R-06 | **Confidence LEFT JOIN skews / drops on bad endpoints (SR-11, FR-9).** A dangling target (no `entries` row) or NULL confidence under an inner JOIN silently **drops the edge** (D-02 says retain); under unspecified NULL ordering it sorts unpredictably, making the inferred rank non-deterministic. #4166: graph_edges endpoint JOINs need explicit guards on both endpoints. | High | Med | High |
| R-07 | **Shared serializer byte-identity regression (SR-01, FR-13, D-07).** An edit threads `edges` (or a non-`None`-key-absent path) through `entry_to_json`/markdown helper, breaking byte-identity for search/lookup/store/correct. #1268: must be proven through the *real* producer, not a hand-crafted snapshot. | High | Med | High |
| R-08 | **Additive `source` column breaks `context_graph` neighbors (SR-02, FR-16, ADR-004).** Wrong column index in `map_edge_row`, row-shape drift across the 4 SELECT branches, or `↔`/canonicalization leaking into the shared plain query so neighbors output changes. | High | Med | High |
| R-09 | **`authored` mislabel (FR-6, D-03).** Boolean computed off the wrong source value, case/whitespace near-miss on `'agent'`, or a live inferred source treated as authored — corrupting the trust split AND the ranking (authored-first depends on this exact predicate). | Med | Med | High |
| R-10 | **Direction/`target_id` projection inverted (FR-4, FR-15, D-02).** Inbound shown as outbound, a spurious `→`/`←` emitted for a canonicalized `↔` edge, or `target_id` pointing back at the anchor — sending the reader to the wrong (or self) entry. | High | Low | High |
| R-11 | **Confidence JOIN / canonicalization SQL untraced — wrong scenario coverage (SR-13, #3621/#3645).** The JOIN-heavy ranked query and its expected top-3 are written by intuition rather than an explicit per-edge rank trace, so a discriminating scenario (e.g. authored-fills-then-inferred-tops-up) is mis-specified and passes vacuously. | Med | Med | High |
| R-12 | **`Supersedes` leak / double-representation (FR-7, D-04).** The `!= 'Supersedes'` filter inheritance broken by the `SELECT`/canonicalization rewrite, surfacing `Supersedes` or double-representing supersession. | High | Low | Medium |
| R-13 | **AC-12 latency budget unbacked / regression on hub nodes (SR-12, NFR-2).** The ≤5ms p50 / ≤15ms p95 numbers are locked without a measured edge-free baseline incl. a high-degree node; the confidence-JOIN + split COUNT land on the hottest read which also feeds the co-access loop (cost compounds). | High | Med | High |
| R-14 | **Opt-out does not actually skip the queries (FR-3, AC-11).** `include_edges:Some(false)` still issues the ranked select / count / title join, or still emits an `edges` key — so the escape hatch (and OQ-03 internal-caller relief for SR-12) is illusory. | Med | Med | High |
| R-15 | **Dangling target dropped or panics (DNB-1, FR-5).** A signal-bearing dangling edge silently disappears (inner JOIN), or a `null` title panics a non-null render path. | Med | Low | Medium |
| R-16 | **Edge-query / title-join failure handling (OQ-A, Error boundaries).** On partial failure the whole get crashes via `.unwrap()`, or OQ-A (fail vs degrade) is decided implicitly/inconsistently. | Med | Low | Medium |
| R-17 | **Format-string drift from the reframed D-08/ADR-005 contract (FR-14).** Summary digest missing the `↔` split, markdown still rendering the *dropped* Author-asserted/Inferred sub-split, wrong capped pointer, or json `edge_totals` not symmetric-once. NFR-7 makes these strings an acceptance surface. | Low | Med | Medium |
| R-18 | **File-size limit (≤500) breached mid-build (OQ-B, NFR-5).** `graph_queries*.rs` / `tools.rs` / `response/entries.rs` overflow, forcing an unplanned split that smears edge logic across modules. | Low | Med | Low |
| R-19 | **Corrected-entry transient misread as a bug (DNB-2, SR-07).** Sparse inferred candidates after `context_correct` (authored carry forward, inferred re-earn next tick) flagged as data loss. Now manifests only as *which edges win the ≤3 slots* (sub-split dropped). | Low | Low | Low |
| R-20 | **`authored` boolean mislabels a future non-statistical source (SR-05, C-10).** An NLI revival is silently labeled inferred because the precondition is undocumented at the code site. | Low | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Symmetric canonicalization miss — double-render AND double-count (Critical)
**Severity**: High **Likelihood**: High
**Impact**: A `Contradicts`/`CoAccess`/`Informs` edge appears **twice** in the ≤3 displayed
set (consuming a precious slot and pushing a real next-hop out) and/or is **counted twice**
in the split totals — breaking both the affordance and the honest-totals/observability goal.
The reframe's headline blocker (SR-08; grounded in #4083/#3618 per the scope assessment).
**Test Scenarios** (two **independent** surfaces — neither alone proves the other):
1. **Displayed-set canonicalization** — seed a `Contradicts` pair stored as **both
   reciprocal rows** (A→B and B→A); `context_get(A)` surfaces **one** `↔` edge, not two.
   Extend the same assertion to `CoAccess` and `Informs`.
2. **Totals canonicalization (separate assertion)** — the same pair contributes **once** to
   the inbound/outbound split totals, not twice. Assert this on the COUNT output directly,
   independent of the displayed set (a fix to render-dedup that misses count-dedup must
   fail here).
3. **Order-of-operations** — canonicalization happens **before** `ORDER BY ... LIMIT 3` and
   **before** `COUNT(*)`: seed >3 symmetric pairs plus an authored edge; assert the
   authored edge still wins a slot (proving symmetric rows were collapsed *before* the cap
   consumed slots).
4. **Asymmetric untouched** — a `Prerequisite`/`Supports` single-row edge keeps its `→`/`←`
   and is **not** collapsed.
**Coverage Requirement**: "Symmetric counted once" is an invariant asserted **separately on
display AND on totals**, for all three symmetric types, with canonicalization proven to
precede both the cap and the count. A passing build with double-counted totals is the
failure mode to exclude.

### R-02: Ranking ORDER BY silently wrong (Critical)
**Severity**: High **Likelihood**: High
**Impact**: The exact rank `ORDER BY (source='agent') DESC, t.confidence DESC ... LIMIT 3`
is the core of the feature. A wrong key degrades the top-3 with no error and no build
signal; under cap-3 one wrong edge is 1/3 of what the reader sees.
**Test Scenarios** (discriminating per #3886 — proof value OUTSIDE the cap; traced per #3645):
1. **ranking-by-target-confidence, proof-outside-cap (#3886)** — seed inferred candidates so
   the **higher-confidence target is excluded by a batch-local / wrong-key bug but included
   by the correct global rank**. Concretely: seed ≥4 inferred edges whose target confidences
   straddle the cap boundary, and assert the displayed top-3 contains the high-confidence
   target and **excludes** a lower-confidence one. A naive implementation that orders by
   `weight` (frozen ~constant, ass-079) or by `target_id`/insertion order yields a
   **different** visible set → test fails correctly. Explicitly seed the higher-confidence
   target with a **lower** `graph_edges.weight` so weight-ordering and confidence-ordering
   disagree (weight must NOT decide).
2. **authored-priority-under-cap** — seed >3 edges with **≥3 authored**; assert **only
   authored** show, **no** inferred edge appears (the `(source='agent') DESC` term).
3. **inferred-fill-only-when-authored<3** — seed <3 authored + several inferred; assert
   inferred top up to **exactly 3**, ordered by target confidence.
4. **deterministic tie-break** — equal-confidence inferred targets resolve by the locked
   tiebreak (`target_id ASC`), proven stable across runs.
**Coverage Requirement**: Each scenario carries an **explicit per-edge rank trace** in the
test plan (#3645). The confidence-discriminating case seeds the proof target **outside the
naive/batch-local selection** so a wrong key produces a visibly different top-3 (#3886).
Assert edge `weight` does **not** decide order.

### R-03: Split COUNT divergence / canonicalization mismatch (Critical)
**Severity**: High **Likelihood**: Med
**Impact**: Totals undercount (computed off the capped Vec), aren't split by direction, or
count `↔` twice (canonicalized differently from the rank query) — defeating honest totals
and #744/#745 observability.
**Test Scenarios**:
1. **Capped (>3) totals exact** — seed 8 mixed-direction edges (cap-3 applies); assert the
   rendered set is ≤3 but the totals report the true uncapped split, and the "…N more — use
   context_graph" pointer appears (FR-12).
2. **Canonicalization parity** — the count query and the rank query apply the **same**
   canonicalization: a symmetric pair contributes once to totals AND occupies one slot
   (cross-checks R-01 surface 2 from the count side).
3. **Direction split load-bearing** — high inbound + zero outbound entry reports the true
   inbound count (proves #744 redirect-cap observability survives the cap).
4. **Nested shape** — `edge_totals` is the `{inbound, outbound}` object (OQ-01/ADR-005), not
   a flat or capped scalar.
**Coverage Requirement**: Totals exact, uncapped, direction-split, symmetric-once — asserted
from the COUNT path, with canonicalization parity to the rank query proven.

### R-04: Rank-and-limit in Rust instead of SQL (Critical)
**Severity**: High **Likelihood**: Med
**Impact**: A hub node's full neighbor set is pulled into memory; the latency/memory intent
(SR-14, the reframe's structural reason for moving rank-and-limit into SQL) is violated with
no functional symptom.
**Test Scenarios**:
1. **high-degree-node-hits-SQL-LIMIT** — seed a node with **many** edges (e.g. ≥50); assert
   the ranked query returns **exactly 3 rows** and the count query returns **two scalars** —
   the full neighbor set is **never** materialized. Prove via query instrumentation /
   returned-row-count at the store boundary, not just the rendered output (which a Rust-slice
   bug would also satisfy).
2. **SQL contains `LIMIT`/`COUNT(*)`** — assert the executed statement carries the `LIMIT 3`
   and aggregate, not a `SELECT *` followed by Rust truncation (structural check on the query
   text / a store-level unit test that a 1000-edge fixture allocates ≤3 `RawEdgeRow`).
**Coverage Requirement**: The ranked select returns ≤3 rows and the count returns scalars at
the **store boundary** on a high-degree fixture; the full fan-out is provably never read into
memory (#3621 — trace the JOIN-heavy query against the hub scenario).

### R-05: Carried-forward / `context_edge` edges mis-classified (High)
**Severity**: High **Likelihood**: Med
**Impact**: Edges carried forward via vnc-035 or written via `context_edge` lose authored
slot priority if `source != 'agent'` — silently demoting them for exactly the corrected
entries the feedback loop targets. No build signal.
**Test Scenarios**:
1. **carried-forward-classifies-authored (named, FR-17)** — carry an authored edge forward
   via the vnc-035 path; `context_get` the corrected entry; assert the edge has
   `source='agent'`, `authored=true`, and **wins a slot ahead of inferred** under the
   ranking.
2. **context_edge-classifies-authored** — write an edge via `context_edge`; assert the same
   (`source='agent'`, authored, slot priority).
3. **mixed corrected entry** — corrected entry with authored (carried) + inferred (re-earned)
   edges; assert authored fill slots first (ties to DNB-2 / R-19).
**Coverage Requirement**: Both write paths verified to stamp `source='agent'` and to receive
authored slot priority, locked by named tests (not inferred from R-09's generic split).

### R-06: Confidence LEFT JOIN skews/drops on bad endpoints (High)
**Severity**: High **Likelihood**: Med
**Impact**: A dangling/deprecated target drops the edge (inner JOIN) against D-02's
retain-dangling rule, or NULL confidence sorts unpredictably making inferred rank
non-deterministic.
**Test Scenarios**:
1. **dangling-target-retained-and-ranked** — inferred edge whose `target_id` has no `entries`
   row: assert the edge is **retained** with `target_title:null` and ranks **deterministically
   last** among inferred (NULLS LAST), not dropped.
2. **NULL-confidence deterministic order** — mix resolved + NULL-confidence inferred targets;
   assert a stable, deterministic order across runs.
3. **cold-start uniform confidence** — all inferred targets at the cold-start default (0.0):
   assert the deterministic tiebreak (`target_id ASC`) decides, not arbitrary row order (ties
   to A4 — degenerate ranking under uniform confidence).
4. **LEFT not INNER** — assert the JOIN is LEFT (a store-level test seeding a dangling edge
   and confirming it appears in the ranked output).
**Coverage Requirement**: LEFT JOIN proven (dangling retained); NULL/cold-start confidence
ranks deterministically; no edge dropped by the rank-key join.

### R-07: Shared serializer byte-identity regression (High)
**Severity**: High **Likelihood**: Med
**Impact**: search/lookup/store/correct payloads gain an `edges` key or drift — silently
breaking every downstream consumer (the #3449/#1268 class).
**Test Scenarios**:
1. **Byte-identity golden via real producer (#1268)** — capture the exact JSON/markdown/summary
   bytes of `context_search`/`lookup`/`store`/`correct` by invoking the **real tool handler /
   serializer path** (not hand-authored expected strings); assert **no `edges` key**, no
   `### Related`, no `edges:` digest, and byte-equality vs a pre-vnc-037 baseline.
2. **`None ⇒ key absent` structural** — `entry_to_json` signature unchanged; the `edges` key
   is injected only by the get path; a `None` arg produces a byte-identical payload.
**Coverage Requirement**: All four list-view tools byte-identical across all three formats,
produced through the genuine producer; the baseline is pre-vnc-037 output, not a re-derivation.

### R-08: Additive `source` column breaks `context_graph` neighbors (High)
**Severity**: High **Likelihood**: Med
**Impact**: The shared plain neighbor query returns wrong edges, panics on a column shift, or
leaks `↔`/canonicalization into `context_graph`'s `EdgeRecord`.
**Test Scenarios**:
1. **Existing neighbors suite passes UNEDITED (#4876, empirical)** — run the current
   `context_graph` neighbors tests after the `RawEdgeRow`/`SELECT` extension; green with zero
   edits.
2. **`map_edge_row` across all branches** — the new `source` field populates correctly across
   empty-`edge_types` and `IN(…)`-type SELECTs, both `run_outgoing_query`/`run_incoming_query`
   (#4166: audit ALL passes, not one).
3. **No leakage** — assert `↔` and the canonicalization/confidence logic live only in the
   **ranked variant**; `context_graph` output bytes unchanged and contain no `source` leakage.
**Coverage Requirement**: Neighbors suite green unedited; new field verified on every SELECT
branch; canonicalization/`↔` proven absent from the neighbors contract.

### R-09: `authored` mislabel (High)
**Severity**: Med **Likelihood**: Med
**Impact**: A human-declared edge shown as inferred (or vice versa) corrupts the trust split
**and** the authored-first ranking (R-02 depends on this exact predicate).
**Test Scenarios**:
1. Seed `source='agent'` plus `co_access`/`cosine`/`behavioral`/`S8` edges; assert
   `authored=true` **only** for `'agent'`.
2. **Exact-match guard** — near-miss strings (`'Agent'`, `' agent'`) do **not** flip authored
   true; the predicate is exact (cross-checks the SQL `(source='agent')` term used in R-02).
**Coverage Requirement**: Authored/inferred split asserted across all live source values; the
`'agent'` match is exact and identical between the boolean projection and the rank predicate.

### R-10: Direction / `target_id` projection inverted (High)
**Severity**: High **Likelihood**: Low
**Impact**: The discovery pointer is wrong — inbound shown as outbound, a spurious arrow on a
symmetric edge, or `target_id` pointing back at the anchor.
**Test Scenarios**:
1. Seed one outbound (anchor=`source_id`) and one inbound (anchor=`target_id`) **asymmetric**
   edge; assert `direction` outbound/inbound and `target_id` is the **other** endpoint each.
2. **Symmetric carries `↔`, no arrow (D-02 fix)** — a canonicalized `Contradicts` edge renders
   `↔` and emits **no** `→`/`←`.
3. Cross-check the projected direction against `EdgeRecord`'s `incoming`/`outgoing` mapping
   (FR-15 fidelity).
**Coverage Requirement**: Asymmetric directions + far-endpoint `target_id` correct; symmetric
edges carry `↔` only; projection matches the documented `EdgeRecord` mapping.

### R-11: Untraced JOIN-heavy ranked query — vacuous scenario coverage (High)
**Severity**: Med **Likelihood**: Med
**Impact**: The ranked confidence-JOIN query and its expected top-3 are intuited rather than
traced, so a discriminating scenario is mis-specified and passes for both correct and buggy
code (#3621/#3645).
**Test Scenarios**:
1. Each R-01/R-02/R-06 scenario carries an **explicit per-edge rank trace** in the test plan:
   list edges in (authored, confidence) order, state the rule per edge, derive the expected
   displayed set and totals from the rule (not intuition).
2. The confidence-discriminating seed is verified to place the proof target **outside** a
   batch-local/naive selection (#3886) before the test is accepted.
**Coverage Requirement**: No ranking/canonicalization expected value is intuited; every one is
derived from a written trace, with the discriminating value provably outside the cap.

### R-12: `Supersedes` leak / double-representation (Medium)
**Severity**: High **Likelihood**: Low
**Impact**: Supersession double-represented (violates C-2) because the `!= 'Supersedes'` filter
inheritance broke under the canonicalization/`SELECT` rewrite.
**Test Scenarios**:
1. Seed a `Supersedes` edge on an entry with other typed edges; assert it is **absent** from
   surfaced edges and from the totals.
2. Assert supersession still reflected only via `supersedes`/`superseded_by`.
**Coverage Requirement**: `Supersedes` never surfaces (display **and** totals); no second
representation appears — re-verified against the rewritten ranked/count SQL.

### R-13: AC-12 latency budget unbacked / hub-node regression (High)
**Severity**: High **Likelihood**: Med
**Impact**: The default-on confidence-JOIN + split COUNT land on the hottest read (which also
feeds the co-access loop, compounding cost); an unmeasured budget ships a latency regression.
**Test Scenarios**:
1. **Measured edge-free baseline** — measure the opt-out (`include_edges:false`) `context_get`
   path on a representative store **including a high-degree node**; record it.
2. **Default-on delta within budget** — measure default-on on the same store/node; assert the
   added delta is within the **confirmed** budget (proposed ≤5ms p50 / ≤15ms p95, locked only
   after the baseline measurement — C-9).
3. **Read-pool + indexed JOIN** — assert the ranked select / count / title join run on
   `read_pool_server` over indexed columns (NFR-3), not the write pool.
**Coverage Requirement**: Baseline measured before numbers locked; high-degree node in the
latency case; delta within the confirmed budget; read-pool usage confirmed. (If the baseline
shows the budget unattainable, escalate to the human per the spec's soft decision — relax
budget / mandate OQ-03 opt-out / revisit default-on.)

### R-14: Opt-out does not skip the queries (High)
**Severity**: Med **Likelihood**: Med
**Impact**: The escape hatch (and OQ-03 internal-caller relief for SR-12) is illusory if the
queries still run or an `edges` key still emits.
**Test Scenarios**:
1. **opt-out skip proof** — `include_edges:Some(false)` issues **zero** neighbor/count/title
   queries (query-count or instrumentation) and the response carries **no `edges` key** —
   byte-indistinguishable from a list-view payload.
2. **all three resolutions** — `None` and `Some(true)` surface; `Some(false)` suppresses.
3. **internal-caller opt-out (OQ-03)** — enumerated internal call sites (hook path, briefing
   by-ID fetches, by-ID loop fetches) pass `Some(false)`; assert each as a test (per ARCH OQ-03).
**Coverage Requirement**: Opt-out proven to skip **all** edge queries and emit no `edges` key;
the named internal call sites are asserted, not assumed.

### R-15: Dangling target dropped or panics (Medium)
**Severity**: Med **Likelihood**: Low
**Impact**: A signal-bearing dangling edge disappears, or a `null` title panics a render path.
**Test Scenarios**:
1. **dangling-title-retained (DNB-1, named)** — edge whose `target_id` has no `entries` row:
   `target_title:null`, edge retained across all three formats, no panic. (Rank behavior in
   R-06.)
2. Mixed resolved + dangling: the dangling one does not drop resolved ones; no `.unwrap()`
   panic on the null path.
**Coverage Requirement**: Dangling edge retained with `null` title across all formats; no
unwrap panic.

### R-16: Edge-query / title-join failure handling (Medium)
**Severity**: Med **Likelihood**: Low
**Impact**: Failure crashes the get via `.unwrap()`, or OQ-A (fail vs degrade) is decided
implicitly/inconsistently.
**Test Scenarios**:
1. Inject a ranked-query failure; assert the documented OQ-A behavior (arch leans **fail** with
   mapped `ServerError`, no `.unwrap()`), verified by **running it red** (#4876).
2. Inject a count-query and a title-join failure; assert the same mapped path.
3. Static check: no `.unwrap()`/`expect()` on the edge path in non-test code.
**Coverage Requirement**: Failure paths empirically exercised (not reasoned); error mapping
matches the primary-read pattern; OQ-A resolved before implementation; no unwrap on the edge path.

### R-17: Format-string drift from the reframed contract (Medium)
**Severity**: Low **Likelihood**: Med
**Impact**: Output diverges from the locked D-08/ADR-005 contract (NFR-7 makes strings an
acceptance surface).
**Test Scenarios**:
1. **summary** digest snapshot with the **`↔` split** form (e.g. `edges: 5↑ 2↓ ↔3 (2 authored)`,
   OQ-02 form); zero → `edges: none`.
2. **markdown** `### Related` after footer, **flat ranked ≤3** list (assert the
   Author-asserted/Inferred sub-split is **absent** — dropped under the reframe), `↔` glyph on
   symmetric lines, single `_…and N more — use context_graph_` pointer on cap.
3. **json** `edges` array of the exact 5-field shape + nested `edge_totals` (symmetric-once).
**Coverage Requirement**: Each format asserted against the **reframed** contract; the dropped
sub-split is explicitly verified absent; the `…use context_graph` pointer present on overflow.

### R-18: File-size limit breach (Low)
**Severity**: Low **Likelihood**: Med
**Test Scenarios**: Line-count check on `graph_queries*.rs`, `tools.rs`, `response/entries.rs`,
and any new `graph_queries_ranked.rs` / `mcp/get_edges.rs` / `response/edges.rs` ≤500 (OQ-B
pre-authorizes the sibling modules).
**Coverage Requirement**: All touched/new files ≤500 lines; any split lands on the
pre-authorized sibling module.

### R-19: Corrected-entry transient misread (Low)
**Severity**: Low **Likelihood**: Low
**Impact**: A correct sparse-inferred state after `context_correct` filed as a bug. Under the
reframe it manifests only as *which edges win the ≤3 slots* (sub-split dropped).
**Test Scenarios**:
1. `context_correct` an entry with authored + inferred edges, then `context_get`: assert
   authored (carried) fill slots first and inferred are sparse by design (DNB-2) — encoded as
   expected behavior.
**Coverage Requirement**: The transient is an asserted expected state, documented in the test.

### R-20: Future non-statistical source mislabel (Low)
**Severity**: Low **Likelihood**: Low
**Test Scenarios**: A comment/doc precondition (C-10) at the `authored` site names NLI revival
as the trigger to revisit D-03; the `source` string is retained beneath the boolean.
**Coverage Requirement**: Precondition documented at the code site; `source` preserved (no
information loss).

## Integration Risks

- **The ranked SQL path (R-01, R-02, R-03, R-04, R-06, R-11)** — the densest new integration
  surface: canonicalization → LEFT JOIN confidence → `ORDER BY ... LIMIT 3`, plus a parallel
  split `COUNT(*)` that must canonicalize **identically**. The rank query and count query are
  **two queries that must agree** on canonicalization; a divergence (one dedups, one doesn't)
  is the highest-probability silent defect (#3621: trace the JOIN-heavy query against explicit
  scenarios). Per #4166, the canonicalization/status guards must be applied on **every** pass,
  not one.
- **Shared plain neighbor query (R-08)** — `query_direct_neighbors`/`RawEdgeRow`/`map_edge_row`
  are co-owned by `context_graph` neighbors. The additive `source` column must touch all four
  SELECT branches consistently; the rank/JOIN/canonicalization/`↔` logic must live **only** in
  the new ranked variant (never the shared path) so the neighbors contract and `EdgeRecord`
  shape are byte-stable (empirical, #4876).
- **Serializer seam (R-07)** — `format_single_entry` gains `edges: Option<&EdgesView>`;
  `entry_to_json`/`format_entry_markdown_section` signatures fixed; the key/section injected
  only by the get path. Byte-identity proven through the *real* producer (#1268).
- **Confidence column dependency (R-06, R-13)** — `entries.confidence` JOINed as the rank key.
  Cold-start uniform 0.0 degenerates inferred ranking to the tiebreak (A4); the JOIN lands on
  the hottest read (latency, R-13).
- **vnc-035 carry-forward (R-05, R-19)** — surfaced view reads live `graph_edges`, auto-
  reflecting post-carry state; correctness rests on the carry-forward/`context_edge` write
  stamping `source='agent'` (R-05) and produces the sparse-inferred transient (R-19).

## Edge Cases

Named cases from AC-10, each with a required discriminating test:
- **symmetric-canonicalization** (R-01): a `Contradicts`/`CoAccess`/`Informs` pair → one `↔`,
  asserted on **display AND totals** separately.
- **authored-priority-under-cap** (R-02): >3 edges, authored ≥3 → only authored show.
- **inferred-fill-only-when-authored<3** (R-02): <3 authored → inferred tops up to 3.
- **ranking-by-target-confidence** (R-02, R-06): higher-confidence target ranks first, proof
  value **outside the cap** (#3886), weight does NOT decide; NULL/cold-start deterministic.
- **opt-out** (R-14): `include_edges:false` → no `edges` key, queries skipped.
- **high-degree-node-hits-SQL-LIMIT** (R-04): many edges → 3 rows + two counts, full set never
  read into memory.
- **carried-forward / `context_edge` classifies authored** (R-05): `source='agent'`, slot priority.
- **dangling-title-retained** (R-06, R-15): unresolved `target_id` → `null` title, retained,
  ranks NULLS LAST.
- **zero-edge** (R-17): explicit empty state in all three formats.
- Additional: high inbound + zero outbound (degree observability), asymmetric vs symmetric
  direction glyph, near-miss `source` string, `Supersedes` excluded.

## Security Risks

`context_get` accepts an external `id` and `include_edges: Option<bool>`.
- **Untrusted input** — `id` is validated by the pre-existing `entry_store.get(id)`, which
  errors before edge logic runs; `include_edges` is a bounded `Option<bool>` (no injection
  surface).
- **SQL injection** — the ranked select, the split `COUNT(*)`, the confidence JOIN, and the
  batched title `IN(…)` MUST use **positional binds** (precedent `fetch_nodes_batch`), never
  string-interpolated ids or a string-built IN-list. The canonicalization `CASE`/dedup and the
  `LIMIT`/`ORDER BY` must be static SQL, not assembled from input. Assert parameterized binds.
- **Blast radius if compromised** — read-only path on `read_pool_server`; no writes, no DDL.
  Worst case is information disclosure of edge structure (the intended behavior). Dangling-target
  handling must not leak existence of quarantined/filtered targets beyond the `target_id`
  already present in `graph_edges`; if the confidence JOIN must respect a status filter on the
  target (#4166 endpoint-guard precedent), confirm it does not surface a suppressed target's
  title.
- **Resource exhaustion** — the rank-and-limit-in-SQL design (R-04) is itself the mitigation:
  a hub node returns ≤3 rows + two scalar counts, never its full fan-out. The title `IN(…)` is
  bounded to the displayed ≤3 targets — naturally small (no chunking needed at depth-1 single-
  entry, unlike crt-049 swarm sets). Confirm the count query is a bounded aggregate, not a
  materialized scan.

## Failure Modes

- **Edge-query / count / title failure** (R-16): per OQ-A the architecture leans **fail the get**
  with a mapped `ServerError` (same pattern as the primary read), no `.unwrap()`; empirically
  exercised red (#4876). OQ-A resolved before implementation.
- **Dangling target** (R-06, R-15): NOT a failure — `target_title:null`, edge retained, ranks
  NULLS LAST.
- **Cold-start / uniform confidence** (R-06): inferred ranking degenerates gracefully to the
  deterministic `target_id` tiebreak, not arbitrary order.
- **Opt-out** (R-14): graceful no-op — zero edge queries, no `edges` key.
- **Hub node** (R-04): graceful bound — 3 rows + two counts in SQL, full set never materialized.
- **>3 edges** (R-03): graceful truncation of the displayed set with exact uncapped totals and
  the `…use context_graph` pointer.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (shared serializer byte-identity) | R-07 | ADR-003 `None ⇒ key absent` structural (`entry_to_json` signature fixed). AC-07 byte-identity golden across 4 tools × 3 formats via the **real producer** (#1268). |
| SR-02 (additive `source` breaks `context_graph` neighbors) | R-08 | ADR-004 additive `RawEdgeRow.source`; rank/JOIN/`↔` confined to the new variant. Existing neighbors suite passes **unedited** (empirical, #4876, AC-09). No DDL. |
| SR-05 (`authored` mislabels future non-statistical source) | R-20 | C-10 documents the NLI-dark precondition at the code site; `source` string retained. **Accepted/deferred** — revival is the documented trigger to revisit D-03. |
| SR-06 (edge vocabulary divergence from `EdgeRecord`) | R-10, R-08 | ADR-002 documents the get shape as an explicit projection; FR-15 fidelity; `↔` is get-only and must not leak into neighbors (R-08 surface 3). |
| SR-07 (dangling / corrected-transient misread) | R-15, R-19 | DNB-1 (dangling) + DNB-2 (corrected transient) encoded as explicit expected-behavior tests. |
| SR-08 (symmetric double-count/double-render — **blocker**) | R-01, R-03 | FR-8/C-6: canonicalize in SQL **before** rank AND count; ADR-007. Tested **independently on display (R-01) AND totals (R-03)**; order-of-operations asserted. |
| SR-09 (ranking ORDER BY silently degrades) | R-02, R-11 | FR-9/C-8: exact `ORDER BY (source='agent') DESC, t.confidence DESC LIMIT 3` (ADR-006). Discriminating test with the proof value **outside the cap** (#3886) + per-edge trace (#3645); weight does NOT decide. |
| SR-10 (carried-forward/`context_edge` mis-classified) | R-05 | FR-17: assert `source='agent'` on both write paths; named carried-forward + `context_edge` authored-priority tests (silent-degrade, no build signal). |
| SR-11 (confidence JOIN skews on bad endpoints) | R-06 | FR-9: **LEFT JOIN** with explicit `NULLS LAST`; dangling retained (D-02), NULL/cold-start ranks deterministically (tiebreak `target_id ASC`). |
| SR-12 (AC-12 latency budget unbacked) | R-13 | NFR-2/C-9: measured edge-free baseline incl. high-degree node before numbers lock; read-pool + indexed JOIN; OQ-03 internal-caller opt-out relieves load (R-14). |
| SR-13 (cap-3 raises cost of every defect) | R-01, R-02, R-05, R-11 | NFR-7: output is an acceptance surface; canonicalization/ranking/classification tests elevated to **discriminating** (not smoke); "which 3" is the acceptance surface. |
| SR-14 (rank-and-limit must bound fan-out, not pull 1000 rows) | R-04 | FR-11/C-7: `LIMIT 3` + `COUNT(*)` in SQL; high-degree node returns 3 rows + two counts; full set never materialized — proven at the **store boundary**, not just rendered output. |
| SR-03 (hot-path N+1 / opt-out skip)¹ | R-14, R-13 | NFR-1: one batched title `IN(…)` (no N+1); opt-out skips all queries (R-14); read-pool per NFR-3. |
| SR-04 (default-on UX/contract shift)¹ | R-17 | NFR-7 makes output an acceptance surface; reframed D-08/ADR-005 strings snapshot-tested (sub-split dropped, `↔` glyph). |

¹ SR-03/SR-04 are carried-forward IDs from the pre-reframe assessment (the revised assessment
renumbered the dominant risks SR-08…SR-14); both remain covered above. **All scope risks
SR-01…SR-14 are traced.** SR-08/SR-09/SR-10/SR-12/SR-14 (the reframe's new failure modes)
receive Critical/High discriminating coverage per the spawn emphasis.

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-04) | ~14 (canonicalization display+totals+order ×3 types; ranking authored-priority/inferred-fill/confidence-outside-cap/tiebreak; split-count exact/parity/direction; SQL-LIMIT-not-memory at store boundary) |
| High | 7 (R-05, R-06, R-07, R-08, R-09, R-13, R-14) | ~18 (carried-forward+context_edge authored; LEFT-JOIN dangling/NULL/cold-start; byte-identity golden ×4×3; neighbors-suite empirical + map_edge_row branches; authored exact-match; latency baseline+delta+read-pool; opt-out skip + internal callers) |
| Medium | 5 (R-10, R-11, R-12, R-15, R-16) | ~12 (direction/`↔` projection; rank trace discipline; Supersedes excluded display+totals; dangling retained; failure red-run + no-unwrap) |
| Low | 3 (R-17, R-18, R-19) | ~7 (format snapshots ×3 with `↔`/dropped-sub-split; file-size; corrected transient) |

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` for SQL-LIMIT/symmetric-double-count/canonicalization
  and ranking-ORDER-BY lessons. Found **#3886** (crt-034 — an `ORDER BY...LIMIT` ranking test is
  non-discriminating unless the proof value falls **outside the cap**; directly governs R-02
  ranking-by-target-confidence), **#3645** (col-030 — ranking expected values cannot be intuited,
  trace the algorithm; governs R-11), **#3621** (col-029 — JOIN-heavy SQL traced before Gate 3a),
  **#1044** (crt-018 — risk strategy caught a COUNT bug; R-03 is the same shape), **#4166/#4162**
  (graph_edges endpoint JOINs guard BOTH endpoints / ALL passes; R-06/R-08), **#4247** (SQLite
  COUNT-DISTINCT separator collision). All applied to the Critical/High risks.
- Stored: **nothing novel to store** — the cross-feature pattern this reframe most exercises
  ("an `ORDER BY ... LIMIT` ranking test must seed the discriminating value outside the cap")
  already exists as **#3886** and applies verbatim. The symmetric-canonicalization-before-rank-
  and-count risk is feature-specific (SR-08/D-10), not yet a 2+-feature pattern. Will revisit at
  retro if vnc-037 establishes a recurring "canonicalize-before-cap-and-count" pattern distinct
  from #3886.
