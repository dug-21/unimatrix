# SCOPE: ass-104 — Background-processing target state (research CAMPAIGN)

**Goal(s) advanced**: `goal:self-learning` + `goal:integrity` + `goal:domain-agnostic` (cross-goal)
**Type**: envisioning / design-research **CAMPAIGN** — multi-track, read-only, no build. Coordinated by **`uni-research-sm`**.
**Purpose**: Envision the **TARGET STATE** of Unimatrix's background processing — grounded in every option we have ever considered, plus forward architecture and external best-practice — so the ass-103 fixes/enhancements execute *targeted against a picture*, not ad hoc.

---

## Framing

- **ass-103 established what the tick IS** (inventory + issue triage). This campaign establishes **what it SHOULD BE**.
- **Core lens (carried from uni-zero, 2026-07-19):** every background behavior is either a **UNIVERSAL INVARIANT** (always on — graph integrity, currency, the write path itself) or a **DOMAIN-CONDITIONAL POLICY** (tunable per domain pack — time-based decay, retention aggressiveness, promotion/demotion thresholds, quarantine sensitivity, contradiction strictness). **Many knobs already exist but are LATENT.** The canonical example is **time-based relevance decay**: right for a news/ops/compliance domain, *wrong* for SDLC — entry #597 surfacing in a bugfix months later is fully valuable; **age ≠ staleness**. The capability we want is *degradation tunable within a domain pack*, not a hardcoded universal.
- **Envisioning, not implementation.** The campaign produces the target picture + candidate capabilities; mechanism design happens later in delivery. Research proposes; **uni-zero + human ratify** into goals/capabilities.

---

## Tracks (parallel; `uni-research-sm` coordinates → synthesis)

### Track A — Internal retrospective ("every option we've ever considered")
**Question**: Across all prior research, what has Unimatrix considered for graph / learning / knowledge-lifecycle / integrity background processing, and what was the disposition of each?
**Explore**: mine these dirs — **ass-015, 020, 022, 025, 029, 030, 031, 032, 034, 035, 036, 037, 038, 039, 040, 041, 042, 047, 050, 051, 052, 055, 057, 061, 066, 074, 076, 079, 088, 090, 091, 092, 093** — plus **ass-103**. Note the alt-format dirs (**ass-015/022/025/030** have no standard heading; **ass-040 = ROADMAP.md**) — open them directly. For each relevant idea/option, record: the idea, its spike, and its disposition — **ADOPTED / DEFERRED / REJECTED / LATENT** — with the stated reason.
**Output**: a single **OPTION LEDGER** — every background/graph/learning/lifecycle idea ever considered, tagged by disposition + reason, cross-referenced to its spike. The "what we already know" foundation for the synthesis.

### Track B — Graph acceleration: event-driven vs tick-rebuild
**Question**: Can graph updates move from full **tick-rebuild** to **event-driven / incremental near-real-time** — each edit updating its slice of the graph as it happens?
**Explore**: the current path (TypedGraphState full-swap every tick — ass-103 op 4); **ass-088** (edge-consistency: synchronous-redirect vs read-time resolution vs background-tick convergence); **#904** (accelerate typed-graph currency below the tick window); **#870**. Weigh options against consistency, cost, complexity, and the Principle-7 hot-path contract. Not a build.
**Output**: architectural **options for graph currency** (event/incremental vs tick vs hybrid) with a recommendation *direction* and the invariants each must preserve.

