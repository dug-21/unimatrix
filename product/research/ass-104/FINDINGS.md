# FINDINGS: ass-104 — Background-processing TARGET STATE (campaign synthesis)

**Spike**: ass-104 (SYNTHESIS of Tracks A/B/C/D)
**Date**: 2026-07-20
**Approach**: synthesis (envisioning) — no re-investigation
**Confidence**: **DIRECTIONAL** — target picture, options, and a recommendation *direction*. Mechanism choices are **not** settled here; uni-zero + human ratify.

**Inputs**: `FINDINGS-TRACK-A.md` (option ledger), `FINDINGS-TRACK-B.md` (graph currency), `FINDINGS-TRACK-C.md` (integrity/perf target), `FINDINGS-TRACK-D.md` (external bar). Baseline: `ass-103/FINDINGS.md` (24-op tick inventory).

---

## Headline

Three findings dominate everything below, and each is a *reframe* rather than a new gap.

1. **The target state is mostly not new construction — it is activation, exposure, and observation.** Track A's ledger and Track C's inventory converge: the largest single class in this campaign is **built-but-unwired** (`Preset` lifecycle policy, `current_phase`, the `Contradicts` write, `s2_vocabulary`, `RelatedTo`, `resolve_supersessions`, `ResourceClass`, the entire GNN staging set). The second-largest is **computed-then-discarded** (orphaned-edge delete counts, index-drift counts, contradiction scores, effectiveness aggregates, confidence-refresh saturation). Very little of the target state requires greenfield.

2. **The engine has no self-observability, and that is the keystone gap.** Track C: no metrics surface, no per-pass log on the production path, no tick ledger, `next_maintenance_scheduled` advancing on a dead tick. Track D independently ranks the two lowest-cost, highest-value external patterns as *exactly this* (F2-3 constraint-violation report, F5-1 corpus-health scorecard). Track B: no currency claim is verifiable without a freshness signal (I-9). Track A O-3: scale-gated deferrals are never re-checked because nothing watches the gate. **Every other recommendation in this document is unmeasurable until this is fixed** — including whether it worked.

3. **The external bar is a four-tier architecture; Unimatrix has tier 4 only.** Track D: provenance-bearing write log → delta-driven incremental maintenance → **asynchronous improvement pass** → periodic full reconciliation. The tick is a *reconciliation* mechanism (it makes the hot path match the store). It is not an *improvement* mechanism (making the store better than it was). Conflating them is why expensive-but-valuable work (contradiction, synthesis, dedup, consolidation) has repeatedly been deferred or deleted for competing with cheap-and-mandatory work in a single 120s-per-op budget. **A corpus that is only reconciled does not learn.**

---

## 1. Function taxonomy — the 24 ops collapsed

ass-103's inventory is an *implementation* list. Collapsed by **purpose**, it yields **seven** functions — the SCOPE's five, plus two the inventory forces into existence.

| Fn | Purpose | ass-103 ops | Class |
|---|---|---|---|
| **F1. Serving-cache reconstruction** (currency) | Rebuild the Principle-7 in-memory hot path so it matches the store | 1e, 1h, **4**, **5**, **6** | *Forced addition* — the SCOPE folded this into "graph-healing"; they are different jobs with different failure modes. Healing changes the store; currency changes only the projection. |
| **F2. Graph healing / structural integrity** | Keep the *stored* graph consistent under correction, deprecation, quarantine | **2** (repoint leg + DELETE leg), 1c | Universal invariant |
| **F3. Learning-signal maintenance** | Produce and refresh the signals ranking consumes | 1b, 1g, 1h, **3**, **8**, **9a/9b/9c**, 5 | Structurally universal, numerically domain-tunable |
| **F4. Relevance / lifecycle maintenance** | Promote, demote, and transition entries by learning utility | **1l**, 1m, 1n, *(the absent purge)* | Structurally universal, policy domain-tunable |
| **F5. Storage governance** | Bound storage by learning utility | 1f, **1j**, 1k | Universal principle, tunable aggressiveness |
| **F6. Index governance** | Keep store ⟷ vector_map ⟷ HNSW consistent and performant | 1c, **1d**, **1i** | Universal invariant |
| **F7. Knowledge acquisition** | Passive extraction of new entries from observations | **7** | *Forced addition* — not maintenance at all; it is the only op that **creates** knowledge, and it is the only one whose integrity gates fail **open and silent** (I-23). |

**What the taxonomy exposes that op-by-op reading did not:**

- **F1 has no owner of its own liveness.** Five caches, all full-swap, all sole-supplied by the tick, and only one (1e) models retain-on-error correctly. I-3, I-5, I-9 are one defect in three places.
- **F2's two legs point in opposite directions.** ass-103's capability map is blunt about it: the repoint leg is a **positive** SLN3 contribution; the DELETE leg is a **negative** one. A single op both heals and destroys.
- **F3 is all promotion and no demotion.** Four promotion paths (3, 8, 9a, 9c); zero count- or score-aware deleters (Track A O-6, ass-103 I-4). This is not an oversight in one op — it is a **missing member of the taxonomy**.
- **F4 is a stub.** Its only live behavior is auto-quarantine (1l), which carries three defects; 1m is dead work; 1n is a finished migration. The lifecycle function the SCOPE asks about **barely exists**.
- **F7 does not belong in a maintenance tick** and its presence there is why its failure modes are silent — the tick has no place to report an admission it shouldn't have made.

**Missing functions (present in no op):**

| Missing | Evidence |
|---|---|
| **F8. Engine self-observation** | Track C §4.3 — no metrics, no ledger, no duration, dishonest health fields |
| **F9. Corpus quality assessment** | Track D F5-1/F5-2 — no scorecard, no golden set, no constraint report |
| **F10. Asynchronous improvement** | Track D F4-1 — no pass exists whose job is making the corpus *better* |
| **F11. Structural removal** | Track A O-7, Track C §4.1 — no purge in any production path |
| **F12. Index reconciliation** | Track C §4.2 — no drift detection in either direction |
| **F13. Edge demotion** | Track A O-6, ass-103 I-4 |

---

## 2. Universal-invariant vs domain-tunable — and the latent-param inventory

### 2.1 The classification

**Universal invariants** (always on, identical in every domain, never operator-configurable):

