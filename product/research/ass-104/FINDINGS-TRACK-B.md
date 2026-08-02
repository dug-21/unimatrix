# FINDINGS: ass-104 TRACK B — Graph acceleration: event-driven vs tick-rebuild

**Spike**: ass-104 (Track B of 4) | **Date**: 2026-07-20 | **Approach**: evaluation
**Breadth**: code + Unimatrix internal | **Confidence**: **DIRECTIONAL** — envisioning only, no build, no PoC

## Executive summary

**Yes — but three code facts reframe the question.**

1. **The staleness contract is misstated by ~15×.** Every tool description discloses "rebuilt each tick (typically 30-60 seconds)" (`mcp/tools.rs:86, :98, :124, :4248, :4260, :4286`; `graph_read_subgraph.rs:65`; `graph_read_path.rs:65`), and ADR-004 vnc-019 (#4493) rests its "knowing the age doesn't change behavior" reasoning on that figure. The shipped default is `DEFAULT_TICK_INTERVAL_SECS: u64 = 900` (`background.rs:83`) — **15 minutes**. #904's premise inherits the same wrong number. The largest currency defect is a config/disclosure problem, not an architecture problem.

2. **The read side already pays O(N+E) per query.** `search.rs:733-744` deep-clones `typed_graph` **and** `all_entries` on every search — including when `use_fallback == true`, where both clones are pure waste (fallback branches at `:777, :822, :928, :1375` never touch them). `graph_read_subgraph.rs:189-192`, `graph_read_neighbors.rs:279-282`, `graph_read_path.rs:162-165` each clone the full graph. `TypedRelationGraph` derives `Clone` *specifically* for this (`graph.rs:292-293`). The hot path is "copy the whole graph per request," not "read a prebuilt cache under a short lock."

3. **Incremental is structurally feasible, and the global invariant is narrower than it looks.** `TypedRelationGraph` is `StableGraph<u64, RelationEdge>` + `node_index: HashMap<u64, NodeIndex>` (`graph.rs:295-301`) — `StableGraph` was chosen per ADR-001 precisely so indices survive removal. The builder (`graph.rs:381-513`) is a pure fold with exactly one whole-graph check: `is_cyclic_directed` over a **Supersedes-only** temp subgraph (`:487-511`). Supersedes edges derive from `entries.supersedes` (`:396-422`), not `graph_edges` — so they change only on `context_correct`/deprecate. High-volume typed-edge writes provably cannot violate the one global invariant.

**Direction**: do **not** jump to incremental patching. Take the win in graded steps; hold #904 mechanism 4 behind a measured trigger.

## Ground truth — cited

| Element | Location | Fact |
|---|---|---|
| Rebuild body | `services/typed_graph.rs:91-148` | `query_all_entries()` + `query_graph_edges()` — full tables, no cap, no watermark, no delta |
| Quarantine filter | `typed_graph.rs:107-110` | Quarantined excluded (GH #444); Deprecated deliberately **retained** for Supersedes chains (SR-01) |
| Builder | `engine/graph.rs:381-513` | Pass 1 nodes → 2a Supersedes from `entries.supersedes` → 2b typed edges from `graph_edges` → 3 cycle check |
| Cycle scope | `graph.rs:487-511` | **Supersedes-only** temp subgraph (full graph false-positives on bidirectional CoAccess) |
| Swap | `background.rs:968-1006` | `tokio::spawn` + 120s `TICK_TIMEOUT`, `*guard = new_state`; retain-on-error/panic/timeout |
| Cadence | `background/job.rs:265-278` | Job 4 of 9, `EveryTick`; interval 900s (`background.rs:83`, `:107-127`) |
| Accepted contract | `background.rs:766` | Comment states new edges reach PPR at the **next** tick — one-tick delay accepted by design |
| Mutation API | `graph.rs:392, 418, 484` | Only mutations of `inner` are inside the builder. No `&mut` method; `inner`/`node_index` are `pub(crate)` — server cannot mutate. **Incremental API is greenfield.** |
| Generation precedent | `services/effectiveness.rs:61, :118-137` | `EffectivenessState.generation: u64` + reader snapshot cache re-cloning only on mismatch — the exact shape a graph generation counter would take |
| Live-read hatches shipped | ADR-005 vnc-018 (#4479), ADR-001 vnc-043 (#5448) | `neighbors`/`subgraph` depth==1 → live SQL; `use_fallback` → live SQL on all three modes |

**Genuinely cache-only consumers** (what a currency fix buys): PPR / `graph_penalty_with` / `find_terminal_active` / `graph_expand` / `suppress_contradicts` on the search ranking path (`search.rs:786, 827, 960, 1074, 1382`), and `subgraph`/`neighbors`/`path` at **depth > 1**. Depth-1 is already live.

Per Principle 7 (`PRODUCT-VISION.md:72`) and ass-103, op 4 is the *sole* writer of the handle, and ass-103 I-9 established a wedged rebuild is indistinguishable from a healthy one. **Currency and observability-of-currency are the same problem.**

## Ranked options

Ranked by **recommended sequence**, not sophistication. O-A is a prerequisite, not a rival.

### O-A. `Arc`-swap the handle (enabler — do regardless)
`Arc<RwLock<TypedGraphState>>` → `ArcSwap`/`RwLock<Arc<_>>`; readers take an `Arc` clone.
- **Consistency**: unchanged shape; *strictly stronger* (immutable point-in-time snapshot).
- **Cost**: removes O(N+E) deep clone from every search + every depth>1 graph read. Net win.
- **Complexity**: low, mechanical — ~4 read sites, ~4 write sites, compiler-enforced.
- **Blast radius**: search + 3 graph modes; test fixtures need updating (cf. #5456 — routing a cached path to live SQL broke in-memory fixtures; same fixture class).
- **Principle 7**: held, and *better honored* — the "short read lock" the crt-021 comments claim (`typed_graph.rs:30-32`) finally becomes true.

### O-B. Correct the interval + disclosure (the actual defect)
- **Cost/complexity**: disclosure half is documentation-only, **zero runtime risk**; should not wait on anything. Interval half → take as O-C (lowering the *global* default is #904 mechanism 1, "blunt" — it drags op 1's ONNX/heal and op 6's O(N) scan along: ass-103 I-15, I-16, I-20).
- **Note**: ADR-004 vnc-019 declined `graph_rebuilt_at` partly *because* "tick interval is already documented (30-60s)." That premise is false. Its reasoning — not necessarily its conclusion — needs revisiting alongside I-9.

### O-C. Decoupled graph cadence (#904 mech. 2)
- **Consistency**: unchanged — same code, more often.
- **Cost**: O(N+E) per graph interval; **grows with E**, and no edge demoter exists anywhere (ass-103 I-4, I-18). Cost is not stable over corpus lifetime — its real weakness.
- **Complexity**: low-mod. `job.rs:265-278` already models per-job cadence via a static predicate.
- **Blast radius**: background only; write path untouched. Needs missed-tick behavior reconsidered (I-20: tokio default `Burst` makes a faster loop *more* storm-prone).
- **Principle 7**: held verbatim — most principle-conservative real option.

### O-D. Dirty-flag → debounced early rebuild (#904 mech. 3) — **recommended mechanism**
Writes set an atomic dirty flag; a debouncer triggers an out-of-band **full** rebuild, with a minimum-interval floor.
- **Consistency**: eventual, bounded by debounce + rebuild duration. Sub-tick by construction. Whole-graph snapshot preserved, so **every invariant holds unchanged and unaudited** — the decisive advantage over O-E.
- **Cost**: O(N+E) per *burst*, not per interval. Under idle (common for a personal-cloud SDLC corpus) strictly **cheaper than today**. Debounce + floor are the entire cost-control surface.
- **Complexity**: moderate — dirty signal, debouncer with floor, coordination against a scheduled rebuild. All background-side except one atomic store.
- **Critical design constraint (from code)**: the flag needs **two trigger classes**, not one. A hook keyed only on `graph_edges` writes would miss every `context_correct` and every deprecation — precisely the mutations driving `find_terminal_active` and `graph_penalty`, because Supersedes lives in `entries.supersedes` and Pass 2b explicitly skips `graph_edges` Supersedes rows.
- **Principle 7**: held. "Rebuilt by tick" → "rebuilt by tick *or sooner on demand*"; hot path still never reads DB at query time. Reading the principle as forbidding an extra *trigger* would be over-reading it — it governs where reads come from, not what schedules the rebuild.

### O-E. Event-driven incremental patch (#904 mech. 4)
- **Consistency**: near-zero staleness — strongest currency. But the served graph becomes an incrementally-maintained structure that can **silently diverge** from the DB. The full rebuild detects divergence only if something compares the two; nothing does today.
- **Feasibility**: real. `StableGraph` + `node_index` is the right shape; the global invariant is engaged only by `context_correct`/deprecate. A bounded form — incremental for non-Supersedes edge inserts, full rebuild for anything touching `supersedes`/status — is coherent and covers most write volume.
- **Cost**: O(1)/edit; **the only option whose cost does not grow with E**.
- **Complexity: highest by a wide margin.** Concrete hazards found in code:
  - `node_index` is **one-directional**; `inner.remove_node` leaves a stale `u64 → NodeIndex` entry with no reverse map to find it. Needs an added reverse map or ID passed alongside.
  - `personalized_pagerank` iterates `graph.node_index.keys()` (`graph_ppr.rs:67`) — **a stale key resurrects a deleted node into the PPR result set.** Silent-wrong-answer, not staleness.
  - `all_entries` must be patched in lockstep; `graph_penalty_with`/`find_terminal_active` take both, and disagreement yields wrong answers.
  - Cycle detection is whole-graph today; needs an incremental equivalent or full-rebuild fallback on any Supersedes mutation.
- **Blast radius: largest.** Write path (`tools.rs:1109, 1140, 1439, 1465, 1481, 3982, 4044, 1723, 1737, 4986`) **and** hot path **and** the tick ops that bulk-mutate `graph_edges` (ass-103 ops 2, 3, 8, 9a/9c) — so "each edit updates its slice" must cover tick-authored edits, a far bigger surface than #904 implies.
- **Principle 7**: preserved in letter, **weakened in spirit** — the implicit guarantee (*a faithful projection reconstructed wholesale on a known cadence*) becomes conditional on the incremental path's correctness. An explicit ratified trade, not a side effect.

### O-F. Extend the live-read hatch (already the shipped pattern)
- **Consistency**: strong read-your-writes for routed reads. **Cost**: permanent per-read DB tax (ass-088: right interim cover, wrong sole home — "does not repair the stored graph").
- **Blast radius**: known-hazardous — vnc-043 SR-02 documented that promoting a rare branch to default makes latent bugs load-bearing; #5456 records the fixture breakage.
- **Principle 7**: **this is the option that actually strains it** — it moves reads back to the DB at query time.
- **Verdict**: **complete as shipped, not the growth path.** The consumers that matter most (PPR/`graph_penalty` on the ranking path) are whole-graph algorithms with no live-SQL analogue.

## Comparison

| | Staleness | Cost shape | Growth w/ corpus | Complexity | Blast radius | Principle 7 |
|---|---|---|---|---|---|---|
| Status quo | **900s** (disclosed 30-60s) | O(N+E)/tick + **O(N+E)/read** | grows w/ E | — | — | held |
| **O-A** Arc-swap | unchanged | removes per-read clone | improves | low | search + 3 modes | held, better honored |
| **O-B** disclosure | ↓ by factor | ×freq all 9 jobs | unchanged | trivial (docs) | none (docs) | held |
| **O-C** cadence | ↓ to graph interval | O(N+E)/interval | **grows w/ E** | low-mod | background only | held verbatim |
| **O-D** debounce | **sub-tick** | O(N+E)/burst; cheaper idle | grows w/ E, writes only | moderate | background + 1 atomic | held |
| **O-E** incremental | **~zero** | O(1)/edit + backstop | **flat** | **high** | **write + hot + 6 tick ops** | letter yes, spirit weakened |
| **O-F** live-read | zero (routed) | permanent read tax | w/ read volume | low/surface | fixture hazard | **strained** |

## Invariants each option must preserve

The real deliverable — the acceptance surface whichever option is chosen.

**Structural**
1. **Quarantined absent** (`typed_graph.rs:107-110`, #444) — must not propagate PPR mass. Under O-E a quarantine must *remove* a node, not mark it.
2. **Deprecated retained** (`typed_graph.rs:98-106`, SR-01) — removal breaks `find_terminal_active` for chains through a deprecated intermediate. Code carries an explicit "do not add a filter here" warning.
3. **`bootstrap_only=true` structurally excluded** (`graph.rs:428-430`, C-13).
4. **Supersedes derives from `entries.supersedes`, never `graph_edges`** (`graph.rs:396-435`). An incremental path keyed on `graph_edges` silently misses every supersession.
5. **Unrecognized `relation_type` skipped, not an error** (`graph.rs:438-446`).
6. **`typed_graph` and `all_entries` must agree** — both feed `graph_penalty_with`/`find_terminal_active`. Divergence = wrong answers, not stale ones.

**Validity**
7. **Supersedes-only subgraph stays acyclic**, and cycle detection stays Supersedes-scoped — the full graph legitimately holds bidirectional CoAccess pairs (`graph.rs:487-491`).
8. **Cycle sets `use_fallback=true` and does NOT swap** (`background.rs:986-993`) — must stay distinguishable from I/O failure.

**Availability**
9. **Retain-on-error** (`background.rs:994-1005`). No option may replace good state with degraded state — the exact failure ass-103 flagged as I-3/I-5; the graph currently gets this right.
10. **Cold-start safe**: `use_fallback` → live SQL on all three modes (`graph_read_subgraph.rs:196`, `neighbors.rs:288`, `path.rs:172`); search applies `FALLBACK_PENALTY`. Principle 5.
11. **Readers never block on rebuild, never hold a lock across async** (`search.rs:731`, `graph_read_path.rs:166-167`). Free under O-A; a proof obligation under O-D/O-E.
12. **No write-path latency regression** — #904's own DoD. O-D satisfies by construction; O-E must prove it.
13. **Disclosed contract matches shipped default** — currently violated ~15×. `tools.rs` keeps a twin-literal byte-equality guard (ADR-002 vnc-043, #5449), so both copies move together.
14. **Staleness must be observable** (ass-103 I-9). Sub-tick claims are unverifiable without a freshness signal — where ADR-004 vnc-019's declined `graph_rebuilt_at` should reopen as a `context_status` health field, per that ADR's own stated "Harder" consequence.

**Invariants 1-6 and 9-11 are free under O-A/B/C/D (rebuild code unchanged) and are all new proof obligations under O-E.** That asymmetry is the core of the recommendation.

## Recommendation DIRECTION

**Graded hybrid — "fix the window, then trigger the rebuild; do not patch the graph yet."** Mechanism explicitly **not** settled; uni-zero + human rule.

1. **Correct the disclosed contract** to match `DEFAULT_TICK_INTERVAL_SECS`. Zero runtime risk; stops #904 and ADR-004 reasoning off a 15×-wrong number. Unblocked, do first.
2. **`Arc`-swap the handle** (O-A). Removes O(N+E) clone from every search; every faster option depends on cheap high-frequency swaps. **Worth doing on perf merits alone.**
3. **Dirty-flag → debounced rebuild** (O-D) on a graph-owned cadence (O-C's decoupling) with a minimum-interval floor, and **two trigger classes** (edge-table writes *and* `supersedes`/`status` writes). Delivers sub-tick currency with the rebuild body — and invariants 1-11 — untouched. Satisfies #904's DoD without touching the hot path or write path beyond an atomic store.
4. **Add a freshness/health signal** (rebuild age, last-success, consecutive failures) on `context_status`. Without it no currency SLO is verifiable and I-9 persists regardless of mechanism.

**Hold O-E behind an explicit trigger.** Feasible, and the only option whose cost doesn't grow with E — but the only one converting eleven free invariants into proof obligations, and the only one whose failure mode is *silently wrong* rather than *merely stale*. Revisit when measurement shows full-rebuild wall-clock approaching the debounce floor. **That measurement does not exist** (ass-103's costs are shapes from code; it named this same gap).

**Rationale**: the gap is 15 minutes, not 60 seconds; steps 1-3 close nearly all of it with rebuild logic untouched, and step 2 is required for step 3 or for O-E either way — so the graded path dominates on every axis except steady-state cost at a scale no one has measured.

**Consistency with ass-088/#870**: no contradiction. ass-088 concerned *stored-edge* convergence under correction; the human chose the simpler synchronous split for personal-cloud scale and deferred the amortized tick. Track B concerns *in-memory currency*, a different layer. Both point the same way: **prefer the mechanism matching realized scale; defer the enterprise mechanism behind a measurement trigger.**

## Unanswered Questions

- **Debounce + floor values for O-D** — needs full-rebuild wall-clock at production corpus size and realized write rate. *Neither exists; needs an `empirical` spike — the same one ass-103 flagged first.*
- **Actual O(N+E) rebuild cost vs the 120s `TICK_TIMEOUT`** — the trigger condition for escalating to O-E. *Not measurable at `directional`; SCOPE bars build.*
- **Real cost of the per-read graph clone at p50/p99 search latency** — O-A's win is argued structurally, not measured. *Needs profiling; nan-018 harness may fit.*
- **Does faster graph convergence change PPR output materially?** ass-088 flagged the identical question as "directionally safe, unmeasured." *Needs a retrieval-quality eval.*
- **Should ADR-004 vnc-019 be amended or superseded?** Its reasoning is partly invalidated; whether the *conclusion* holds is an ADR judgment. *SCOPE bars ADR amendment — flagged for uni-zero.*
- **Under O-E, how do tick-authored bulk edge writes (ops 2, 3, 8, 9a/9c) participate?** "Event-driven" cannot mean agent-writes-only. *SCOPE bars mechanism design.*

## Out-of-Scope Discoveries

- **[Track C / gap] The disclosed staleness contract is wrong by ~15× in 8 locations.** Arguably a filable defect independent of currency work. Flagging, not filing — outward commitments are the human's call.
- **[Track C / perf] The per-request full graph clone** (`search.rs:739-740` + 3 graph modes) is worth fixing even if cadence never changes. Note it is also **wasted entirely when `use_fallback == true`** — a free early-return win.
- **[Track C / perf] `find_terminal_active` linearly scans `all_entries` per visited node** (`graph.rs:664` via `entry_by_id`) — O(depth × |all_entries|) per call, invoked once per superseded candidate in the search loop. Unrelated to currency; a standalone hot-path inefficiency.
- **[Track C] No `rebuilt_at`, generation, or failure counter on `TypedGraphState`.** ADR-004 vnc-019 declined it and named the consequence: "future freshness monitoring requires a separate cross-cutting feature (e.g. `graph_cache_age_ms` on `context_status`)." That is exactly Track C's monitoring gap. **`EffectivenessState.generation` (`effectiveness.rs:61, :118-137`) is the in-repo precedent to copy** — and would additionally let the search path skip both large clones when generation is unchanged.
- **[Track C] `all_entries` is a second, parallel cache living inside the graph handle.** It is an entry cache, not a graph. Whether it belongs there is a taxonomy question for the synthesis.
- **[Track A] #904's four mechanisms map cleanly onto O-B/O-C/O-D/O-E here**, and its "Open question (design gate)" says a spike should survey them — Track B *is* that survey. Record #904 as **LATENT** (open, mechanism undecided, now surveyed).
- **[Cross-cutting, for synthesis] Edge monotonic growth (ass-103 I-4) is the hidden variable under every option except O-E.** O-C and O-D cost O(N+E) per rebuild, so no edge demoter means their cost grows unbounded over corpus lifetime. **Fixing I-4 makes O-D durable and pushes the O-E trigger out; not fixing it eventually forces O-E.** These decisions are coupled — rule on them together.

## Recommendations Summary

- **Can graph updates move to event-driven/incremental?** **Yes structurally** — `StableGraph` supports it and the only global invariant is engaged by a narrow, low-rate write class. **But it should not be the next move.**
- **Direction**: (1) fix the 15× disclosure error, (2) `Arc`-swap to remove the per-read O(N+E) clone, (3) dirty-flag → debounced rebuild on graph-owned cadence with two trigger classes, (4) add a graph-freshness health signal. Hold incremental patching behind a measured trigger.
- **Ranked**: O-A (prerequisite, net win) → O-B (trivial, urgent) → **O-D (recommended)** with O-C → O-E (deferred, feasible, highest blast radius) → O-F (complete as shipped, not the growth path).
- **Principle 7**: held by O-A/B/C/D. O-E holds the letter but weakens the "faithful wholesale projection" guarantee — an explicit trade requiring ratification. **O-F is the option that actually strains the principle.**
- **Invariants**: 14 named. Nos. 1-6 and 9-11 are free under every option except O-E, where all become proof obligations — the primary argument for the graded path.
- **Coupling for synthesis**: unbounded edge growth (I-4) determines how long O-D stays viable before O-E is forced.
- **Blocking measurement**: full-rebuild wall-clock at production corpus size — gates the debounce floor, the O-E trigger, and any currency SLO. Recommend an `empirical` follow-on spike.

## Confidence statement

**Directional.** Structural claims read from code and cited to `file:line`; ADR claims cited to Unimatrix ids (#4493, #5448, #4479, #5449, #5456). No PoC, no measurement, no test run. No Unimatrix writes were made.

- **High confidence**: the 900s-vs-30-60s discrepancy; the per-request full-graph clone at all four sites; `StableGraph`/`node_index` shape and absence of any mutation API; the builder's pass structure; Supersedes-only cycle scope; Supersedes deriving from `entries.supersedes` with `graph_edges` Supersedes rows skipped; retain-on-error; the depth-1 live-read asymmetry; PPR iterating `node_index.keys()`.
- **Medium confidence**: relative cost rankings; that O-D's debounce can meet #904's DoD; that O-A is net-negative cost; the O-E feasibility boundary for non-Supersedes writes.
- **Not established**: any wall-clock figure, corpus-size threshold, or PPR-quality consequence. **No cost number here is a measurement.**