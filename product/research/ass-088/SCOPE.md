# ass-088 — Edge-consistency under correction: synchronous-redirect vs read-time resolution vs background-tick convergence (reconciles #744, #745, vnc-042/NG-1)

## Problem Statement

When `context_correct` deprecates an entry X in favor of terminal X′, the edges touching X
must eventually point at X′ for the typed graph to stay integrity-consistent (architectural
principle #4). We are currently attempting that consistency **synchronously, at correction
time** — and that approach is bounded and orphan-prone. Three separately-tracked issues are
actually three faces of the *same* problem:

- **#744 (inbound orphans)** — `context_correct`'s incoming-edge redirect (`run_redirect_loop`,
  `tools.rs:4660`) caps at `REDIRECT_CEILING = 50` and truncates-with-`warn!` past it.
  Correcting a **hot node** (`vision_root`, core ADRs, widely-referenced patterns — exactly the
  heavy-tailed nodes most likely to cross 50) silently leaves referrers 51+ pointing at the
  deprecated entry. The ceiling exists *because* synchronous fan-out redirect of a hot node is
  expensive. The issue's own option list already names "complete redirect via batched/paginated
  work."
- **#745 (outbound orphans, historical)** — pre-vnc-035 corrections silently dropped outgoing
  edges; a **one-shot sweep** is proposed to repair the existing corpus. Already a
  convergence/repair framing, just not maintained.
- **vnc-042 / NG-1 (stale neighbor targets)** — with `include_edges` default-on (vnc-037), a
  resolved `context_get` returns the terminal's edge *list* but its *targets* still render the
  deprecated id+title. A resolved get is current for the entry you asked for, but its neighbor
  labels can be stale.

The read path already self-heals: `context_get` follows to current by default (vnc-042) and
`context_graph` has `resolve_supersessions`. So no *consumer* is forced to see stale content —
the residual is **stored-graph drift** and **discovery-label staleness**, both corpus-hygiene
problems. Hygiene problems are a poor fit for either the write path (heavy, ceiling-bounded
correction) or the read path (a permanent per-read resolution tax). This spike asks whether the
right home is **amortized background convergence** — keeping correction cheap and reads
resolve-on-demand, with a tick converging the stored graph — and, if so, whether that **retires
#744 and #745 as separate efforts** by folding them into one maintained convergence design.