| Invariant | Status today |
|---|---|
| Graph integrity under correction — repoint **before** compact | Partial (repoint leg correct; DELETE leg violates it) |
| Hash-chain + append-only audit | Held |
| Current-version resolution (supersession → active terminal) | Held |
| **Retain-on-error for every Principle-7 cache** | **Violated** (I-3, I-5); no stated convention |
| Index ⟷ store consistency (no orphan vectors, no unembedded actives) | Partial; unobserved in both directions |
| Per-slug isolation | Held |
| Storage bounded by **learning utility**, never wall-clock age | Held in retention; **violated** by `cleanup_stale_co_access` (365d wall-clock on a learning signal) |
| Lambda as a **structural** metric (no activity/freshness dimensions) | Held (crt-048) |
| **The tick completes, and says so** | **Missing entirely** |
| **Reversible states stay reversible** | **Violated** — quarantine is reversible for status/vector, irreversible for edges (#889/#890) |

**Domain-tunable policy** (belongs on the L1 preset/pack surface):

confidence freshness half-life ✅ *already done* · confidence weight vector ✅ · quarantine sensitivity (env-only) · contradiction strictness and scan cadence (const) · promotion thresholds S1 `≥3` / S2 `≥2` / co-access `≥3` / NLI floors (**SQL string literals**) · edge weights (0.25, ×0.1) · retention aggressiveness · compaction trigger (const) · tick cadence (env-only) · purge policy (**does not exist**) · HNSW parameters (**no config path at all**).

### 2.2 The doctrinal line — write it down

Three tracks independently arrived at the same rule; it has been rediscovered at least four times and never recorded:

- **Age MAY inform confidence** — domain-tunable via half-life. *Already built.*
- **Age MUST NOT drive health** — crt-048 deleted the freshness dimension because it made a structurally improving corpus look degrading.
- **Age MUST NOT drive retention** — RETAIN (#5581): pruning is governed by learning-cycle utility.
- **[net-new, from Track D F3-1] Where decay is applied, decay on *last access*, never on creation.** This is the sharpest external contribution to the SCOPE's own framing: last-access decay makes entry #597 surfacing months later *fresh by construction*, with no domain exemption required. Track C found `last_accessed_at` and `access_count` are already written and **read by nothing except relevance scoring** — the substrate exists.

### 2.3 The domain-tunable surface: **`Preset`, not the domain pack**

Track C's decisive finding. `DomainPack` has four fields and **no numeric or threshold field anywhere in its schema** — and the tick never consults the registry at all (it constructs one and throws it away, twice per pass). `Preset` (`Collaborative`/`Authoritative`/`Operational`/`Empirical`/`Custom`) is already shipped, boot-validated, per-slug overlayable (vnc-040), and already carries per-domain half-lives 8760/720/24.

**Direction: extend `Preset` from a confidence-weight vector into a full lifecycle-policy preset.** Smallest change satisfying the Framing lens; reuses proven machinery. *Track D adds a refinement (F3-2): the strongest form is two-level — pack sets the default, a **per-entry** time-sensitive/timeless attribute overrides — because an SDLC corpus provably contains both timeless entries (architectural rationale) and genuinely time-sensitive ones (a version pin, a workaround for a fixed bug). A per-domain switch must pick one and be wrong about the other half.*

### 2.4 Latent-param inventory — **activate/expose** vs **build new**

**(a) ACTIVATE — built, tested, reaching production, switched off by one field**

| Item | Why it is off | Effort shape |
|---|---|---|
| `current_phase` scoring | Every production caller passes `None`; weight is a live 0.05; value already in scope at `tools.rs:743` | one line + a measured rollout (SL-METRIC caveat) |
| `Contradicts` edge write | Computed then discarded at `nli_detection_tick.rs:718`; `nli_enabled=false` | one branch inside the existing gate |
| `s2_vocabulary` | Defaults empty → op 9b is a no-op out of the box | config population + a cost guard (O(N²·V)) |
| `resolve_supersessions` on `context_graph` | Default-OFF; ass-088 recommended flipping | flag flip |
| `RelatedTo` edge type | In the PPR positive set, **0 edges written** | a write path; "cheapest diversity lever" |

**(b) EXPOSE — hardcoded, should be preset-tunable**

Promotion/density (highest leverage): S1 `≥3`, S1 weight ×0.1, S2 `≥2`, S2 weight ×0.1, S8 weight 0.25, co-access `≥3`, co-access noise floor 0.1, `MAX_INFORMS_PER_TICK=25`, `MAX_COSINE_SUPPORTS_PER_TICK=50` (carries its own `TODO: Config-promote`).
Lifecycle/health: co-access 365d window, `graph_stale_ratio` 0.10 trigger, confidence staleness 24h, refresh cap 500, Lambda threshold 0.8 and dimension weights, contradiction cadence `EveryN(4)` and its heuristic constants, `TICK_TIMEOUT` 120s, effectiveness constants, cold-start α/β (hardcoded **even for `Custom`**).
Index: **all** HNSW parameters — dimension, M, ef_construction, max_elements, max_layer, ef_search (duplicated in three places).
Env-only → belongs in config: tick interval, `auto_quarantine_cycles`.

**(c) RETIRE — inert config that controls nothing (and can fail startup)**

`retention.audit_log_retention_days` (validated knob → `gc_audit_log` warns and returns `Ok(0)`, then the tick logs "rows_deleted=0" forever: dead work + false operator signal + unhonored compliance promise) · `nli_entailment_threshold` · `nli_contradiction_threshold` · `max_contradicts_per_tick` · `nli_auto_quarantine_threshold` (**zero readers, yet `validate()` enforces a cross-field invariant between two dead fields**) · the entire `[graph_penalty]` section (parsed, merged, validated; production hardcodes `::default()`).

**(d) BUILD NEW — no substrate exists**

Purge lifecycle and `deprecated_at`/status-transition timestamp · index reconciliation (three-way, both directions) · edge demotion · tick liveness/duration/failure fields · a metrics surface · corpus-health scorecard · constraint-violation report · golden-set regression harness · bi-temporal validity · a separately-budgeted improvement pass.

---

## 3. Disposition per function — keep / rework / activate / retire

### 3.1 By taxonomy function

| Fn | Disposition | Reason |
|---|---|---|
| **F1 Currency** | **REWORK** (mechanism → §4) | Correct purpose; wrong cadence (900s, disclosed as 30–60s), wrong cost shape (O(N+E) per tick **and** per read), and no liveness signal |
| **F2 Graph healing** | **REWORK — urgent** | The DELETE leg makes a reversible action irreversible and fires in the **same pass** as auto-quarantine. Repoint leg is correct and should become the model |
| **F3 Learning signal** | **REWORK + ACTIVATE** | Thresholds must become policy; demotion must gain an owner; two consumers (phase, contradiction) must be connected to the work already being paid for |
| **F4 Lifecycle** | **REWORK + BUILD** | Auto-quarantine carries three defects incl. a silent status *promotion* on restore; the purge half does not exist |
| **F5 Storage governance** | **KEEP + one REWORK + one RETIRE** | 1j is the RETAIN proof and the model (cycle-based). 1f applies wall-clock to a learning signal. `gc_audit_log` is a no-op behind a validated knob |
| **F6 Index governance** | **REWORK + BUILD** | Heal exists but is `status = 0`-only; compaction is the only evictor and is uncapped; **no drift detection in either direction** |
| **F7 Acquisition** | **REWORK** | Integrity gates fail **open with no log at any level**. Fail-open on an integrity gate must be loud, counted, and surfaced |
| **F8–F13 (missing)** | **BUILD** | §5, §6 |

### 3.2 Op-level dispositions (ass-103 numbering)

**KEEP**: 1c (+ surface the count it discards), 1e (**the retain-on-error model**), 1j (**the RETAIN proof**), 9b (as documented domain opt-in), 9c (+ fix I-8; the only op with a configurable cadence — the model the others should follow).

**REWORK**: 1a (five full active-set scans per tick → one), 1b (split classification from report aggregates), 1d (extend past `status=0`), 1f (→ domain-tunable, prefer cycle-anchored), 1g (expose the budget, surface saturation), **1h — urgent** (retain-on-error; it writes degenerate state on a transient DB error), 1i (batch cap + incremental path; cannot be retired, it is the only HNSW evictor), **1l** (hysteresis, retry bound, and fix the `pre_quarantine_status` bypass), **2 — urgent** (#889/#890), 3 (demotion owner + source filter), 4 (→ §4), 6 (full-corpus re-embed every 4 ticks for one Lambda scalar; cadence at minimum tunable), 7 (loud fail-open), 9a (→ tunable).

**ACTIVATE**: **5** (phase table — rebuilt every tick, forever, into a consumer that passes `None`), **8** (the discarded `Contradicts` write, behind `nli_enabled`).

**RETIRE**: 1k's `gc_audit_log` call, 1m (`TODO(#409)` stub — pure dead work; its intended purpose becomes the purge lifecycle, not a resurrection here), 1n (post-verification), the four dead NLI config fields, `[graph_penalty]`.

### 3.3 The three named questions

**GNN — RETIRE the commitment, KEEP the hooks, FIX the pointer.**
All four tracks agree, and none of them from first principles. Track A: designed twice (ass-029 scope → ass-031 full design), built zero times, de-scoped by ass-032 on an exposure-bias argument (#3429 — training labels come only from what the current formula already surfaced); ass-038 rated training data "NEAR-PASS" (3/4). Track C: SL5 (#5564) and SL-ROLLUP (#5566) already record it verbatim as *"a research candidate, NOT a committed mechanism"*, gated behind SL-METRIC (#5572) — which is itself `partial` and **not trusted for live-corpus interpretation**. *A GNN cannot be scheduled ahead of its own yardstick.* Track D: **no external signal either way** — the 2025–26 production stack is LLM-adjudication + graph algorithms + vector search; nothing surveyed runs GNNs in background maintenance.
→ Retire any roadmap commitment. **Keep** the staging hooks (`graph_edges.metadata`, `phase_category_weights()`, `FusedScoreInputs`, `Prerequisite`, the learned-weights priority-0 hook, `ResourceClass`): small, inert, honestly documented, and collectively the *named stable feature interface* any learned-ranking method would need. Retiring them buys nothing and would have to be rebuilt. **Fix the stale pointer**: `WAVE2-ROADMAP.md:228` tracks ass-029 as "not started"; ass-031 executed it and holds all six deliverables. This SCOPE inherited the stale pointer.

**NLI / contradiction — ACTIVATE behind its gate; RETIRE the dead config; RE-SCOPE the mechanism.**
Track A traces the only complete adopt → measure → remove → regress → restore-proposed arc in the corpus. Two facts are load-bearing: (i) ass-035 measured NLI as structurally wrong for *Supports* but **explicitly instructed "do not change the contradiction detection path"** — crt-038 removed the `Contradicts` write anyway, as collateral to a ranking refactor, **never argued on its merits**; (ii) KI-CONTRADICT (#5548) already ratifies the framing: *"Domain-conditional: high value in a research domain, null in SDLC. **Do NOT prune on single-domain utility.**"*
→ Restore the write (~one branch), then run the deferred value spike (#899) **against a mature research corpus**. Do not decide on SDLC evidence. *Track D supplies the re-scope: the ecosystem has moved off standalone sentence-pair NLI for this task — production systems do **retrieval-gated adjudication at write time against a top-k candidate set**, and resolve by **supersession, not deletion**. Track D also notes dedup, entity resolution, and contradiction are **one retrieval-gated pass with three verdict types**, not three background jobs.*

**Phase / category — ACTIVATE, do not retire.**
Neither "real future" nor "dead weight": a finished, tested feature that reaches production and is then zeroed. Rebuilt every tick, consumed in search, applied at a live 0.05 weight explicitly *"raised from 0.0 — PhaseFreqTable activates this term"* — and every production caller passes `None`. The `tools.rs:794` comment is wrong about the cause: the phase is computed at `:743` and already used for usage recording and query logging. **It is in scope and deliberately dropped for scoring.** PD3 (#5530) is currently supplied by an op whose output reaches nothing.
→ Activate as a **measured** change, not a flip. Caveat (Track C/ass-052): a phase-signal feedback loop is precisely what disqualified the ass-031 GNN for the injection pipeline. Gate on SL-METRIC evidence.

---

## 4. Event-vs-tick — graph-currency recommendation

**Direction: graded hybrid. "Fix the window, then trigger the rebuild; do not patch the graph yet."** Mechanism explicitly not settled.

Three code facts reframe the question before any option is weighed:

1. **The staleness contract is misstated by ~15×.** Eight disclosure sites say "rebuilt each tick (typically 30–60 seconds)"; the shipped default is **900s**. ADR-004 vnc-019 declined a `graph_rebuilt_at` field *partly because* "the interval is already documented (30–60s)" — a false premise. #904's own framing inherits the same wrong number. **The largest currency defect is a config/disclosure problem, not an architecture problem.**
2. **The read side already pays O(N+E) per query.** Search deep-clones the whole graph **and** `all_entries` on every call — including when `use_fallback == true`, where both clones are pure waste. Three graph-read modes do the same.
3. **Incremental is structurally feasible and the global invariant is narrower than it looks.** `StableGraph` + `node_index` is the right shape; the sole whole-graph check is a **Supersedes-only** cycle test, and Supersedes derives from `entries.supersedes` — so high-volume typed-edge writes provably cannot violate it.

**Recommended sequence:**

1. **Correct the disclosed contract** to match the shipped default. Documentation-only, zero runtime risk, unblocked. Stops #904 and ADR-004's reasoning resting on a 15×-wrong number.
2. **`Arc`-swap the handle** (O-A). Removes the per-read O(N+E) clone; worth doing on perf merits alone; a prerequisite for every faster option (including incremental).
3. **Dirty-flag → debounced rebuild** (O-D) on a graph-owned cadence with a minimum-interval floor and **two trigger classes** — edge-table writes *and* `supersedes`/`status` writes. A hook keyed only on `graph_edges` would miss every `context_correct` and every deprecation. Delivers sub-tick currency with the **rebuild body untouched**, so invariants 1–11 stay free rather than becoming proof obligations.
4. **Add a graph-freshness health signal** (rebuild age, last-success, consecutive failures) to `context_status`. Without it no currency SLO is verifiable and I-9 persists under *any* mechanism. `EffectivenessState.generation` is the in-repo precedent to copy.

**Hold O-E (event-driven incremental patching) behind an explicit measured trigger.** It is feasible and is the only option whose cost does **not** grow with E — but it is the only one converting eleven free invariants into proof obligations, and the only one whose failure mode is **silently wrong** rather than merely stale (a stale `node_index` key resurrects a deleted node into the PPR result set). Blast radius spans the write path, the hot path, **and** six tick ops that bulk-mutate edges — "event-driven" cannot mean agent-writes-only.

**External corroboration (Track D), and it is unusually direct.** Zep — the state of the art — chose **hybrid**: dynamic extension for currency, periodic full recalculation for correctness, with the paper explicitly acknowledging *"gradual divergence, necessitating periodic complete recalculations."* RDFox and Stardog examined the same tradeoff and **split, with deletion cost deciding** — so **any incremental move must be costed on the retraction path, not the insertion path**; incremental insert is easy and misleadingly cheap. Differential dataflow's exactness offers a migration strategy worth adopting whenever O-E is revisited: run incremental and full rebuild together, **assert equality using the rebuild as an oracle**, then retire it. And: **a full tick-rebuild is immune to the entire truth-maintenance-under-retraction problem class — a real, under-credited virtue the current architecture would forfeit.**

**The coupling that decides the timeline:** O-C and O-D cost O(N+E) per rebuild, and **no edge demoter exists anywhere** (I-4). Unbounded edge growth is the hidden variable under every option except O-E. **Fixing edge demotion makes O-D durable and pushes the O-E trigger out; not fixing it eventually forces O-E.** These decisions must be ruled on together.

**Blocking measurement**: full-rebuild wall-clock at production corpus size. It gates the debounce floor, the O-E trigger, and any currency SLO. It does not exist.

---

## 5. The structural-gap set

### 5.1 Purge / hard-delete lifecycle — **absent**

There is **no hard delete of an entry in any production code path**: `Store::delete()` has zero production callers, `drop_all_data()` is a whole-DB import wipe, `VACUUM` reclaims pages not rows, and `retention.rs` targets cycles/sessions/observations only. Supporting absences: no `deprecated_at`/`expires_at`/tombstone column — **the data needed to answer "how long has this been deprecated?" does not exist**; `last_accessed_at` and `access_count` are written but read by **no sweep at all**. Meanwhile ~2504 quarantined entries sit forever and `generate_recommendations` emits *"N entries quarantined — review for resolution"* — advice with **no bulk-review surface and no purge path to act on**, firing indefinitely.

Track A adds the history: `context_purge` was named as a gap in ass-040 (*"can be deleted but no atomic delete + HNSW removal exists"*) and never built; ass-091 then **removed** the only purge verb as "dead surface" — correct locally, leaving no purge lifecycle at all. These are not in conflict: removing a caller-less destructive verb is not the same as having a lifecycle.

**Target shape (directional):**
1. Add a status-transition timestamp (`deprecated_at`) — lifecycle is currently unreasonable-about without it.
2. **Tiered, reversible**: `Quarantined` → (review window) → `Archived` (out of all serving paths, retained for audit) → (policy-gated) → purge. Track D F1-3 independently: **demote first, quarantine later, purge last, each step reversible.**
3. Gate on **learning utility, never age** — deprecated, superseded, zero injections, zero co-access, unreferenced by any active edge, outside the retained cycle window. All computable from data that exists. *This answers the SCOPE's "auto-quarantine aged/unaccessed DEPRECATED" question: **not as specified** — "aged" is the wrong predicate. Track D agrees flatly: age-triggered quarantine is the exact anti-pattern the temporal-RAG literature names.*
4. **Default-OFF and domain-tunable.** Compliance domains may forbid deletion outright; the audit trail survives regardless.
5. **Never purge un-reviewed entries** (RETAIN's proven invariant).
6. **Fix the reversibility inversion (#889/#890) first.** Do not build purge atop a lifecycle whose *reversible* state already destroys edges irreversibly.
7. **[Track D F1-2] Purge must repair the index, not just drop the row.** HNSW has no deletion API; a tombstone leaves the vector in the graph, consuming memory and participating in routing. The Ghost Vectors preprint (2026, unreviewed — treat exploitability as unconfirmed, but the structural claim as sound since it follows from how HNSW works) shows deleted geometry is reconstructible from retained neighbour topology. **A purge that leaves HNSW untouched is not a purge.**

### 5.2 HNSW / index heal — **partially exists; repair and detection absent**

Distinguish carefully. **(a) The heal pass that exists** is well-built — ordered deliberately after the prune, DB write last so a crash re-heals idempotently. Its gap is narrow: both sub-cases filter `status = 0`, so Deprecated/Proposed entries with `embedding_dim=0` are **never** embedded, despite being readable via `context_get` and carrying supersession chains.

**(b) Index-health repair — genuinely absent.** `remove_entry` erases only the `IdMap`; the vector stays in the graph as a soft tombstone. The **only** eviction is a full rebuild-and-swap that evicts **by omission**. And there is **no drift detection of any kind** in either direction: nothing compares `vector_map` to HNSW contents; `load()` validates only dimension and file existence. Worse, `graph_stale_ratio` is `point_count.saturating_sub(id_map.active_count())` — pure in-memory arithmetic that never consults SQLite, and because of `saturating_sub` **the inverse fault floors to a healthy-looking `0.0`**.

**Target**: a periodic three-way reconciliation across `entries(status=0)` ⟷ `vector_map` ⟷ HNSW `IdMap` that **counts and reports divergence in both directions**, repairs the repairable, and surfaces the rest. Both existing passes already enumerate the drifted rows before repairing them — **the counts are in hand and thrown away.** Pair with #828 (versioned atomic graph+data flip), which removes the crash window that manufactures drift.

**[Track D F4-3, and this changes the priority] Index repair is recall-preservation, not space reclamation.** FreshDiskANN's consolidation re-links the graph around removed nodes with the stated purpose of improving *search quality*. HNSW recall degradation after heavy mutation is **silent** — no error, no exception, no log; queries just quietly return worse neighbours. **This is the most dangerous failure mode in the entire map, because nothing surfaces it** — and its only detector is a golden-set regression harness (§6).

### 5.3 Corpus-health / engine-health monitoring — **absent, and it is the keystone**

`context_status` is a genuinely rich **corpus** surface (49 fields). It is bolted onto a tick with **essentially zero self-observability**:

- No metrics surface of any kind — no Prometheus, no `/metrics`, no gauges, no timers.
- `/health` checks nothing — compile-time constants, *"no I/O, no database access."* It returns 200 if the process can format a string.
- **No per-pass log and no duration on the production path.** Duration is computed in exactly one place — on the **stdio path the daemon never runs** (I-7 surfacing as an observability gap: the better-instrumented copy is the one nobody executes). A fully successful pass produces **2 log lines per slug**; a pass where nothing fires produces **zero**.
- **The three tick-health fields are dishonest by omission.** `last_maintenance_run` is written only on success; `next_maintenance_scheduled` advances **unconditionally**. **A tick failing for a week reports a next-run 15 minutes out.** No failure count, no last error, no staleness flag, no duration.
- **No persisted tick ledger.** The only persisted signal is one negative-path audit event, reachable only on `Err` — and therefore **never** in the silent-degradation case (I-10) — and **nothing can read it back**: no MCP tool, no HTTP route, no CLI reads `audit_log`.

So: a tick that **errors** leaves one row; a tick that **silently degrades**, **succeeds**, or **never fires** leaves nothing.

**On LAMBDA-HONEST**: the doctrine (#5555, *"never a fake 0"*) governs *corpus* metrics. **It has never been applied to the engine's own liveness** — `next_maintenance_scheduled` advancing on a dead tick is precisely the fake 0 it forbids, one level up. The missing tick-health NFR is confirmed missing and **structurally homeless**: there is no global NFR registry. And the tick — sole supply line for ≥6 capability nodes, **two marked `delivery:proven`** — **appears nowhere in the capability map as an entity.** Nothing records that those proofs are conditional on a background loop continuing to complete.

**Target signals** (nearly all already computed and discarded): tick last-success age + `is_stale`; consecutive failures, last error, per-job outcome; per-pass and per-job duration; **orphaned-edge delete count** (computed at `background.rs:820`, discarded at `:827` — the cheapest win in the campaign); repointed-edge count; index⟷DB drift both directions; confidence-refresh saturation (currently `debug!`); extraction fail-open admissions (currently **no log at any level**); graph serving-stale flag. Plus a **readiness** dimension on `/health` distinct from liveness: process-up vs. corpus-served-from-a-currently-maintained-index.

---

## 6. Gap vs external best practice — net-new only

Track D's verdicts were formed **blind to Unimatrix** by design. Adjudicated against Tracks A and C, the genuinely net-new set is:

| # | Pattern | Adjudicated verdict | Why it survives |
|---|---|---|---|
| 1 | **Declarative constraints + a violation *report*, not a *block*** (SHACL/Wikidata/Stardog) | **NET-NEW — highest value-per-effort** | No ML, no LLM, no new storage — deterministic graph queries on a schedule. Wikidata maintains the largest open collaborative KG by **reporting and triaging, not blocking**. Converts "the corpus is probably fine" into a number that trends. Nothing in the ledger proposes it. |
| 2 | **Corpus-health scorecard** (Guru Internal Trust Score) | **NET-NEW** | ass-032/#413 shipped graph-cohesion *metrics*; nothing publishes a single trending scalar. Candidate signals are mostly counting queries over structure that already exists — including the **unretrieved-entry fraction**, cheap and highly diagnostic, and directly answering ass-032 §3.1's unresolved finding that *94% of lesson-learned entries have never been accessed*. |
| 3 | **Bi-temporal validity — supersede, don't delete** (Zep/Graphiti; SQL:2011) | **NET-NEW** | Status is a single-axis field: it can say "deprecated *now*"; it cannot reconstruct "what the corpus asserted on 2026-03-14." Track A confirms the nearest prior art — `deprecated_at` + `as_of` queries (ass-057 UQ-2) — was **deferred to Ph.3+**. This is the substrate that makes aggressive lifecycle policy *safe to automate*. |
| 4 | **Asynchronous improvement pass ("sleep-time compute")** (Letta; A-MEM) | **NET-NEW — largest conceptual gap** | The tick is a *consistency* mechanism; nothing is an *improvement* mechanism. This is why knowledge synthesis (ass-022/04), ACE-style grow-and-refine, A-Mem link evolution, and taxonomy evolution were all deferred and never rescheduled — **they have no home**. A budgeted, separately-observable pass, allowed to be expensive and allowed to be *skipped under load*, is not implied by a rebuild tick. Note Letta's architectural constraint: the interactive agent is **denied** the tools to edit core memory; those belong exclusively to the background agent. |
| 5 | **Golden-set retrieval regression** | **NET-NEW** | The **only** detector for silent index and embedding degradation. Also the direct answer to ass-074's primary discovery — *"the platform is steering its graph features blind"* — and to SL-METRIC's `partial` status. |
| 6 | **Last-access decay + never-decaying importance + access reinforcement** (Generative Agents, UIST '23) | **NET-NEW (the decay/reinforcement halves)** | ass-032 covered the ranking half. **Last-access framing resolves "age ≠ staleness" without any domain exemption** — the single most useful sharpening the outside view offers this campaign. Honest caveat: reinforcement creates rich-get-richer, and **no settled mitigation exists in the surveyed literature** — an open problem in the field, and it rhymes with the exposure-bias hazard (#3429) already recorded internally. |
| 7 | **Per-entry temporal-sensitivity classification** | **NET-NEW** | Sharpens the domain-pack lens exactly where per-domain is provably too coarse. `EMERGING` maturity (preprints) — weight accordingly. |
| 8 | **Retrieval-gated conflict adjudication** (write-time, top-k candidates, three verdicts) | **NET-NEW as a mechanism** | See §7 tension T-3. The retrieval gate is what collapses the O(N²) cost that killed the previous attempts. |
| 9 | **Purge that repairs the index** | **NET-NEW** (§5.1) | |
| 10 | **Index consolidation as recall-preservation** | **NET-NEW** (§5.2) | |
| 11 | **Statement-level provenance + confidence** (PROV-O, RDF-star, Zep episodes) | **PARTIAL** | `graph_edges.source` + `signal_origin` + the hash chain are a strong substrate. What is likely absent is the **discipline** — mandatory per-assertion attribution and the **bulk retraction operator provenance exists to enable** ("retract everything from source S"). Note Track A: `previous_hash` is written as an empty string — *the field exists in schema but is unused as a chain* — and ass-020 P5's "verify the chain on read" is still DEFERRED. |
| 12 | **Truth maintenance under retraction** (RDFox DRed/FBF vs Stardog's refusal) | **NET-NEW as a *constraint*, not a feature** | A caution on §4, not a capability. |

**Deliberately not adopted:** Guru-style human verification-with-expiry (**PARTIAL** — the state machine transfers, the human review queue does not; an agent-written corpus has no SMEs to drain it). Track D records the live conflict honestly — Guru says freshness needs human re-attestation, Glean says derive it from the source, the agent-memory literature effectively sides with usage signal — and does not resolve it. **Neither do we.**

---

## 7. Cross-track tensions — resolved explicitly

**T-1 — Freshness half-life: "hardcoded global" (A) vs "already domain-tunable" (C).**
**Both are true, at different times.** Track A mined *spike documents*, where the ass-022-era disposition stands: a global hardcoded constant, rated *"Critical, ~1h"* to externalize, with the per-category form specified, costed at 2–3 days, and **deferred twice**. Track C read *current code*: `Preset` now carries per-domain half-lives (8760/720/24) with an operator override, range-validated at boot. **The instance-level knob was built in the interim; the ledger's disposition is stale.**
Residual gap: granularity. Per-*instance/preset* exists; per-*category* (ass-022/03) and per-*entry* (Track D F3-2) do not. **The SCOPE's canonical example is solved at the coarsest useful level and unsolved at the two finer ones.**
The apparent RETAIN contradiction dissolves under §2.2's doctrine: age informing **confidence** is permitted and tunable; age driving **retention** or **health** is not. RETAIN and the live half-life are not in conflict. **The one genuine live violation is `cleanup_stale_co_access`** — a wall-clock 365-day DELETE on a *learning signal*, hardcoded, while the edges promoted from that signal live forever with no demotion owner. That is the exact failure the doctrine exists to prevent, and **it is invisible because nothing counts what it deletes.**

**T-2 — GNN: "evidence favours retire" (A) vs "retire the commitment, keep the hooks" (C).**
Not a contradiction; different objects. Track A's "retire with reason" targets the **roadmap commitment**; Track C separates that from the ~40 lines of inert, honestly-documented staging. Track D finds **no external support for GNNs in background maintenance** but explicitly flags this as weak evidence (absence of evidence, non-exhaustive search). **Resolution: retire the commitment; keep the hooks; do not re-litigate from first principles.** Any revival must first answer the exposure-bias objection (#3429) and clear SL-METRIC.

**T-3 — NLI: "restore, one branch" (C) vs "the ecosystem moved off standalone NLI" (D).**
The sharpest tension in the campaign, and it is **sequencing, not disagreement**. Track C is right that the removal was collateral to a ranking refactor, never argued on its merits, and that restoring the discarded write is ~one branch behind an existing gate. Track D is right that a sentence-pair classifier trained on short self-contained pairs degrades on long, context-dependent, domain-specific text — which is what a knowledge corpus is.
**Blocking constraint Track D could not see**: Unimatrix is **architecturally LLM-agnostic** (ass-034 rejected LLM annotation at store time on that basis), and ass-036 hard-failed the local-LLM alternative (Phi-3-mini: 44% correct, 70% FP, 24s/pair). **Track D's recommended mechanism is not adoptable as specified.**
**Resolution — adopt the *shape*, not the model.** (i) Restore the write behind `nli_enabled` now; it is cheap, gated, and reverses an unargued removal. (ii) Adopt Track D's three structural corrections regardless of scorer: **write-time against a retrieved top-k candidate set** rather than an all-pairs background scan (op 8 already generates candidates via HNSW — the substrate exists); **resolve by supersession, not deletion**; and **treat dedup + resolution + contradiction as one retrieval-gated pass with three verdict types**, not three jobs. (iii) Leave the value verdict to the deferred spike (#899) on a mature research corpus — KI-CONTRADICT already forbids pruning on single-domain utility.

**T-4 — Decay: RETAIN forbids time-based expiry (A) vs Track D recommends decay (D).**
**Convergent once "last access" is read carefully.** Last-access decay is a **usage** signal, not an age signal, and therefore squarely inside RETAIN's "learning utility" predicate. The mode RETAIN rejects — and the SCOPE correctly calls wrong for SDLC — is **creation-time** decay, a strictly worse and more common variant. Track C independently found the substrate already written and unread (`last_accessed_at`, `access_count`). **No conflict; this is a convergence worth acting on.**

**T-5 — Event-driven: Track B's graded hybrid vs Track D's IVM framing.**
Convergent. Track D's own evidence (Zep's hybrid with acknowledged divergence; RDFox-vs-Stardog splitting on deletion cost) *supports* Track B's recommendation against a rewrite, and adds two things Track B did not have: **the retraction-path costing rule** and the **oracle-based migration strategy** (validate incremental against the rebuild, then retire it). Track D also credits the tick with a virtue no internal track named: **immunity to the truth-maintenance-under-retraction problem class.**

**T-6 — Purge: "removed as dead surface" (A) vs "build a purge lifecycle" (C).**
Not a contradiction. ass-091 removed a *destructive verb with no natural caller* — correct locally. It left **no purge lifecycle at all**, which is a different (and older, ass-040) gap. The lesson to carry: purge must be a **policy-gated lifecycle stage**, never an agent-invokable verb.

**T-7 — Measurement caveat propagates.**
Track A O-9 and ass-074 establish that P@K/MRR collapse to a cosine proxy, so **every graph-side disposition citing an MRR delta inherits the caveat** — including the +0.0031 MRR that justified removing the `Contradicts` write. Conversely (O-10): **where a measurement gate existed, the corpus self-corrected** — ass-074 overturned ass-037, ass-038 disproved its own density hypothesis, ass-039 invalidated the entire prior eval scenario set. **This is the strongest internal argument for Track D's #1/#2/#5 (constraint report, scorecard, golden set): the corpus has demonstrated it corrects itself when it can see, and rests on code-reading alone when it cannot.**

---

## 8. CANDIDATE CAPABILITIES — input for uni-zero

*Proposed only. Not authored, not stored. Names are placeholders; goal attribution is a suggestion.*

| ID | Candidate | Goal | What it would assert | Notes for authoring |
|---|---|---|---|---|
| **CC-1** | **TICK-LIVENESS** | integrity | The maintenance engine reports its own liveness honestly — last-success age, consecutive failures, per-job outcome, duration — and never reports health it has not achieved | **Prerequisite for everything.** Applies LAMBDA-HONEST to the engine itself. Also the missing tick-health NFR's home. **Register the tick as an entity in the capability map** — it is the sole supply line for ≥6 nodes incl. two `delivery:proven`, and nothing records that dependency |
| **CC-2** | **CORPUS-SCORECARD** | self-learning | Corpus health is a small set of trending scalars, published and readable, computed from structure that already exists | Track D #2. Most signals are counting queries. Would also give Track A O-3's scale-gated deferrals a re-check trigger |
| **CC-3** | **CONSTRAINT-REPORT** | integrity | Structural invariants are declaratively expressed and violations **reported and triaged, never blocked at write** | Track D #1 — highest value-per-effort in the map. Composes with CC-2 |
| **CC-4** | **INDEX-RECONCILE** | integrity | `entries ⟷ vector_map ⟷ HNSW` divergence is detected in **both** directions, counted, repaired where repairable, and surfaced where not | Today's `graph_stale_ratio` floors the inverse fault to a healthy `0.0`. Framed as **recall preservation** |
| **CC-5** | **PURGE-LIFECYCLE** | integrity + domain-agnostic | Removal is a tiered, reversible, policy-gated lifecycle — utility-predicated, default-OFF, audit-preserving, index-repairing, never touching un-reviewed entries | Blocked behind fixing #889/#890. Needs a status-transition timestamp |
| **CC-6** | **EDGE-DEMOTION** | self-learning | Every promotion path has a matching demotion owner; the graph does not grow monotonically | **The campaign's largest unowned gap.** ass-079 scoped it and never ran. Determines how long O-D stays viable before O-E is forced |
| **CC-7** | **GRAPH-CURRENCY** | self-learning | The served graph is current within a **disclosed and observable** window that matches the shipped configuration | Subsumes the 15× disclosure defect, `Arc`-swap, debounced rebuild, and the freshness signal |
| **CC-8** | **LIFECYCLE-PRESET** | domain-agnostic | Lifecycle **policy** — retention aggressiveness, promotion thresholds, quarantine sensitivity, contradiction strictness, purge posture — is domain-tunable through the existing preset surface; **structure is not** | Extends shipped, boot-validated, per-slug-overlayable machinery. Versioning implications land on **PL-2** (#5700, `delivery:missing`) |
| **CC-9** | **RETAIN-ON-ERROR** *(may be an invariant, not a capability)* | integrity | No Principle-7 cache is ever replaced with degraded state; serving stale-but-valid always beats serving degenerate | One-line doctrine fixing the shared root of I-3, I-5, I-9. Op 1e is the model |
| **CC-10** | **AS-OF / bi-temporal** | integrity | The corpus can answer what it asserted at a past instant; supersession invalidates rather than erases | Track D #3. Substrate that makes CC-5 and CC-11 safe to automate |
| **CC-11** | **CONFLICT-ADJUDICATION** | integrity | Conflicting knowledge is detected at write time against a **retrieved candidate set** and resolved by supersession — domain-conditional in strictness, never pruned on single-domain utility | Re-scopes KI-CONTRADICT (#5548, `delivery:missing` — REGRESSED). Absorbs dedup + resolution as sibling verdicts |
| **CC-12** | **IMPROVEMENT-PASS** | self-learning | A budgeted, separately-observable background pass exists whose job is making the corpus **better**, distinct from reconciling it — allowed to be expensive, allowed to be skipped under load | Track D #4 — largest conceptual gap. The missing home for synthesis, consolidation, taxonomy evolution, link evolution |
| **CC-13** | **RETRIEVAL-REGRESSION** | self-learning | Retrieval quality is measured against a stable labeled set on a cadence, so silent degradation is detectable | Directly addresses ass-074's "steering blind" and SL-METRIC's `partial` (#803) |

---

## 9. Targeted execution roadmap — ass-103's fixes re-framed against the target

Ordering is by **dependency and evidence-generation**, not severity. The organizing principle: **make it observable, stop it destroying data, then make it policy, then make it fast, then raise the bar.**

**Wave 0 — Free wins and honest signals** *(no design required; several are one line)*
Surface the orphaned-edge delete count (computed then discarded one line later — **cheapest win in the campaign**) and the repoint count · correct the 15× staleness disclosure in all eight sites · retain-on-error for **I-3** and **I-5** · retire `gc_audit_log`'s call, the four dead NLI config fields (one of which enforces a cross-field invariant against another dead field and **can fail startup**), `[graph_penalty]`, op 1m, op 1n (post-verification) · fix **I-8** (S8 weights overwritten), **I-6** (poisoned mutex pins `current_tick` to 0 forever, inverting a cost control), **I-19** (interval 0 → infinite panic-restart), **I-20** (tokio `Burst` catch-up storm) · raise extraction fail-open (**I-23**) and malformed-payload drops (**I-22**) above silence · fix the ass-029 → ass-031 roadmap pointer.
→ *Serves CC-1, CC-9. Nothing else is blocked on it, and it is the cheapest evidence the campaign can buy.*

**Wave 1 — Observability (the keystone)**
Tick liveness/duration/failure fields; per-pass summary log on the **production** path; an audit read surface; corpus-health scorecard v1 from signals that already exist; constraint-violation counts; `/health` readiness distinct from liveness; graph freshness signal (`context_status`).
→ *CC-1, CC-2, CC-3. **Everything after this wave becomes measurable.** Without it, no later claim in this roadmap is verifiable.*

**Wave 2 — Stop destroying data**
**#889/#890** — the reversibility inversion (compaction deletes edges to `Proposed`, a live pending state, and to `Quarantined`, a reversible one, **in the same pass that auto-quarantines the entry**) · the `pre_quarantine_status` bypass (auto-quarantined Deprecated entries **silently restore as Active** — a status *promotion* via a maintenance path) · auto-quarantine hysteresis (**I-12**) and retry bound (**I-11**) · **I-10** (effectiveness snapshot failure silently disables auto-quarantine with no audit event).
→ *CC-9, prerequisite for CC-5. **Purge cannot be built on a lifecycle whose reversible state already destroys data.***

**Wave 3 — Policy surface**
Extend `Preset` into a lifecycle-policy preset · expose the promotion thresholds and edge weights · move `auto_quarantine_cycles`, tick interval, compaction trigger, contradiction cadence, and the 365-day co-access window out of env/consts · give HNSW parameters a config path · **write down the age doctrine** (three lines, rediscovered four times).
→ *CC-8. Coordinate with PL-2 (#5700) on contract versioning.*

**Wave 4 — Currency and cost**
`Arc`-swap the handle · dirty-flag → debounced rebuild on a graph-owned cadence with two trigger classes · single-pass maintenance snapshot (**I-16**: five full active-set reads per tick → one) · split `build_report` (**I-17**) · cap the compaction re-embed (**I-15**) · fix `generation` bumping unconditionally (**I-13**) · surface confidence-refresh saturation (**I-14**) · resolve **I-7** (two divergent copies of tick logic; **stdio runs the un-fixed one**).
→ *CC-7. Blocked on a wall-clock measurement that does not exist — see §10.*

**Wave 5 — Structural gaps**
Edge demotion (**CC-6** — decides Wave 4's durability) · index reconciliation, both directions (**CC-4**) · purge lifecycle with index repair (**CC-5**) · extend the heal pass past `status = 0`.

**Wave 6 — Activate**
Thread `current_phase` (gated on SL-METRIC evidence, not a flip) · restore the `Contradicts` write behind `nli_enabled`, then run the deferred value spike on a research corpus · populate `s2_vocabulary` with a cost guard · flip `resolve_supersessions` · give `RelatedTo` a write path.

**Wave 7 — Raise the bar**
Golden-set regression (**CC-13**) · bi-temporal validity (**CC-10**) · retrieval-gated conflict adjudication absorbing dedup and resolution (**CC-11**) · the asynchronous improvement pass (**CC-12**) · last-access decay + never-decaying importance + reinforcement · per-entry temporal-sensitivity attribute.

**Explicitly not on the roadmap**: event-driven incremental graph patching (held behind a measured trigger); GNN delivery (commitment retired, hooks kept); a human verification-review queue (no staffing model); uniform creation-time decay (anti-pattern by three independent lines of evidence).

---

## Unanswered Questions

*Merged and deduplicated across all four tracks. Bracketed tag = what would resolve it.*

**Blocking measurement (nothing here is answerable at `directional` confidence)**
1. **Full-rebuild wall-clock at production corpus size** — gates the debounce floor, the O-E escalation trigger, and any currency SLO. *[empirical spike]*
2. **Real cost of the per-read graph clone at p50/p99 search latency** — `Arc`-swap's win is argued structurally, never measured. *[profiling]*
3. **Real cost of the heuristic contradiction scan** — a full-corpus re-embed every 4 ticks for one Lambda scalar. *[measurement]*
4. **How much index⟷DB drift actually exists** — unmeasurable today *precisely because nothing counts it*. Chicken-and-egg: needs CC-4 to exist first.
5. **Does faster graph convergence change PPR output materially?** ass-088 flagged the identical question as "directionally safe, unmeasured." *[retrieval eval]*
6. **Do the scale-gated deferrals now qualify?** Leiden ">500", synthesis ">200 clustered", SimCSE "≥2000", S3/S4 "≥3000", the filtered-PPR probe "≥5K". Several have plausibly been crossed and **nothing re-evaluates a deferral when its gate opens.** *[measurement pass; CC-2 would automate it]*
7. **How many of the 2504 quarantined entries are auto- vs human-quarantined?** Bears directly on purge policy and on the `pre_quarantine_status` defect's blast radius. *[live corpus query]*

**Open decisions (for uni-zero + human)**
8. **What is the disposition of behavioral `Informs`?** ass-079 posed four ranked options (retire / keep-and-fix with accumulate-decay / redefine / status quo) and **never ran**; also flagged the first-write-wins weight freeze as untested. **Direct owner of CC-6.** *[needs a spike]*
9. **Should ADR-004 vnc-019 be amended or superseded?** Its reasoning rested partly on the false 30–60s premise; whether the *conclusion* survives is an ADR judgment. *[SCOPE bars ADR amendment]*
10. **Extend `Preset`, or add a new lifecycle-policy section?** Versioning implications land on PL-2 (#5700, `delivery:missing`).
11. **Under an incremental path, how do tick-authored bulk edge writes participate?** "Event-driven" cannot mean agent-writes-only. *[mechanism design]*
12. **Keep or delete the write-side `run_redirect_loop` fast-path; new `EdgeConvergenceJob` vs extending Job 2.** ass-088 explicitly reserved both for the ADR.
13. **The TypedRelationGraph staleness window** — accept / fallback-SQL / partial-refresh all left open by ass-057 OQ-B-4.
14. **Does activating `current_phase` improve retrieval, or feed a bias loop?** Blocked on SL-METRIC (#5572, `partial`, #803).
15. **Is contradiction detection valuable in a research domain?** #899 defers the verdict by design.
16. **Does `context_correct`'s hard reset of confidence/access_count/helpful/unhelpful cause material learning-signal loss?** ass-093 flagged it "under-considered."
17. **Was the MicroLoRA-vs-scalar-boost overlap ever evaluated?** Deferred to col-015/#50; architecture called "transitional," the 0.03 cap "provisional"; no later spike revisits it.
18. **What is the disposition of ass-042's questions?** (write attribution, cross-agent contradiction cadence, poisoning-based privilege escalation, per-agent rate limiting) — SCOPE-only, no findings; overlaps SLN1 (#5528, `delivery:asserted`, threat model "uncharacterized").
19. **Does the GRAPH_EDGES-Supersedes dual-source gap matter?** Supersedes rows are written but skipped by the graph builder; `revision_reason` is "invisible to all graph traversal logic."
20. **What replaces `sessions.keywords`?** "dead schema — implement or remove," never answered anywhere in the corpus.
21. **Does retiring op 1n require a deployment-wide completion check?** *[operational data]*

**Open in the field (not just here)**
22. **Rich-get-richer under access reinforcement** — real failure mode, **no settled mitigation found**. Rhymes with the internally-recorded exposure-bias hazard (#3429).
23. **Guru-style attestation vs Glean-style derived freshness** — two credible products, opposite answers; agent-memory literature on a third path (usage signal). Deliberately unresolved.
24. **Does retrieval-gated LLM adjudication stay affordable at scale?** Zep and Mem0 both do it; **neither publishes per-write cost.**
25. **What half-life is defensible for an SDLC-like domain?** γ=0.995/hr is tuned for a simulation. No source gives a defensible default. *[needs the golden set]*
26. **Is incremental constraint validation tractable?** Confirmed an open research problem — do not assume continuous constraint checking is free at scale.
27. **Is the Ghost Vectors attack practical?** 2026 preprint, unreviewed, no replication. Structural claim sound; exploitability unconfirmed.

---

## Out-of-Scope Discoveries

*Merged and deduplicated. Carry-forwards only — none pursued.*

**Likely warrant a GitHub issue** *(filing is the human's call — outward commitments are not auto-filed)*
- **Auto-quarantine bypasses `pre_quarantine_status`** (`background.rs:1440` uses `update_status`, not `update_entry_status_extended`) → auto-quarantined Deprecated/Proposed entries **silently restore as Active**. A status *promotion* via a maintenance path. Not in ass-103's register; appears to be a new finding.
- **`gc_audit_log` is a triple defect** — inert validated knob, per-tick dead work, false `info` "cleanup" line, and an unhonored compliance-facing retention promise. vnc-014 accepted unbounded audit growth as a documented limitation but never withdrew the knob.
- **The disclosed staleness contract is wrong by ~15× in eight locations.**
- **Four dead NLI config fields still pass boot validation, including a cross-field invariant between two of them.** Dead config that can *fail startup* is worse than dead config.

**Cheap corrections**
- The roadmap's GNN pointer is stale (`WAVE2-ROADMAP.md:228` tracks ass-029 as "not started"; ass-031 holds all six deliverables).
- `phase_affinity_score()`'s doc claims a PPR caller that does not exist while `search.rs:1030` says *do NOT call it* — two docs in direct contradiction.
- Three duplicated `EF_SEARCH = 32` constants will drift (same class as I-7).
- Documentation/code divergence on NLI: `a870d073` scrubbed docs so the system *"presents as cosine-only"* while ~95% of the substrate remains and its config still validates. The operator-facing surface **denies a capability the code still carries.**

**Standalone performance (unrelated to currency)**
- The per-request full graph clone is **entirely wasted when `use_fallback == true`** — a free early-return win.
- `find_terminal_active` linearly scans `all_entries` per visited node — O(depth × |all_entries|) per call, once per superseded candidate in the search loop.
- `all_entries` is a second, parallel cache living inside the graph handle. It is an entry cache, not a graph. **Whether it belongs there is a taxonomy question** raised and not settled here.

**Knowledge-map integrity**
- **Two capability nodes are `delivery:proven` on an unrecorded condition** — RETAIN (#5581) and C5 (#5550) rest entirely on the tick completing. **Worth checking whether other `proven` nodes carry unrecorded liveness conditions.**
- **ass-020's critical tick findings recur verbatim in ass-103** — `compute_report` inflation (#1777 → I-17) and panic-kills-the-tick (P1 → I-19). Old audit findings re-emerging in new locations, with no mechanism that would have caught the recurrence.
- **A very large LATENT surface exists whose dominant cause is not oversight** — the consumer was deferred and the hook shipped anyway. Worth a standing rule: *a forward hook ships only with a named owner and a re-check trigger.*

**Candidate future spikes**
- **`unimatrix run` (ass-066) would reshape the signal supply.** Event-driven session host assessed feasible (2–3 weeks for observation parity), deferred. If it lands, several tick-based observation paths become redundant.
- **Memory-injection attacks on agent-written memory** (ER-MIA) — an engine accepting agent-authored writes has this surface by construction. A threat-model spike, not a background-processing one. Overlaps SLN1 (#5528) and ass-042's never-answered questions.
- **LazyGraphRAG's defer-to-query-time inversion** — comparable quality at reported >700× lower query cost by avoiding regular full re-indexing. **The strongest single argument found for "do less in the background"**, and it deserves a hearing before Wave 7 adds passes.
- **Emerging memory-system evaluation benchmarks** could supply a ready-made golden set for CC-13. Small spike, high leverage.
- **ACT-R base-level activation** as a decay model — 30-year-validated, published parameters, better grounded than the ad-hoc exponentials most agent systems use. Candidate if a decay model is ever designed for real.
- **Ghost-vector reconstruction as a *privacy* issue** (distinct from its lifecycle implication) — if the corpus ever holds sensitive material.

---

## Recommendations Summary

*One line per recommendation. Direction only — mechanism choices and capability authoring belong to uni-zero + human.*

- **Function taxonomy**: the 24 ops collapse to **seven** purposes (currency, graph healing, learning-signal, lifecycle, storage governance, index governance, acquisition) — and the collapse exposes six **missing** functions: engine self-observation, corpus quality assessment, asynchronous improvement, structural removal, index reconciliation, and edge demotion.
- **Universal vs domain-tunable**: structure is universal, **policy is tunable** — and the tunable surface is **`Preset`**, not the domain pack, which has no numeric field and which the tick never reads.
- **Latent params**: the target state is dominated by **activate/expose**, not build-new — five one-field activations, ~25 hardcoded knobs to expose, six inert config items to retire; only the structural gaps are genuinely greenfield.
- **Doctrine to write down (three lines, rediscovered four times)**: age MAY inform confidence (tunable); age MUST NOT drive health; age MUST NOT drive retention — **and where decay applies, decay on last access, never on creation.**
- **Retain-on-error**: adopt as a stated universal invariant across all five Principle-7 caches; op 1e is the model. Single root beneath I-3, I-5, I-9.
- **GNN**: **retire the commitment, keep the hooks, fix the stale ass-029 → ass-031 pointer.** It cannot be scheduled ahead of its own yardstick (SL-METRIC, itself `partial`).
- **NLI/contradiction**: **activate the discarded write behind `nli_enabled`** (~one branch, reversing an unargued removal), adopt the **retrieval-gated, write-time, supersede-don't-delete** shape, retire the four dead config fields, and leave the value verdict to a research-corpus spike — **never decide it on SDLC evidence.**
- **Phase/category**: **activate, measured, not flipped** — the feature is finished, tested, reaches production at a live 0.05 weight, and is zeroed by a `None` the caller already holds.
- **Event-vs-tick**: **graded hybrid** — (1) fix the 15× disclosure, (2) `Arc`-swap the handle, (3) dirty-flag → debounced rebuild with **two** trigger classes, (4) add a freshness signal. **Hold incremental patching behind a measured trigger**; it is the only option whose failure mode is *silently wrong* rather than merely stale, and the external evidence supports "add an incremental path, keep the rebuild as reconciler" — never "replace the tick."
- **Edge demotion is the campaign's largest unowned gap** and it is **coupled to the currency decision** — fixing it makes the debounced rebuild durable; not fixing it eventually forces the incremental rewrite. Rule on them together. The spike scoped to answer it (ass-079) never ran.
- **Structural gaps**: purge lifecycle (tiered, reversible, utility-predicated, default-OFF, audit-preserving, **index-repairing**) — **blocked behind fixing #889/#890**; three-way index reconciliation counting drift in **both** directions; and engine self-observability.
- **External bar**: Unimatrix has tier 4 of four; the gap is tiers 2 and 3. The **net-new, lowest-cost, highest-value** additions are a **declarative constraint-violation report** and a **corpus-health scorecard** — no ML, no LLM, mostly counting queries over structure that already exists. Then bi-temporal supersession, a golden set, and a distinct **improvement pass**.
- **Sequence**: make it observable → stop it destroying data → make it policy → make it fast → raise the bar. **Wave 1 gates everything**: the corpus has repeatedly self-corrected where a measurement gate existed and rests on code-reading alone where none did.
- **Cheapest win, do it first**: surface the orphaned-edge delete count — already computed, discarded one line later.

---

**Confidence**: DIRECTIONAL, as the SCOPE requires. Every claim in this synthesis is drawn from one of the four track findings or the ass-103 baseline; no new investigation was performed. Tensions T-1 through T-7 are resolved by reconciling the tracks' evidence, not by new evidence. All cost and volume figures are **shapes inherited from code reading, not measurements** — the campaign's own headline gap. No capabilities were authored, no ADRs amended, no mechanisms settled, and **no Unimatrix writes were made.**