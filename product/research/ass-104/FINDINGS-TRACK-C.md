# FINDINGS: ass-104 TRACK C — Integrity & performance target

**Spike**: ass-104 (Track C of a 4-track parallel campaign)
**Date**: 2026-07-20
**Approach**: investigation / evaluation (read-only; code + Unimatrix internal)
**Confidence**: **DIRECTIONAL** — envisioning only. Dispositions and direction, not settled mechanism design.
**Baseline**: ass-103 FINDINGS (the 24-op tick inventory). This track establishes what those ops **should be**.

> **Lane discipline.** Track A owns the option ledger, Track B owns event-vs-tick graph currency, Track D owns external best practice. Cross-track observations are recorded under *Out-of-Scope Discoveries*, not pursued.

---

## Headline

**The domain-conditional-policy capability the SCOPE Framing asks for is not missing — it is built, shipped, boot-validated, and applied to exactly one behavior.**

`Preset` (`infra/config.rs:165`, resolved `:3719-3768`) is a five-value **knowledge-lifecycle preset** — `Collaborative` / `Authoritative` / `Operational` / `Empirical` / `Custom` — and each preset carries its own freshness half-life:

| Preset | `freshness_half_life_hours` | `w_fresh` | Implied domain |
|---|---|---|---|
| Authoritative | **8760** (1 yr) | 0.10 | durable/compliance |
| Collaborative (default) | **8760** (1 yr) | 0.18 | SDLC |
| Operational | **720** (30 d) | 0.24 | ops |
| Empirical | **24** (1 d) | 0.34 | news/sensor |