### Track C — Integrity & performance target
**Question**: What must background processing do to keep the corpus healthy and performant — and which behaviors are universal vs domain-tunable?
**Explore the concrete gaps/questions**:
- **Lifecycle** — auto-quarantine of **aged / unaccessed DEPRECATED** entries: wanted? domain-tunable?
- **Structural REMOVE** — there is **no hard-delete / purge** of quarantined items today (the 2504 quarantined entries sit forever). What should the removal/purge lifecycle be?
- **HNSW healing** — there is **no index-heal mechanism** today. What should corpus/index health repair be?
- **GNN** — the phase/category machinery is staged for a *future* GNN (**ass-029**): real future, or dead weight to retire?
- **NLI** — restore contradiction detection / relationship NLI (**ass-092 / 035 / 036**): add back? value vs cost?
- **Monitoring** — a healthy corpus needs the engine *watching* it: what health signals, surfaced where? (ties to **LAMBDA-HONEST** + the missing tick-health nfr).
Classify each behavior on the **universal-invariant vs domain-tunable** axis; inventory the **LATENT parameters** that already exist but aren't exposed/active.
**Output**: the integrity/performance **target** — per-function keep/rework/activate/retire disposition + the universal/domain-tunable classification + latent-param inventory + the named **structural gaps** (purge, HNSW heal, monitoring).

### Track D — External research (world-class bar) — full parallel stream
**Question**: What do best-in-class knowledge-management / knowledge-graph / agent-memory / retrieval platforms do for knowledge **lifecycle, integrity maintenance, relevance decay, and background health** — that a world-class Unimatrix should have?
**Explore**: ecosystem + literature via **`uni-external-researcher`** (never accesses Unimatrix; brings outside vision). Runs in parallel; cross-references — but does **not** redo — prior internal-external work Track A surfaces (**ass-032** best-possible surfacing pipeline; **ass-052** RuVector). Report **net-new** features/patterns, not a re-derivation.
**Output**: an external **FEATURE / PATTERN map** — what world-class KM/lifecycle/integrity features exist, which Unimatrix has / lacks / should adopt, with rationale. The "what should we have" bar.

### Synthesis (after tracks converge)
Combine A (considered) + B (graph architecture) + C (integrity target) + D (external bar) into the **TARGET-STATE vision**:
- **Function taxonomy** — collapse ass-103's 24 ops into coherent purposes (health-monitoring, graph-healing, learning-signal maintenance, relevance maintenance, storage governance).
- **Universal-invariant vs domain-tunable classification**, with the **latent-param inventory** (what to *activate/expose* vs *build new*).
- **keep / rework / activate / retire** disposition per function — including GNN, NLI, phase/category — kept or retired **with reason**.
- **Event-vs-tick** graph-currency recommendation (from Track B).
- The **structural-gap set** (purge lifecycle, HNSW heal, corpus-health monitoring).
- **Gap vs external best-practice** — what a world-class product should add (from Track D).
- **CANDIDATE CAPABILITIES** (input for uni-zero to author) + a **targeted execution roadmap** that re-frames the ass-103 fixes against the target.

---

## Bounds / out of scope
- **Envisioning, not implementation**: target state + options + candidate capabilities. **No** mechanism build, **no** ADR amendment, **no** capability authoring (uni-zero does that on ratification).
- Does **not** re-inventory the tick (ass-103 did) — builds on it.
- Does **not** settle mechanism choices (event-vs-tick, NLI-back, GNN-keep/retire, purge design) — surfaces options + a recommendation *direction* for uni-zero + human to decide.

## Constraints / prior art
- **Read-only, no build**; code + docs cited to `file:line` / spike.
- **Option corpus**: the 34 spikes listed in Track A + ass-103.
- **Open issues in-space**: #904, #870, #889, #890, #745, #744, #617, #886, #828, #625, #604, #899, #753, #804.
- **Grounding**: Architectural Principle 7 (in-memory hot path rebuilt by tick); the domain-pack / L1 config surface (`goal:platform` PL-1/PL-2); the universal-vs-domain-tunable frame.

## Execution
**Campaign via `uni-research-sm`.** Tracks A/B/C/D run in parallel; the SM routes cross-track findings and runs the synthesis after convergence. Track D (`uni-external-researcher`) is a full parallel stream — cross-referencing Track A's coverage, not gated on it.
