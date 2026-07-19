# SCOPE: ass-103 — Background maintenance-engine (tick) inventory + potential-issue triage

**Goal(s) advanced**: `goal:self-learning` + `goal:integrity` (cross-goal)
**Type**: inventory / assessment (read-only; no build)
**Purpose**: give the human a **triage read on urgent areas of focus** in the background tick — what runs, what it serves, and where potential issues lie. **IDENTIFICATION ONLY — no recommendations, no fixes, no re-architecture.**

---

## The question

The background tick has become a cross-cutting maintenance engine spanning **knowledge integrity** (graph healing) and **self-learning** (the self-improving loop), plus proactive-delivery phase signal and per-slug maintenance. It has evolved for months without a holistic review and appears **nowhere in the goal/capability map** as a thing in its own right.

**Primary question**: What does the tick actually run today — every phase/operation, cadence, reads/writes, per-slug vs global scope, cost — which capability does each serve (or none), and **what potential issues exist, rated by urgency?**

## Why it matters

- The tick is the **silent delivery mechanism** behind several capabilities (co-access→SL4, confidence→SL2, per-slug analytics→C5, pruning→RETAIN, phase signal→PD3) **and** the graph-healing/integrity side — but it is invisible in the map, so nobody can tell what is load-bearing, dead, wasteful, or actively harmful.
- Graph-healing ops are where the open integrity bugs live (#889 — compaction deletes edges to Proposed-status entries every tick; #890 — quarantine-restore loses inbound edges deleted by compaction). A tick that corrupts the graph is an **integrity risk, not a cost nit**.
- The human needs a triage read: which parts of the tick are potential **urgent** problems vs benign — to decide where to focus.

## What to explore (bounded)

1. **Inventory** — enumerate every tick phase/operation: name, trigger/cadence, tables read/written, in-memory caches rebuilt (Architectural Principle 7), per-slug vs global scope, approximate cost.
2. **Capability mapping (factual)** — map each op to the capability it silently delivers; flag **orphans** (ops serving no stated capability) and **silent dependencies** (capabilities whose delivery secretly rides the tick). This is identification, not a proposal.
3. **Graph-healing / integrity ops** — compaction, edge repointing, orphan sweep, quarantine-restore recovery: what each does and whether it heals or harms; tie to #889 / #890.
4. **Potential-issue identification** — surface, with `file:line` evidence and an **urgency rating**: correctness risks (does an op drop/corrupt data), wasteful/dead phases (the `compute_report`-inflation class), redundant work, near-threshold oscillation (#3822 class), GC scattered across features, per-slug correctness, unbounded cost, silent failure. **Flag with severity; do NOT diagnose-to-fix or recommend.**

## Expected output (FINDINGS.md)

- **(a) Inventory table** — every tick op with cadence, reads/writes, scope, cost.
- **(b) Capability mapping** — op→capability delivered; orphans; silent dependencies (factual identification).
- **(c) Potential-issues register** — each identified issue: what it is, `file:line` evidence, why it could matter, and an **urgency rating** (e.g., integrity-risk / correctness / cost / benign). **Ranked so the urgent areas surface first** — this is the triage read the human asked for.
- **NO recommendations, NO fix backlog, NO re-architecture, NO capability authoring.** The spike identifies and triages; the human decides what (if anything) to act on.

## Out of scope

- Any fix, recommendation, or re-architecture of the tick.
- Deep root-cause diagnosis of a specific issue (flag it with evidence + urgency; do not solve it).
- Authoring or proposing capability nodes (uni-zero decides that later, if warranted, from the findings).
- Deciding what to fix or in what order — that is the human's call from the triage read.

## Constraints / prior art

- **Read-only, no build**; code-verified to `file:line`.
- Prior art: open tick bugs **#889** (compaction deletes Proposed-edge every tick), **#890** (quarantine-restore loses compaction-deleted inbound edges); tick-cost/oscillation lessons + patterns (**#3822** near-threshold promotion oscillation; the `compute_report`-as-tick-loader inflation lesson); **Architectural Principle 7** (in-memory hot path rebuilt by tick); the capabilities the tick delivers (SL2 / SL4 / C5 / RETAIN / PD3; SLN3 / N3 graph-consistency-under-correction).