This is an architecture decision (it changes an integrity guarantee's shape) that intersects
blocked work. It is scoped now so the decision is *derived*, and so the two blocked issues get a
single coherent answer instead of three point-fixes.

## Goal

Answer six questions and produce an ADR-ready recommendation:

1. **Enumerate the full failure surface.** Catalog every way an edge goes stale under
   correction/quarantine: inbound referrers past the ceiling (#744), outbound drops
   (historical, #745; and confirm the vnc-035 default closed this going-forward), stale
   neighbor *targets* in retrieval views (NG-1), and any others — quarantine chains, cycles/
   self-loops, cross-slug edges (per-slug stores, vnc-034), edges whose target chain
   dead-ends on a non-active terminal (the vnc-042 ADR-002 case). State which are *live*
   (going-forward) vs *historical* (existing-corpus) drift.

2. **Evaluate the three strategies** against each failure mode — **synchronous-at-write**,
   **read-time resolution**, **background-tick convergence** — on: correctness guarantee
   (immediate vs eventual), cost profile (who pays: writer / reader / tick), scale behavior
   (hot-node inbound degree grows with corpus and at enterprise scale — #744's explicit
   concern), failure/partial-work modes, and interaction with the in-memory hot path
   (principle #7). A hybrid is expected to win; the deliverable is *which mechanism owns which
   failure mode*, not a single winner.

3. **Recommend the division of labor.** Confirm or refute the working hypothesis: **write stays
   cheap** (deprecate + set `superseded_by`, no synchronous fan-out redirect) · **reads resolve
   on demand** (vnc-042 for get, `resolve_supersessions` for graph — the interim-window cover) ·
   **a background tick converges the stored graph** (repoints referrer edges to terminals,
   amortized). If refuted, say what wins instead and why.

4. **Rule the guarantee semantics.** If convergence is background, SLN3's write-side promise
   shifts from synchronous *"correcting a hot node carries/redirects its referrers and
   accumulates no orphan edges"* to eventual *"referrer edges converge to their current
   terminals; no permanent orphans."* Determine: is there any consumer that needs *immediate*
   stored-graph consistency (i.e., one that cannot tolerate the convergence window and cannot
   resolve at read)? Define a **convergence SLO** — a bound on how stale the stored graph may be
   (e.g. ≤ N ticks / ≤ T after a correction) that makes the eventual guarantee concrete and
   testable.

5. **Design the tick (mechanism sketch, enough to cost it).** Fold into an existing tick or a new
   one? (The co-access promotion tick already writes `GRAPH_EDGES` bidirectionally — pattern
   #3897 — so edge-mutating ticks have precedent and infra.) Address: idempotency, **bounded
   work per tick** (never a full-corpus scan every tick), how it finds edges needing repoint
   (a work queue vs a scan), ordering/interaction with concurrent corrections, **audit-log
   volume** (principle #2 — every state change is audited; a repoint sweep must not bloat the
   audit log — batch or summary-level auditing), and the hash-chain question (edges are a
   separate table, not in the entry hash chain — confirm no principle-#1 interaction).

6. **Reconcile #744, #745, and NG-1.** State explicitly whether the recommended design **retires
   #744 and #745 as standalone efforts** (folding the ceiling-orphan fix and the historical sweep
   into the convergence mechanism + a one-shot backfill), or whether either remains independently
   necessary. Give the **existing-corpus backfill** story (#745): does the same convergence
   mechanism sweep the historical orphans, or is a separate one-shot pass still required?

## Breadth

`code+ecosystem` — **code-dominant.** The primary surface is internal: `context_correct` /
`run_redirect_loop` / `REDIRECT_CEILING` (`tools.rs`), the typed-edge storage
(`GRAPH_EDGES`, `graph.rs`), the supersession columns (`schema.rs`), the existing tick
infrastructure (co-access promotion tick and analytics rebuild tick), the audit-log DDL
triggers, and the per-slug store boundary. A **light** external scan of eventual-consistency
graph-repair / reference-rewriting convergence patterns (how comparable systems repair dangling
references off the hot path) is in scope only insofar as it informs the SLO and the bounded-work
design — **not** industry-depth.

## Approach

`investigation` + `design`, with an optional **quick cost estimate** (not an empirical sweep):
enumerate the failure surface from the code, evaluate the three strategies, and produce a
mechanism sketch. If cheap, a back-of-envelope estimate of synchronous-redirect cost vs corpus
hot-node inbound degree (to make #744's scale argument concrete) — a small probe, not a
characterization.

## Confidence required

`directional`. A grounded recommendation — failure-surface catalog + strategy division-of-labor +
guarantee-semantics ruling + convergence SLO + tick mechanism sketch + #744/#745 reconciliation —
sufficient to author an ADR and re-scope the follow-on work. Not a proven implementation.

## Target outputs

FINDINGS.md containing:
- The **failure-surface catalog** (Q1), each item tagged live vs historical.
- A **strategy comparison** (Q2) — synchronous / read-time / background — across the stated
  dimensions, with the **recommended division of labor** (Q3).
- The **guarantee-semantics ruling** (Q4): eventual vs synchronous, any immediate-consistency
  consumer, and a concrete **convergence SLO**.
- A **tick mechanism sketch** (Q5): placement, idempotency, bounded-work strategy, audit
  approach, hash-chain confirmation.
- An explicit **#744 / #745 / NG-1 reconciliation** (Q6): what is retired, what remains, and the
  existing-corpus backfill story.
- A recommended **SLN3 `done_when` rewording** (mechanism-agnostic) for the goal steward.
- **Unanswered Questions** and **Out-of-Scope Discoveries**.

## Constraints

**Hard** (architectural principles — non-negotiable; a recommendation that violates one is out of
bounds):
- **#1 Hash-chain integrity is immutable.** Confirm edge repointing does not touch the entry hash
  chain (edges are a separate typed-edge table). If any recommendation *would* touch it, it is
  wrong.
- **#2 Audit log is append-only and complete.** A tick that mutates edges is a state change and
  must be audited — but must not bloat the audit log. The design must state how it audits
  convergence work (batch/summary vs per-edge).
- **#5 Graceful degradation.** Convergence must fail-loud-not-broken: a tick that cannot complete
  its bounded work leaves the graph in a resolvable-at-read state (vnc-042 / `resolve_supersessions`
  cover), never a worse one.
- **#7 In-memory hot path.** Analytics-derived search data is `Arc<RwLock<_>>` rebuilt by tick and
  never read from DB at query time. The convergence tick must not violate this — determine whether
  repointed edges require an in-memory rebuild and how that sequences.
- **Per-slug boundary (vnc-034).** Corrections and edges are per-slug; the convergence mechanism
  must respect store isolation — no cross-slug edge repoint.
- **Read-only in Unimatrix / no code, no PR.** This is a research spike. It *recommends* an ADR and
  a re-scope; it changes nothing.

**Hypothesis** (challengeable positions to test, NOT assumptions to carry):
- "**Background convergence is the right home.**" The working position (write cheap + read-resolve +
  tick converge). Challenge it — e.g. if some consumer genuinely needs immediate stored consistency,
  synchronous redirect (with the ceiling *fixed*, not removed) may still be required for a subset.
- "**Eventual consistency is safe because reads self-heal.**" Test the claim that vnc-042 +
  `resolve_supersessions` fully cover the interim window for *every* consumer — including
  `context_graph` traversal, briefing/injection, and any analytics that read edges.
- "**#744 and #745 collapse into this.**" Challenge whether the ceiling-orphan fix and the historical
  sweep are genuinely subsumed, or whether one has a residual that convergence does not address.

## Background Research / Prior art

Read directly for this scope:
- **#744** — `REDIRECT_CEILING = 50` inbound-orphan defect; `run_redirect_loop` (`tools.rs:4660`,
  ceiling at `tools.rs:44`); the enterprise/scale dimension; the option list (raise/configurable,
  batched-complete, surface-loud, accept-with-contract).
- **#745** — pre-vnc-035 outbound-orphan one-shot sweep; the "repair data, not product behavior"
  framing; two goal entries (`personal-cloud`, `proactive-delivery`) hand-restored 2026-06-10.
- **vnc-035 / #730** — flips `context_correct` to carry **outbound** edges forward by default
  (the going-forward fix for the outbound mirror). Confirm scope: outbound-only, referrer/inbound
  redirect is the separate `run_redirect_loop` path.
- **vnc-042 (this session)** — read-time supersession resolution on `context_get`; ADR-002 dead-end
  path; NG-1 defers stale neighbor-target resolution. The read-side interim-window cover.
- **capability SLN3 (#5230)** — "the typed knowledge graph stays integrity-consistent under
  correction"; write-side `done_when`; the clause this spike may reword.

Unimatrix knowledge:
- **Pattern #3897** — helper-extraction for infallible bidirectional tick writes to `GRAPH_EDGES`
  (co-access promotion tick). Direct precedent that edge-mutating ticks exist, are infallible-per-
  direction, and follow an established write pattern — the convergence tick is an *extension* of
  this infra, not greenfield.
- **crt-034 / crt-035** — the infallible-tick contract (a tick must not abort the cycle on a single
  write failure). Any convergence tick inherits this.
- **#4468** — supersession chain traversal must use the SQL recursive CTE, never in-memory
  `find_terminal_active`. Convergence repoint-target computation must comply.
- **#4494** — when substituting a deprecated node for its terminal, track the substitution carefully
  to avoid dropping it — directly relevant to a repoint sweep.

Issues (context, not required reading depth): #606 (vnc-017, established incoming redirect +
`REDIRECT_CEILING`), #625 (`neighbors_via_db` unbounded at cold-start — a related edge-scan bound).

## Open Questions

- Is there any consumer of the **stored** graph (not the read tools) that needs immediate
  consistency and cannot resolve at read — e.g. an analytics pass, briefing/injection edge-walk,
  or a `context_graph` traversal used for a gating decision?
- Should convergence be **work-queue-driven** (a correction enqueues its referrers for repoint) or
  **scan-driven** (a tick periodically finds orphaned edges)? The queue bounds work to actual
  corrections; the scan needs no new write-path coupling but must bound its scan (#625's concern).
- Does the historical backfill (#745) share the mechanism, or does the pre-vnc-035 corpus need a
  distinct one-shot pass (different because the *prior link* to reconstruct from is ambiguous)?
- What is the right **convergence SLO** unit — ticks, wall-time, or corrections-processed — and how
  is it made testable without flaking under load (cf. the tick-timing flakes #790/#833)?
- Does repointing referrer edges have any **PPR / retrieval** consequence (Prerequisite edges are
  PPR-positive; redirecting a referrer changes what surfaces together) that must be preserved?

## Tracking

GH research issue **TBD** (ass-088) — to be created (`goal:self-learning`, `research`) referencing
this scope. Single spike → executed via `uni-spike-researcher` once scope is confirmed complete.
Findings feed a new **ADR** (edge-consistency strategy) and a re-scope of **#744 / #745**; informs
the deferred **vnc-042 / NG-1**. The read-side SLN3 clause and the mechanism-agnostic write-side
rewording are goal-steward (uni-zero) actions taken after findings land.