`ConfidenceParams` names this outright (`unimatrix-engine/src/confidence.rs:132-134`): *"The six `w_*` fields carry the **per-domain** weight vector set by the active preset."* The override is operator-facing (`config.toml:63-68`), range-validated at boot (`config.rs:3458-3475`, `HALF_LIFE_MAX_HOURS` `:56`) — the very validation DA6 (#5545) is proven by.

So the SCOPE's canonical example — *"time-based decay: right for news/ops, wrong for SDLC; age ≠ staleness"* — is **already solved, once, for confidence scoring**. It was solved by direct experience: bugfix-426 (lesson #3704) found a 168 h half-life "silently kills ADR and convention confidence," and crt-048 (ADR #4199) went further, **deleting** the time dimension from the Lambda health metric because it was "actively backwards," while RETAIN (#5581) fixed retention as *cycle-based, not wall-clock*.

**The gap is therefore not capability — it is coverage and consistency.** Three findings define this track's target:

1. **Time-decay is domain-tunable in confidence, and deliberately abolished in health and retention.** That asymmetry is correct and should be stated as doctrine, not re-litigated. Track C's classification below preserves it.
2. **Every *other* lifecycle behavior is hardcoded-universal** — promotion thresholds (`>= 3`, `>= 2`) are string literals inside SQL, quarantine sensitivity is env-only, the compaction trigger is a `coherence.rs` const, the co-access retention window is a 365-day constant, and all HNSW parameters have no config path at all. These are the latent knobs (§3).
3. **Three structural gaps are genuinely absent, not merely unexposed** (§4): there is **no purge**, **no index-heal for the case that actually breaks**, and **no tick self-observability whatsoever**.

A fourth finding cuts across all of it: **the tick's two most expensive learning surfaces are switched off by a single field each.** `phase_explicit_norm` is multiplied by a real 0.05 weight but is always 0.0 because every production caller passes `current_phase: None` (`mcp/tools.rs:794`, `services/index_briefing.rs:188`) — while the phase table is rebuilt **every tick, forever**. Contradiction scoring is computed then discarded at `nli_detection_tick.rs:718`. Unimatrix is paying full tick cost for two capabilities it does not serve.

---

## 1. Per-function disposition

Dispositions: **KEEP** (correct as built) · **REWORK** (right purpose, wrong mechanism) · **ACTIVATE** (built, tested, switched off) · **RETIRE** (remove).

### 1.1 The tick ops (ass-103 numbering)

| # | Function | Current state | Disposition | Reason |
|---|---|---|---|---|
| 1a | `load_maintenance_snapshot` | 2 full `status=0` scans + unbounded `injection_log` join | **REWORK** | Corpus scan is a universal invariant; *five* full reads per tick (I-16) is not. Single-pass snapshot. |
| 1b | Effectiveness classify + `build_report` | classification live in ranking; 4 aggregates discarded on the tick path | **REWORK** | Split `build_report`: classification (serving) from report aggregates (`context_status` only). `status.rs:381` should not compute `by_category`/`by_source`/`calibration`/`unmatched` — live at `:950`, wasted at `:381`. |
| 1c | Quarantined-vector prune | deletes `vector_map` rows for `status=3`; HNSW point survives as stale routing node | **KEEP** (+ surface count) | Correct and ordered deliberately before heal/compact (`status.rs:1041-1043`). Only defect: it *knows* the drift count and discards it (§4.3). |
| 1d | Heal pass / re-embed | heals `embedding_dim=0` and restored-but-absent, **Active only** | **REWORK** | Filter `status = 0` (`status.rs:1098-1120`) means Deprecated/Proposed entries with `embedding_dim=0` are **never** embedded — yet Deprecated entries are readable via `context_get` and carry supersession chains. Serial ONNX + 4-5 round-trips/entry. |
| 1e | EffectivenessState swap | full swap, retain-on-error | **KEEP** | Correct retain-on-error discipline; the model the other caches should follow. |
| 1f | `cleanup_stale_co_access` | `DELETE ... last_updated < now-365d`, `CO_ACCESS_STALENESS_SECONDS` hardcoded (`unimatrix-engine/src/coaccess.rs:20`) | **REWORK** → domain-tunable | Wall-clock retention on a *learning signal*. 365 d is reasoned for SDLC dormancy (`coaccess.rs:16-19`); wrong for a high-velocity ops domain. Compounds I-4: source rows die at 365 d, derived edges live forever. |
| 1g | Confidence refresh | cap 500 + 200 ms budget, N+1, abandonment at `debug!` | **REWORK** | Refreshing confidence is universal; the *ceiling* is invisible (I-14). Budget should be domain-tunable and saturation surfaced. |
| 1h | Empirical prior + spread | 2 more full scans; **writes degenerate state on query error** | **REWORK — urgent** | I-3 is the single clearest integrity defect in the tick: `status.rs:1313-1350` writes `compute_empirical_prior(&[])` into the live `ConfidenceStateHandle` on a transient DB error. Every other cache retains. Adopt retain-on-error uniformly. |
| 1i | Graph compaction | `graph_stale_ratio > 0.10` → re-embeds **entire corpus**, one unbounded ONNX batch | **REWORK** | The only mechanism that evicts stale HNSW points (by omission — `index.rs:375-419` is a full rebuild-and-swap), so it cannot be retired. But uncapped + self-perpetuating on timeout (I-15). Needs a batch cap and an incremental path. |
| 1j | Cycle activity GC | cycle-based, capped | **KEEP** | The RETAIN (#5581) proof. Cycle-based, not wall-clock — exactly right. |
| 1k | audit GC + session sweep | **`gc_audit_log` is a no-op** | **RETIRE the audit-GC call** | See §3.2 — a documented, validated 180-day retention knob wired to `retention.rs:271-278`, which warns and returns `Ok(0)`. The tick then logs `info` "audit_log cleanup, rows_deleted=0" every pass. Dead work + false operator signal + unhonored retention promise. |
| 1l | Auto-quarantine | env-only cadence, no hysteresis, retries forever, **bypasses `pre_quarantine_status`** | **REWORK** | Only automatic demotion anywhere (SL2's "recedes" half). Three defects: I-11 (unbounded retry), I-12 (undamped oscillation), and a new one — `background.rs:1440` calls `update_status`, not `update_entry_status_extended`, so `pre_quarantine_status` stays NULL and `restore_with_audit` falls back to Active (`server.rs:1066-1069`). **An auto-quarantined Deprecated entry silently restores as Active — a status promotion.** |
| 1m | Lifecycle-guard stub | `list_adaptive()` then discard; `TODO(#409)` | **RETIRE** | Pure dead work every tick. Its intended purpose (aged-entry lifecycle) should be built as the purge lifecycle (§4.1), not resurrected here. |
| 1n | Dead-knowledge migration v1 | one-shot, counter-gated | **RETIRE (post-verification)** | ass-103 B-1 confirms correct gating; cost is one PK lookup. It is a completed one-shot migration still registered in a perpetual loop. Retire once completion is confirmed across deployments. |
| 2 | Orphaned-edge compaction | repoints Deprecated-with-successor, then **deletes all non-Active-endpoint edges** | **REWORK — urgent** | #889/#890. Deleting edges to `Proposed` (a live pending state) and `Quarantined` (a *reversible* state) makes a reversible action irreversible. Registry order (`job.rs:267-268`) means the tick auto-quarantines then destroys that entry's graph in the **same pass**. Restore returns the entry, never its edges (`server.rs:1055-1150` has no edge-restoration counterpart). |
| 3 | Co-access promotion | `count >= 3` (`read.rs:1815`), cap 200 | **REWORK** | Promotion without demotion (I-4). Also overwrites S8-authored weights (I-8) — filters `relation_type='CoAccess'` without `source`. |
| 4 | TypedGraph rebuild | O(N+E) full-table every tick, full swap | **REWORK** → **Track B owns the mechanism** | Track C's constraint only: whatever replaces it must preserve retain-on-error and must expose staleness (I-9 — a permanently failing rebuild is indistinguishable from a healthy one). |
| 5 | PhaseFreq rebuild | rebuilt every tick; **consumer passes `None`** | **ACTIVATE** | See §1.2. Highest-value single line in the campaign. |
| 6 | Contradiction scan | **heuristic**, not NLI; every 4th tick; O(N) re-embed of whole corpus | **REWORK** | Widely mis-described (`background.rs:654` says "ONNX inference"; it uses the *embedding* model at `infra/contradiction.rs:176`). Feeds one consumer: `contradiction_density_score` → Lambda. Paying a full-corpus re-embed every 4 ticks for one health scalar. Cadence should at minimum be domain-tunable (`CONTRADICTION_SCAN_INTERVAL_TICKS=4`, `contradiction_cache.rs:27`, is a const). |
| 7 | Extraction | watermark in-memory; fail-open gates, silent | **REWORK** | I-23: embed/HNSW failure → `passed.push(entry); continue` with **no log** — entries admitted with no contradiction check. Fail-open on an integrity gate must be loud. |
| 8 | Graph inference (NLI Path B) | `nli_enabled=false` default; contradiction score **discarded** | **ACTIVATE (gated)** | See §1.3. |
| 9a | S1 tag co-occurrence | `HAVING COUNT(*) >= 3` literal in SQL | **REWORK** → domain-tunable | `graph_enrichment_tick.rs:96`. The single most impactful un-exposed knob for graph density. |
| 9b | S2 vocabulary | `s2_vocabulary` defaults **empty** → no-op | **KEEP as opt-in** | A shipped-disabled edge source is legitimate *if* documented as domain opt-in. O(N²·V) when populated — needs a cost guard before promotion. |
| 9c | S8 search co-retrieval | every 10th tick, flat 0.25 weight | **KEEP** (+ fix I-8) | Cadence already configurable — the one op that models what the others should look like. |

### 1.2 Phase / category machinery — **ACTIVATE, do not retire**

The SCOPE asks whether this is "real future or dead weight." Neither. It is a **finished, tested feature that reaches production and is then zeroed**.

- Rebuilt every tick (`background/jobs.rs:181-200`), consumed in `SearchService::search` (`search.rs:890-921`, `:1307-1323`), applied at `search.rs:239` with a **real non-zero weight** — `default_w_phase_explicit() -> 0.05` (`config.rs:1084-1086`), explicitly *"raised from 0.0 to 0.05 — PhaseFreqTable activates this term."*
- But every production caller passes `current_phase: None`:
  - `mcp/tools.rs:794` — `// col-031: MCP tools.rs — phase not yet threaded from tool params` **(verified first-hand)**
  - `services/index_briefing.rs:188` — `// col-031: briefing does not carry phase context` **(verified first-hand)**
  - `uds/listener.rs:1664`
  - only `eval/runner/replay.rs:114` passes a real phase.
- `search.rs:890` branches `None => None`, so `phase_explicit_norm` is **0.0 for every candidate on every production query**.

The `tools.rs:794` comment is also wrong about the cause: the phase does not come from tool params — it is computed at `tools.rs:743-744` and already used for usage recording (`:828`) and query logging (`:855`). **It is in scope and deliberately dropped for scoring.**

**Disposition: ACTIVATE** — but as a *measured* change, not a flip. PD3 (#5530) is currently supplied by an op whose output reaches nothing. Note the caveat: ASS-052 recorded that a phase-signal feedback loop is precisely what disqualified the ASS-031 GNN for the injection pipeline, so enabling it needs SL-METRIC evidence, not just wiring.

**Genuinely GNN-staged, no consumer** — all honestly labelled, all cheap:

| Item | file:line | Disposition |
|---|---|---|
| `phase_category_weights()` — *"for W3-1 GNN cold-start… NOT called on the search hot path"* | `phase_freq_table.rs:218-231` **(verified)** | **KEEP** (inert, ~40 lines) |
| `phase_affinity_score()` — doc claims a PPR caller that does not exist | `phase_freq_table.rs:~281`; `search.rs:1030` says *do NOT call* | **REWORK doc** — stale contract |
| `FusedScoreInputs` feature-vector contract | `search.rs:56-96` **(verified)** | **KEEP** — the named/stable dimension set |
| `graph_edges.metadata` (always NULL) | `read.rs:1689` | **KEEP** |
| `Prerequisite` edge type (no write path) | `unimatrix-engine/src/graph.rs:128-130` | **KEEP** |
| `resolve_confidence_params` priority-0 learned-weights hook | `config.rs:3643-3657` | **KEEP** |

### 1.3 GNN — **RETIRE the commitment, KEEP the hooks, FIX the pointer**

The SCOPE asks: real future, or dead weight to retire? The corpus has already answered, and the answer is *neither committed nor dead*:

- **SL5 (#5564)**: *"A learned fusion (e.g. GNN) is a research candidate, **NOT a committed mechanism**."*
- **SL-ROLLUP (#5566)**: *"A learned-function method (e.g. GNN) is a ceiling-raiser that Motivates this **ONLY once it proves value against SL-METRIC** — not a committed constituent."*
- **ASS-052**: REJECT on RuVector GNN adoption; ASS-031's GNN *disqualified* for the injection pipeline (selection-bias feedback loop).
- **ASS-038 (#3989)**: densifying the graph 6.2× produced **zero retrieval delta** across 2,376 scenarios — the bottleneck was architecture, not signal.

**Disposition:**
- **RETIRE** any *roadmap commitment* to W3-1/GNN as a planned delivery. It is gated behind SL-METRIC (#5572), which is itself `partial` and not yet trusted for live-corpus interpretation. A GNN cannot be scheduled ahead of its own yardstick.
- **KEEP** the staging hooks above. They are small, inert, honestly documented, and constitute the *named stable feature interface* that any learned-ranking method would need. Retiring them buys nothing and would have to be rebuilt.
- **FIX the pointer — a documentation defect worth correcting now.** ass-029 contains **only** `SCOPE.md`, marked *"Not started"* (`ass-029/SCOPE.md:3`); its six required deliverables do not exist there. They exist in **ass-031**, which records `**Predecessor**: ASS-029 (scope definition, not executed)` and marks Q1–Q7 resolved. Yet `product/WAVE2-ROADMAP.md:228` still tracks ass-029 as *"not yet started."* **The SCOPE's own citation of ass-029 as the GNN staging spike inherits this stale pointer** — the real artifact is ass-031.

### 1.4 NLI / contradiction detection — **ACTIVATE behind its gate; RETIRE the dead config**

Two mechanisms are conflated under one word, and separating them is the whole finding:

| | NLI cross-encoder | Heuristic scan |
|---|---|---|
| Where | `nli_detection_tick.rs` | `infra/contradiction.rs:161` |
| Model | `cross-encoder/nli-*` (313 MB FP32 / 50 MB INT8) | **embedding model** + regex (`contradiction.rs:176`) |
| Status | `nli_enabled=false` default (`config.rs:1023`); Path B never runs OOTB | **live, every 4 ticks** |
| Contradiction output | **computed, then discarded** (`nli_detection_tick.rs:718`) | → `ContradictionScanCache` |
| Consumer | none | `context_status` only (`status.rs:606-612`) → `contradiction_density_score` → Lambda |

**What was removed and why it matters:** ass-035 measured NLI as structurally wrong for *Supports* (a task mismatch, not a tuning problem — worst case 0.990 contradiction on a genuinely supportive pair) but explicitly instructed: *"Do not change the contradiction detection path."* crt-038 (`017ee2a4`) removed the automatic `Contradicts` write anyway — **collateral to a ranking refactor (+0.0031 MRR), never argued on its merits** — and `a870d073` then scrubbed the docs so the system *"presents as cosine-only."* ass-036 found no deployable replacement (Phi-3: 24 s/pair, 70% FP).

**Disposition: ACTIVATE, gated and measured — do not decide on SDLC evidence.** KI-CONTRADICT (#5548) states the rule directly: *"**Domain-conditional**: high value in a research domain (competing claims), null in SDLC. **Do NOT prune on single-domain utility.**"* Contradiction detection found nothing in SDLC because SDLC knowledge evolves linearly — that is a fact about the domain, not the mechanism. This is the textbook domain-conditional policy.

Restoration is genuinely ~one branch (ass-092: effort SMALL, risk LOW), and graceful degradation already holds (Principle 5 — disabled and failed paths are behaviorally identical, `nli_handle.rs:160-183`). But **restoration is not the load-bearing step; the deferred value spike is** (#899 explicitly defers the value verdict). Restore behind `nli_enabled`, then evaluate on the research corpus once it is mature.

**RETIRE now, regardless of that decision — four dead config fields that still validate at boot** (§3.2). And **REWORK** the duplicated scan bodies at `background.rs:681` and `:1088`, which will drift (I-7).

---

## 2. Universal-invariant vs domain-tunable classification

Applying the Framing lens. **Universal** = always on, identical in every domain, not operator-configurable. **Domain-tunable** = correct value differs by domain; belongs in the L1 preset/pack surface. **Structurally universal, numerically tunable** = the behavior must always run, but its threshold is a domain judgement — the largest and most under-served class.

### 2.1 Universal invariants (never domain-conditional)

| Behavior | Why universal |
|---|---|
| Graph integrity under correction (repoint before compact) | SLN3 (#5538). A correction must never orphan referrers in any domain. |
| Hash-chain + append-only audit | Principles 1 & 2. KI-CHAIN/KI-AUDIT — proven, non-negotiable. |
| Current-version resolution (supersession → active terminal) | KI-CURRENCY. "Serve the current version" is domain-independent. |
| Retain-on-error for every Principle-7 cache | Serving stale-but-valid always beats serving degenerate. Currently violated by I-3 and I-5. |
| Index ⟷ store consistency (prune, heal, no orphan vectors) | An entry in the store but absent from the index is a *defect* in every domain. |
| Per-slug isolation | ass-103 B-3. A tenancy boundary, never a policy. |
| Storage bounded by learning utility, not wall-clock age | **RETAIN (#5581)** — deliberately universal. The *aggressiveness* is tunable; the *principle* is not. |
| Lambda as a **structural** metric (graph/embedding/contradiction) | crt-048 ADR-001 (#4199) — activity/freshness dimensions were removed *because* they were domain-specific. Pattern #4189 generalizes this: structural dimensions are domain-neutral; activity signals are not and do not belong in a health metric. |
| The tick completes, and says so | Currently **missing entirely** (§4.3). |

### 2.2 Domain-tunable (policy — belongs in the preset/pack surface)

| Behavior | Today | Target |
|---|---|---|
| **Confidence freshness half-life** | ✅ **already domain-tunable** — presets 8760/720/24 + override | **Model case.** Nothing to build; document as the reference pattern. |
| Confidence weight vector (`w_*`) | ✅ already per-domain via preset/Custom | Keep. |
| Quarantine sensitivity (`auto_quarantine_cycles`) | env-only, default 3, absent from `config.toml` | Move to preset surface. A research domain tolerating competing claims wants a *far* less trigger-happy demotion than an ops domain. |
| Contradiction strictness (`nli_enabled`, thresholds, scan cadence) | disabled default; cadence a const | **The canonical domain-conditional behavior** (KI-CONTRADICT #5548). Per-domain toggle already composes with the vnc-040 per-slug overlay. |
| Promotion thresholds (S1 `>=3`, S2 `>=2`, co-access `>=3`, NLI score floors) | **SQL string literals + consts** | Graph density is a domain judgement: a sparse research corpus and a dense ops corpus want different bars. |
| Edge weights (S8 `0.25`, `*0.1` multipliers) | flat constants | Same. |
| Retention aggressiveness (`activity_detail_retention_cycles`, co-access 365 d) | cycles ✅ tunable; **co-access window hardcoded** | Expose the co-access window; keep it *cycle*-anchored where possible per RETAIN. |
| Compaction trigger (`graph_stale_ratio > 0.10`) | `coherence.rs:16` const | Governs a full-corpus re-embed — the most expensive op in the tick — and is not configurable. |
| Tick cadence | env-only, 900 s | Domains differ by orders of magnitude in write rate. |
| Purge/retention policy for quarantined + deprecated | **does not exist** | §4.1 — must be domain-tunable from birth (compliance domains may forbid deletion outright). |
| HNSW parameters (M, ef_construction, ef_search, dim) | **no config path at all** | Recall/latency/memory tradeoff is domain-specific. |

### 2.3 The doctrinal line — where "age ≠ staleness" applies

The corpus already drew this line correctly, and Track C's recommendation is to **write it down rather than rediscover it**:

- **Age MAY inform confidence** — domain-tunable, via half-life. Already built.
- **Age MUST NOT drive health** — crt-048 deleted the freshness dimension from Lambda because it made a *structurally improving* corpus look like a *degrading* one.
- **Age MUST NOT drive retention** — RETAIN (#5581): pruning is governed by learning-cycle utility; un-reviewed entries are never pruned.

The one live violation: **`cleanup_stale_co_access` deletes learning signal on a wall-clock 365-day cutoff** (`coaccess.rs:20`, hardcoded), while the edges promoted *from* that signal live forever with no demotion owner (I-4). This is the exact failure mode the doctrine exists to prevent — and it is invisible because nothing counts what it deletes.

---

## 3. LATENT-parameter inventory

### 3.1 Exposed and active — the working model

| Parameter | Default | file:line | Notes |
|---|---|---|---|
| `profile.preset` | `Collaborative` | `config.rs:165`; resolved `:3719-3768` | **The lifecycle-policy surface that already exists.** |
| `knowledge.freshness_half_life_hours` | `None` → 8760.0 | field `config.rs:408`; validated `:3458-3475`; applied `:3663-3690`; consumed `confidence.rs:208` | Documented `config.toml:63-68`, **commented out by default**. Boot-validated (DA6). |
| `confidence.weights.*` | .16/.16/.18/.12/.14/.16 | `confidence.rs:20-30` | Requires `preset="custom"`. Per-slug overlayable. |
| `retention.activity_detail_retention_cycles` | 50 | `config.rs:1876` | Cycle-based — RETAIN-compliant. |
| `retention.max_cycles_per_tick` | 10 | `config.rs:1882` | |
| `inference.s8_batch_interval_ticks` | 10 | `config.rs:1231` | **The only configurable cadence.** The model other ops should follow. |
| `inference.heal_pass_batch_size` | 20 | `config.rs:1243` | |
| `inference.max_co_access_promotion_per_tick` | 200 | `config.rs:1175` | |
| `inference.max_s1_edges_per_tick` / `max_s2_edges_per_tick` | 200 / 200 | `config.rs:1223`, `:1227` | |
| `inference.s2_vocabulary` | **`vec![]`** | `config.rs:1219` | Empty ⇒ S2 is a **no-op out of the box**. |
| `inference.nli_enabled` | **`false`** | `config.rs:1023` | Per-slug overlayable — the per-domain toggle already exists. |
| `UNIMATRIX_TICK_INTERVAL_SECS` | 900 | `background.rs:83`, read `:108` | **env only**, not in `config.toml`. |
| `UNIMATRIX_AUTO_QUARANTINE_CYCLES` | 3 | `background.rs:179`, applied `:1414` | **env only**, not in `config.toml`. |

### 3.2 Exposed but INERT — validated config that controls nothing

The sharpest category: an operator can set these, boot validation *accepts or even cross-checks* them, and nothing happens.

| Parameter | file:line | Why inert |
|---|---|---|
| **`retention.audit_log_retention_days = 180`** | field `config.rs:1781`; validated `:1955`; called `status.rs:1582` | **Verified first-hand.** Calls `retention.rs:271-278`, which emits a `warn!` — *"gc_audit_log is a no-op: audit_log is append-only (vnc-014)"* — and returns `Ok(0)`. The tick then logs `info` **"cycle GC: audit_log cleanup, rows_deleted=0"** every pass. Simultaneously: dead work, a **false operator signal** (against N4 #5576), and an **unhonored compliance-facing retention promise** — `audit_log` grows forever (vnc-014 ADR-005 accepted this as a documented limitation, but the *knob* was never withdrawn). |
| `inference.nli_entailment_threshold` (0.6) | `config.rs:564`, default `:1031` | Zero readers outside `config.rs`. Documented in shipped TOML. |
| `inference.nli_contradiction_threshold` (0.6) | `config.rs:571`, default `:1035` | Zero readers. |
| `inference.max_contradicts_per_tick` (10) | `config.rs:580`, default `:1039` | Zero readers; doc-comment cites `run_post_store_nli`, deleted in crt-038. |
| `inference.nli_auto_quarantine_threshold` (0.85) | `config.rs:589`, default `:1043` | Zero readers — **yet `validate()` enforces a cross-field invariant against another dead field** (`config.rs:1370`). Dead config that can fail startup. |
| **`[graph_penalty]` — entire section** | `config.rs:231-249` | Parsed, merged, validated — but production `ServiceLayer::new` hardcodes `GraphPenaltyParams::default()` at `services/mod.rs:430` (comment `:429`: *"swept levers are eval-only; production never tunes"*). Live **only** in `eval/profile/layer.rs:391`. Not in shipped `config.toml`. |
| `ResourceClass::{Io,Rayon}` | `job.rs:204-213` **(verified)** | Declared, implemented per-job, read by nothing. Honestly documented as a forward hook. |
| `w_nli = 0.00` | `config.rs:~1055` | NLI term inert in ranking by design (crt-038). |
| `w_phase_explicit = 0.05` | `config.rs:1084` | Non-zero weight × always-zero input (§1.2). |

### 3.3 HARDCODED — the true latent knobs

**Promotion / graph density** (highest leverage):

| Knob | Value | file:line |
|---|---|---|
| S1 tag co-occurrence threshold | `HAVING COUNT(*) >= 3` *(SQL literal)* | `graph_enrichment_tick.rs:96` |
| S1 edge weight | `min(shared_tags * 0.1, 1.0)` | `graph_enrichment_tick.rs:121` |
| S2 shared-term threshold | `>= 2` *(SQL literal)* | `graph_enrichment_tick.rs:215` |
| S2 edge weight | `min(shared_terms * 0.1, 1.0)` | `graph_enrichment_tick.rs:242` |
| S8 CoAccess weight | `0.25` | `graph_enrichment_tick.rs:453, 469` |
| co-access promotion min count | `CO_ACCESS_GRAPH_MIN_COUNT = 3` | `unimatrix-store/src/read.rs:1815` |
| co-access weight noise floor | `0.1` — *"Not operator-configurable"* | `co_access_promotion_tick.rs:35` |
| Informs budget | `MAX_INFORMS_PER_TICK = 25` | `nli_detection_tick.rs:65` |
| Cosine-Supports budget | `MAX_COSINE_SUPPORTS_PER_TICK = 50` — carries an explicit `TODO: Config-promote` | `nli_detection_tick.rs:79` |

**Lifecycle / retention / health:**

| Knob | Value | file:line |
|---|---|---|
| **co-access retention window** | `CO_ACCESS_STALENESS_SECONDS = 365 d` | `unimatrix-engine/src/coaccess.rs:20` **(verified)** |
| **compaction trigger** | `DEFAULT_STALE_RATIO_TRIGGER = 0.10` | `coherence.rs:16` **(verified)** — governs a full-corpus re-embed |
| confidence staleness | `DEFAULT_STALENESS_THRESHOLD_SECS = 24 h` | `coherence.rs:13` |
| confidence refresh cap | `MAX_CONFIDENCE_REFRESH_BATCH = 500` | `coherence.rs:22` |
| Lambda alarm threshold | `DEFAULT_LAMBDA_THRESHOLD = 0.8` | `coherence.rs:19` |
| Lambda dimension weights | `0.46 / 0.23 / 0.31` | `coherence.rs:34-38` |
| contradiction scan cadence | `CONTRADICTION_SCAN_INTERVAL_TICKS = 4` | `contradiction_cache.rs:27` |
| contradiction heuristics | `SIMILARITY_THRESHOLD 0.85`, `NEIGHBORS_PER_ENTRY 10`, `NEGATION 0.6 / DIRECTIVE 0.3 / SENTIMENT 0.1` | `infra/contradiction.rs:21, 27, 33-39` |
| per-op timeout | `TICK_TIMEOUT = 120 s` (**per-op, not per-pass**) | `background.rs:418` |
| effectiveness constants | `INEFFECTIVE_MIN_INJECTIONS 3`; outcome weights `1.0/0.5/0.0`; `UTILITY_BOOST/PENALTY 0.05` | `effectiveness/mod.rs:22, 25-31, 38-46` |
| cold-start prior | `COLD_START_ALPHA/BETA = 3.0` — hardcoded **even for the Custom preset** (`config.rs:3707-3708`) | `confidence.rs:44, 51` |

**HNSW / embedding — no config path whatsoever:**

`dimension 384`, `max_nb_connection 16`, `ef_construction 200`, `max_elements 10_000`, `max_layer 16`, `default_ef_search 32` — all `unimatrix-vector/src/config.rs:25-30`, always constructed via `VectorConfig::default()` at every call site (`main.rs:921, 1704`; `server.rs:1336, 1383, 1595`; `http_provision.rs:195`). Query-time `EF_SEARCH = 32` is duplicated in three places (`search.rs:42`, `store_ops.rs:28`, `contradiction.rs:18`). `AdaptConfig` and `LearnConfig` are likewise always `::default()` (`http_provision.rs:235` notes *"#785 would thread it"*).

### 3.4 The domain-pack surface cannot carry any of this

`DomainPack` has exactly four fields — `source_domain`, `event_types`, `categories`, `rules` (`unimatrix-observe/src/domain/mod.rs:28-39`); the TOML shape adds only `rule_file` (`config.rs:147-158`). **There is no numeric or threshold field anywhere in the pack schema.**

Worse, **the tick never consults the registry at all**. `background.rs:461-463` (and `jobs.rs:52`, `background.rs:1637`) construct `DomainPackRegistry::with_builtin_claude_code()` and throw it away, with the comment stating the tick uses `load_maintenance_snapshot` so *"the observation registry is not consulted."* An operator's configured packs never reach the tick path.

**Direction:** the domain-tunable surface for background processing is **`Preset` + `[inference]` + `[retention]`**, *not* the domain pack. Extending `Preset` from a confidence-weight vector into a full **lifecycle-policy preset** is the smallest change that satisfies the Framing lens — it reuses a mechanism that is already shipped, per-slug overlayable (vnc-040), and boot-validated (DA6/PL-2).

---

## 4. Structural gaps

### 4.1 GAP — no purge / hard-delete lifecycle

**There is no hard delete of an entry in any production code path.** Exhaustive search found exactly two `DELETE FROM entries` sites:

- `unimatrix-store/src/write.rs:228-263` — `Store::delete()`. **Zero production callers**; referenced only from `tests/sqlite_parity.rs` and `write_tag_tests.rs`.
- `import/mod.rs:382-399` — `drop_all_data()`, a whole-DB wipe during import reset.

`VACUUM` (`db.rs:671`) is page-level SQLite compaction at shutdown — it reclaims free pages, not logical rows. `retention.rs` purge machinery targets **cycles, sessions, observations only** — never entries.

**Supporting absences:**
- No `deprecated_at`, `expires_at`, `ttl`, or tombstone column on `entries` (`migration.rs:1685-1712`). `updated_at` is overwritten by any later mutation, so **the data needed to answer "how long has this been deprecated?" does not exist.**
- `last_accessed_at` and `access_count` **are written** (`write_ext.rs:88, 90, 158`) but **never read for any sweep** — grep for `last_accessed_at <`, `ORDER BY last_accessed`, `WHERE last_accessed` returns zero hits. Their only consumer is relevance scoring.

**Answering the SCOPE's lifecycle question:** auto-quarantine of *aged/unaccessed DEPRECATED* entries is **not wanted as specified** — "aged" is the wrong predicate, per RETAIN (#5581) and crt-048. The right predicate is **learning utility**: deprecated, superseded, zero injections, zero co-access, not referenced by any active edge, and outside the retained cycle window. That is computable from data that already exists.

**Target shape (directional):**
1. **Add `deprecated_at`** (or a status-transition timestamp) — currently impossible to reason about lifecycle without it.
2. **Tiered lifecycle**: `Quarantined` → (review window) → `Archived` (removed from all serving paths, retained for audit) → (policy-gated) → purge.
3. **Purge is domain-tunable and default-OFF.** Compliance domains may forbid deletion entirely; the append-only audit trail must survive regardless (Principle 2 / KI-AUDIT).
4. **Never purge un-reviewed entries** — RETAIN's proven invariant.
5. **Fix the reversibility inversion first (#889/#890).** Today quarantine is *reversible* for status and vector but *irreversible* for edges. Building purge on top of a lifecycle whose reversible state already destroys data would cement the defect.

Interim: with 2504 quarantined entries and no purge, `generate_recommendations` (`coherence.rs:150-154`) emits *"N entries quarantined -- review for resolution"* — advice with **no bulk-review surface and no purge path to act on**, firing forever below the Lambda threshold.

### 4.2 GAP — no HNSW index heal

Distinguish two things the SCOPE conflates:

**(a) The "heal pass" that exists** (`status.rs:1082-1217`) repairs `embedding_dim=0` and restored-but-absent entries. Well-built: ordered deliberately after the prune (`:1041-1043`), DB write last as the confirmation step so a crash re-heals idempotently (`:1090-1092`). **Gap:** both sub-cases filter `status = 0`, so Deprecated/Proposed entries are never embedded (§1.1 op 1d).

**(b) Index-health *repair* — genuinely absent.**
- **HNSW has no deletion API** (`hnsw_rs 0.3`). `remove_entry` (`index.rs:433-438`) erases only the bidirectional `IdMap`; the vector **stays in the graph**, consuming memory and participating in routing/distance — a soft tombstone. Same on every re-embed (`index.rs:163-165`).
- The **only** eviction is `VectorIndex::compact` (`index.rs:375-419`), a full rebuild-and-swap that preserves only what the caller passes in — it evicts **by omission**. There is no incremental repair.
- **No drift detection of any kind.** No query compares `vector_map` against HNSW contents; no detection of `vector_map` rows whose `data_id` has no HNSW point (the heal-pass crash window, which `status.rs:1157-1183` `continue`s past); no detection of HNSW points with no `vector_map` row (manufactured by every `remove_entry`); no checksum or generation stamp binding the on-disk graph to the DB. `load()` (`persistence.rs:141-208`) validates **only dimension and file existence** — a graph stale relative to `vector_map` is accepted silently.
- **`graph_stale_ratio` cannot see any of this.** It is `point_count.saturating_sub(id_map.active_count())` (`index.rs:334-338`) — pure in-memory arithmetic that never consults SQLite. And because of `saturating_sub`, the **inverse fault** — IdMap entries with no backing HNSW point — floors to a healthy-looking **`0.0`**. **Verified first-hand.** It is also not persisted: after restart the IdMap is rebuilt from `vector_map` and staleness history is lost.

**Target:** a real reconciliation pass — a periodic three-way check across `entries(status=0)` ⟷ `vector_map` ⟷ HNSW `IdMap` that *counts and reports* divergence in both directions, repairs the repairable, and surfaces the rest. Both existing passes already enumerate the drifted rows before repairing them (`status.rs:1045-1081`, `:1095-1217`) — the counts are in hand and thrown away. Also worth pairing with `#828` (versioned atomic graph+data flip), which removes the crash-window that manufactures drift.

### 4.3 GAP — no corpus-health monitoring of the engine itself

`context_status` is a **genuinely rich corpus surface — 49 fields** (`mcp/response/status.rs:11-144`): active/deprecated/proposed/quarantined counts, category and topic distributions, Lambda plus its three dimensions, `graph_stale_ratio`, `isolated_entry_count`, `unembedded_active_count`, connectivity rate, co-access stats, curation health.

**It is bolted onto a tick with essentially zero self-observability.**

- **No metrics surface at all.** No Prometheus, no `/metrics`, no exporter, no gauges, no timers. The router has exactly three arms (`http/router.rs:194-211`).
- **`/health` checks nothing** — compile-time constants, explicitly *"no I/O, no database access"* (`http/health.rs:18-23`). It returns 200 if the process can format a string. The CLI/Docker probe (`Dockerfile:149-150`) only checks that a UDS socket accepts a connection.
- **No per-pass log and no duration on the production path.** `run_per_slug_tick_pass` (`tick_loop.rs:45-72`) logs **only job errors** — no "starting", no "complete". Duration is computed in exactly one place (`background.rs:775-776`) — on the **stdio path the daemon never runs**. This is I-7 surfacing as an observability gap: the better-instrumented copy is the one nobody executes. A pass where all 9 jobs succeed produces **2 log lines per slug**; a pass where nothing fires produces **zero**.
- **The three tick-health fields are dishonest by omission.** `last_maintenance_run` is written **only on success** (`jobs.rs:85-88`) and left stale on failure/timeout (`:91-92`); `next_maintenance_scheduled` advances **unconditionally** (`tick_loop.rs:68-70`). **A tick failing for a week reports a next-run 15 minutes out and a stale last-run the client must diff itself.** No failure count, no consecutive-failure counter, no last-error, no staleness flag, no duration — `TickMetadata` (`background.rs:131-156`) has no field for any of them.
- **No persisted tick ledger.** None of the 27 tables records tick outcomes. The only persisted signal is a single negative-path audit event (`OP_TICK_SKIPPED`, `background.rs:76`, emitted `:1509-1534`) — reached only when `compute_report()` returns `Err`, and therefore **never** in the I-10 silent-degradation case where the snapshot returns `Ok(None)`. **And nothing can read it back**: no MCP tool, no HTTP route, no CLI reads `audit_log`. An operator must open SQLite by hand.

So: a tick that **errors** leaves one row; a tick that **silently degrades**, **succeeds**, or **never fires** leaves nothing.

**On LAMBDA-HONEST:** it exists as a capability node (**#5555**, `delivery:partial`) — *"Health/curation/review metrics measure reality and fail-loud on empty (never a fake 0)"* — but **has no written artifact in the repo**; the only in-tree occurrence is the forward reference in this campaign's own SCOPE. Its codified kin are `unimatrix-observe/src/fail_loud_guard.rs:1-11` and crt-048 ADR-001. The doctrine currently governs *corpus* metrics; **the gap is that it has never been applied to the engine's own liveness** — `next_maintenance_scheduled` on a dead tick is precisely the "fake 0" LAMBDA-HONEST forbids, one level up.

**On the missing tick-health NFR:** confirmed missing, and **structurally homeless** — there is no global NFR registry; NFR identifiers are per-feature and renumbered per feature. The nearest capability node is N4 (#5576, *"no false-alarm signals"*) — the *inverse* concern. This compounds ass-103's headline: the tick is the sole supply line for ≥6 capability nodes, two marked `delivery:proven` (RETAIN #5581, C5 #5550), and **the tick appears nowhere in the capability map as an entity**.

**Target signals, and where:**

| Signal | Source | Surface |
|---|---|---|
| Tick last-success **age** + `is_stale` flag | `TickMetadata` (add fields) | `context_status` + `/health` |
| Consecutive-failure count, last error, per-job outcome | `TickMetadata` | `context_status` |
| Per-pass and per-job **duration** | `tick_start` already in scope at `jobs.rs:66` | logs + `context_status` |
| Orphaned-edge delete count | **already computed** at `background.rs:820`, discarded at `:827` | `context_status` — cheapest win in the campaign |
| Repointed-edge count | `repoint_deprecated_target_edges`, result dropped | `context_status` |
| Index⟷DB drift, both directions | prune + heal already enumerate it | `context_status` |
| Confidence-refresh saturation | computed, logged at `debug!` (`status.rs:1275-1281`) | raise + surface |
| Extraction fail-open admissions | **no log at any level** (`background.rs:1813-1824`) | raise to `warn` + count |
| Graph serving stale (`use_fallback`) | in-process flag only (`background.rs:987-1005`) | `context_status` |

`/health` should gain a **readiness** dimension distinct from liveness: process-up (today) vs. corpus-served-from-a-currently-maintained-index. Purely additive.

---

## Unanswered Questions

- **Does activating `current_phase` improve retrieval, or feed a bias loop?** The wiring is one line and the weight is live, but ASS-052 recorded the phase-signal feedback loop as the reason ASS-031's GNN was disqualified for injection. *Requires SL-METRIC (#5572) — which is itself not yet trusted for live-corpus interpretation. Blocked on #803.*
- **Is contradiction detection valuable in a research domain?** #899 defers the value verdict by design; ass-035's defense of the contradiction path was never measured, and its own P04 result argues against it. *Needs the deferred value spike against a mature research corpus — not decidable from code.*
- **What is the real cost of the heuristic contradiction scan?** It re-embeds every active entry every 4 ticks for one Lambda scalar. *Cost is a shape from code; needs measurement.*
- **How much index⟷DB drift actually exists?** The mechanism is established; the magnitude is unmeasurable today precisely because nothing counts it. *Needs the reconciliation pass to exist first — chicken-and-egg.*
- **How many of the 2504 quarantined entries are auto- vs. human-quarantined?** Bears directly on purge policy and on the `pre_quarantine_status` defect's blast radius. *Requires live corpus query, not in-repo.*
- **Should `Preset` be extended, or should a new lifecycle-policy section be added?** Track C recommends extending `Preset` (reuses shipped, validated, per-slug-overlayable machinery), but the versioning implications land on **PL-2** (#5700, the L1 contract-version capability, `delivery:missing`). *Mechanism choice is explicitly out of scope per SCOPE Bounds.*
- **Does retiring op 1n (dead-knowledge migration) require a deployment-wide completion check?** *Requires operational data not in-repo.*

---

## Out-of-Scope Discoveries

- **`gc_audit_log` is a triple defect worth its own issue** — inert knob, per-tick dead work, false `info` "cleanup" line, and a compliance-facing retention promise the engine does not honor. vnc-014 ADR-005 accepted unbounded audit growth as a documented limitation but never withdrew the knob. Touches N4 (#5576) and KI-AUDIT.
- **Auto-quarantine bypasses `pre_quarantine_status`** (`background.rs:1440` uses `update_status`, not `update_entry_status_extended`) → auto-quarantined Deprecated/Proposed entries **silently restore as Active**. A status *promotion* via a maintenance path. Not in ass-103's register; appears to be a new finding. **Likely warrants a bug issue.**
- **The roadmap's GNN pointer is stale** — `WAVE2-ROADMAP.md:228` tracks ass-029 as "not yet started"; ass-031 executed it and holds all six deliverables. The SCOPE inherited the stale pointer. Cheap doc fix.
- **Four dead NLI config fields still pass boot validation, including a cross-field invariant between two of them** (`config.rs:1370`). Dead config that can *fail startup* is worse than dead config. Related: `[graph_penalty]` is a fully-built section production never reads (`services/mod.rs:430`).
- **The three duplicated `EF_SEARCH = 32` constants** (`search.rs:42`, `store_ops.rs:28`, `contradiction.rs:18`) will drift — same class as I-7.
- **Documentation/code divergence on NLI**: `a870d073` scrubbed docs so the system *"presents as cosine-only"* while ~95% of the NLI substrate remains intact and its config fields still validate. The operator-facing surface denies a capability the code still carries.
- **Retain-on-error has no stated convention** — ass-103 flagged this; Track C confirms it is the shared root beneath I-3, I-5, and I-9, and that op 1e is the correct model. A one-line doctrine would prevent recurrence.
- **`phase_affinity_score()`'s doc claims a PPR caller that does not exist**, while `search.rs:1030` explicitly says *do NOT call it*. Two docs in direct contradiction.
- **Cross-track note (Track B):** whatever replaces the full-swap TypedGraph rebuild must preserve retain-on-error **and** expose staleness — I-9 means a permanently failing rebuild is today indistinguishable from a healthy one. Recorded, not pursued.

---

## Recommendations Summary

**Framing / doctrine**
1. **Extend `Preset` from a confidence-weight vector into a full lifecycle-policy preset.** The domain-conditional mechanism is already built, shipped, boot-validated, and per-slug overlayable — reuse it rather than inventing a new surface. The domain *pack* cannot carry these knobs and the tick never reads it.
2. **Write down the age doctrine as three lines**: age MAY inform confidence (tunable); age MUST NOT drive health (crt-048); age MUST NOT drive retention (RETAIN). It has been rediscovered three times.
3. **Adopt retain-on-error as a stated universal invariant** for all five Principle-7 caches, with op 1e as the model. Fixes the root beneath I-3, I-5, I-9.

**Activate (built, tested, switched off)**
4. **Thread `current_phase` at `mcp/tools.rs:794`** — the value is already in scope at `:743`. Gate on SL-METRIC evidence, given the ASS-052 bias-loop caveat.
5. **Restore the `Contradicts` write behind `nli_enabled`** (~one branch at `nli_detection_tick.rs:718`), then run the deferred value spike on the research corpus. Do **not** decide on SDLC evidence — KI-CONTRADICT is explicitly domain-conditional.

**Retire**
6. **Retire the GNN *commitment*, keep the hooks.** SL5/SL-ROLLUP already record it as non-committed and SL-METRIC-gated; the hooks are small, inert, and correctly documented. Fix the ass-029 → ass-031 roadmap pointer.
7. **Retire the four dead NLI config fields, the `gc_audit_log` call, op 1m (`TODO(#409)` stub), and op 1n (post-verification).**

**Expose (latent → domain-tunable)**
8. **Promote the hardcoded promotion thresholds and edge weights** — S1 `>=3`, S2 `>=2`, co-access `>=3`, S8 `0.25`, the `*0.1` multipliers. Highest-leverage un-exposed knobs for graph density.
9. **Promote `auto_quarantine_cycles`, tick interval, `graph_stale_ratio` trigger, contradiction cadence, and the 365-day co-access window** out of env/consts into the preset surface. Give HNSW parameters a config path.

**Structural gaps**
10. **Purge lifecycle**: add a status-transition timestamp (`deprecated_at`), define `Quarantined → Archived → purge` gated on **learning utility, never age**, default-OFF and domain-tunable, never purging un-reviewed entries, always preserving the audit trail. **Fix #889/#890 first** — do not build purge atop a lifecycle whose reversible state already destroys edges irreversibly.
11. **Index heal**: build a three-way `entries ⟷ vector_map ⟷ IdMap` reconciliation that counts drift in **both** directions (today's `graph_stale_ratio` floors the inverse fault to a healthy `0.0`), repairs what it can, and surfaces the rest. Extend the heal pass past `status = 0`. Cap the unbounded compaction re-embed.
12. **Monitoring**: add tick liveness/duration/failure fields to `TickMetadata`, a per-pass summary log on the *production* path, and an audit read surface. **Apply LAMBDA-HONEST to the engine's own liveness** — `next_maintenance_scheduled` advancing on a dead tick is exactly the fake-0 it forbids. **Author the tick-health NFR** and register the tick as an entity in the capability map: it is the sole supply line for two `delivery:proven` nodes, and nothing records that dependency.
13. **Cheapest win, do it first**: surface the orphaned-edge delete count — already computed at `background.rs:820`, discarded one line later.

---

## Confidence statement

**DIRECTIONAL**, consistent with the SCOPE's stated requirement (envisioning only, no build). Every disposition and classification is reasoned from code read during this spike and cited to `file:line`, or from Unimatrix entries cited by ID. No PoC was built, no test was run, no measurement was taken.

- **High confidence** (direct reading, unambiguous, several verified first-hand rather than via sub-agent): the `Preset`/half-life domain-tunable surface; `current_phase: None` at both production call sites (`tools.rs:794`, `index_briefing.rs:188`); `gc_audit_log` being a no-op behind a validated knob; the absence of any production entry hard-delete; the absence of any metrics surface, per-pass log, or tick ledger; `graph_stale_ratio`'s `saturating_sub` masking the inverse fault; `ResourceClass` being unread; the hardcoded threshold inventory in §3.3; the GNN non-commitment (quoted verbatim from #5564/#5566); KI-CONTRADICT's domain-conditional instruction (#5548).
- **Medium confidence** (mechanism established, consequence reasoned not observed): the `pre_quarantine_status` bypass and its restore-as-Active consequence — the two call paths were read but the end-to-end sequence was not executed; the claim that no drift-detection query exists anywhere (established by exhaustive search, i.e. proving a negative); the ass-029/ass-031 pointer analysis.
- **Cost and volume figures are shapes, not measurements**, inherited from ass-103's methodology. The "2504 quarantined entries" figure is taken from the SCOPE, not independently verified.
- **One inter-source conflict was found and resolved by first-hand reading**: a sub-agent reported that no GC reads `co_access.last_updated`, contradicting ass-103's op 1f. ass-103 is correct — `CO_ACCESS_STALENESS_SECONDS = 365 d` at `unimatrix-engine/src/coaccess.rs:20`, applied at `status.rs:1220-1221`, executing the DELETE at `write_ext.rs:330-337`. The findings above use the verified version.

Anything here that warrants action should be re-established at `validated` or `empirical` confidence before delivery. Two items in particular are asserted from code semantics alone and would be cheap to confirm by reproduction: the auto-quarantine restore-as-Active promotion, and the compaction/auto-quarantine same-pass edge destruction.

---

**Note on delivery**: this environment blocks subagent report-file writes, so the intended output file `/workspaces/unimatrix/product/research/ass-104/FINDINGS-TRACK-C.md` was not created — the content above is the complete deliverable, verbatim, for the campaign SM to route. No Unimatrix writes were made at any point; only `context_briefing`, `context_search`, and `context_get` reads.