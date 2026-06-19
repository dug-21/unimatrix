---
name: "uni-capability"
description: "Manage a goal's capability map in Unimatrix — the behaviorally-proven units that must exist for a goal to be delivered. Decompose goals into capabilities, track delivery status, and report what's left. Status advances to proven ONLY on attached behavioral evidence."
---

# uni-capability — Goal Capability Management

> The layer between **goals** (intent) and **features** (delivery). A *capability* is a concrete,
> outcome-phrased unit that must **exist and behaviorally work** for a goal to be delivered. It is
> "proven" only when a behavioral, real-artifact test clears its `done_when` — never when a feature
> merely *claims* it. This skill creates, updates, and queries capabilities; the goal entry itself
> stays the stable *intent* (do not bury volatile status in it).

## Why this exists

Features were marked "delivered" against goal criteria they only *structurally* satisfied — the pieces
existed, the behavior didn't, and every gate passed (e.g. per-slug analytics handles constructed but
never maintained by the tick). The capability map closes that hole two ways at once: it forces the
goal's full **decomposition** (no dimension silently dropped) and it forces **behavioral proof** (no
"structure exists" standing in for "it works"). It is also the substrate for goal-driven and
eventually autonomous delivery — "what's the next capability to build" becomes a query.

---

## The schema (single source of truth — maintained HERE)

A capability is a **Unimatrix entry**, `category: "capability"`. Entry id = the global capability id
(no per-goal numbering — a shared capability is ONE entry with multiple `Advances` edges).

```
Fields (in the entry content / structured body):
  name          OUTCOME a user/operator experiences — never an implementation
  why           one sentence — the problem it solves
  done_when     1-2 BEHAVIORAL, runnable statements — the proof gate AND definition of done
  status        missing | partial | proven | claimed
  delivered_by  GH ref(s), e.g. "#787" / "vnc-039"   (FIELD — target is not a Unimatrix node)
  proven_by     evidence ref, e.g. "live: arch-research store/get round-trip" (FIELD)

Edges (RelationType — validated against unimatrix-engine/src/graph.rs):
  Advances      capability -> goal         PPR-neutral. "this capability advances goal G".
  Prerequisite  capability -> capability   PPR-POSITIVE. dependency/DAG. DIRECTION: the prerequisite
                                           is the SOURCE — "C5 -Prerequisite-> C6" means C6 depends on C5.
  Motivates     research   -> capability   PPR-NEUTRAL. "this research drove/shaped this capability."

Corrections (lifecycle):
  context_correct   sharpen done_when / reword / record a regression — preserves provenance.

Status legend:  missing 🔴 | partial 🟡 | proven 🟢 | claimed ⚪ (asserted, no behavioral test exists)
```

**Edge-choice rationale (do not change without re-validating):**
- `Motivates` for research, NOT `Informs`. `Informs` is PPR-positive (`graph_expand.rs`) and would pull
  research findings into agent retrieval. Research is a *candidate*, not knowledge — it must stay inert
  in retrieval until it graduates into a capability. `Motivates` is PPR-neutral and `context_graph`-navigable.
- `delivered_by` / `proven_by` are **fields, not edges** — their targets (GH features, test artifacts)
  are not Unimatrix entries, so no edge can point at them.
- **Retrieval-visibility lever:** `Prerequisite` is PPR-positive, so capabilities cross-surface *each
  other* in retrieval (capability↔capability, never research — that's fine). If capabilities should be
  kept out of *agent delivery* retrieval entirely, filter by `category != "capability"` at the
  retrieval layer — do NOT mangle the edge type; the DAG needs `Prerequisite`.

---

## The firewall (load-bearing — the one rule that makes this trustworthy)

> **Status advances to `proven` ONLY on attached behavioral, real-artifact evidence.**
> Research, claims, and "the feature merged" move *structure*, never *status*. A capability with a
> merged feature but no behavioral proof of its `done_when` stays `partial`/`claimed`.

This is the firewall between the two sub-processes below, and it is what makes autonomous drive safe
(an autonomous loop that trusts "claimed done" compounds rubble at machine speed).

## Two sub-processes

- **Structural management** — *what capabilities exist.* Low-frequency, judgment, uni-zero + research.
  Creates/splits/merges nodes, sharpens `done_when`, adds edges.
- **Status management** — *is it done.* Per-delivery, evidence-driven, gated. Flips status, attaches proof.

---

## Inputs (what drives a create/update, and who owns it)

| Input event | Effect | Owner | Touches |
|---|---|---|---|
| **New goal identified** | research → uni-zero **synthesizes** the initial decomposition (nodes `missing`/`claimed`) | uni-zero + research | structure |
| **Research completes** | add / split / merge capability; sharpen `done_when`; add `Motivates` edge | uni-zero (research-fed) | structure |
| **Feature delivers + behavioral proof clears `done_when`** | status → `proven`; set `delivered_by` + `proven_by` | delivery / vision-guardian gate | **status** |
| **Feature merges, `done_when` NOT proven** | stays `partial`/`claimed`; raise a variance | vision-guardian gate | status |
| **Gap / regression discovered** (dogfood, retro, incident) | add a missing capability, or `proven → partial` + sharpen `done_when` | uni-zero (gap-fed) | structure + status |
| **Dependency identified** | add `Prerequisite` edge | design / uni-zero | structure |

Research produces *findings*; it never authors capability nodes directly and never satisfies one — the
synthesis from findings into outcome-phrased capabilities is the vision judgment.

## Standardized update procedure (same for every input)

1. **Resolve or create** the affected capability node(s) (`context_lookup`/`context_search` by goal/name).
2. **Apply.** A *structural* change carries a reason + provenance (`context_store` for new, `context_correct`
   to evolve). A *status* change to `proven` MUST attach the behavioral evidence in `proven_by` — or it
   does not happen (the firewall).
3. **Recompute** the DAG → the new next-unblocked set (`context_graph` over `Prerequisite`).

## Lifecycle

```
missing ──build+prove──> partial ──done_when fully cleared (behavioral)──> proven
   ▲                                                                          │
   └──────────── gap / sharpened done_when no longer met (context_correct) ───┘
claimed = asserted (often inherited from a goal criterion) with no behavioral test yet — a flag, not a state to rest in.
```

---

## Operations

### Decompose a new goal
1. Confirm the goal entry exists (`context_lookup category="goal"`). Scope research if the capability set is unknown (uni-zero writes the spike scope; a research session executes).
2. Synthesize findings → outcome-phrased capabilities (apply the authoring rules below).
3. For each: `context_store category="capability"` with the fields, and an `Advances` edge to the goal:
   ```
   context_store({ category: "capability", topic: "<goal-tag>",
     content: "name: …\nwhy: …\ndone_when: …\nstatus: missing\ndelivered_by:\nproven_by:",
     tags: ["capability", "<goal-tag>"],
     edges: [{ relation: "Advances", target_id: <goal_id> }] })
   ```
4. Add `Prerequisite` edges for dependencies (source = the prerequisite).

### Mark a capability proven (the gate)
- ONLY with attached behavioral evidence. `context_correct` the entry: set `status: proven`, fill
  `proven_by` (the real-artifact test/evidence) and `delivered_by`. No evidence ⇒ leave `partial`/raise variance.

### Record a gap / regression
- `context_correct`: `proven → partial`, **sharpen `done_when`** to encode the newly-discovered bar.
  This is the dev-process self-learning loop — reality contradicted "proven," the definition tightens.

### Report what's left for a goal (the strategic query)
- `context_graph` neighbors/subgraph from the goal over `Advances` (incoming) → the capability set;
  read `status`. Group: 🟢 proven / 🟡 partial / 🔴 missing / ⚪ claimed. The 🔴/⚪ set with no unmet
  `Prerequisite` is **what to build next**; ⚪ are **honest-unknowns to retire** (claimed, never tested).

### Link research
- `context_edge` add `Motivates` from the research entry → the capability it shaped. PPR-neutral by design.

---

## Authoring rules (what keeps it concrete, not a novel)

- **Outcome altitude.** Name the *outcome a user/operator experiences*, never the implementation
  ("per-slug analytics are maintained", not "wire the tick to iterate stores"). The HOW lives in the
  feature/design below. This keeps capabilities layman-readable, stable across rewrites, and naturally
  behavioral.
- **`done_when` must be runnable.** If you cannot state it as 1–2 behavioral tests, the capability is
  too big (split it) or too vague (sharpen it). This field is the human's "what, concretely," the
  machine's "is it done," and the proof gate — all three.
- **Right size ≈ a feature's worth of outcome** — bigger than a task, smaller than a goal; maps to ~1–3
  features. Ten features ⇒ split. A single function with no observable outcome ⇒ it's a task, fold up.
- **Research is not a capability.** Spikes drive capability adds/changes and sharpen `done_when` — they
  never appear as capability nodes and never satisfy one. (`Motivates` edge, not membership.)
- **Cross-goal capabilities are ONE node** with multiple `Advances` edges (e.g. "per-slug analytics
  maintained" advances both `personal-cloud` and `self-learning`). Global ids (Unimatrix entry ids)
  make this free — never duplicate the node per goal.
- A **rollup capability** (the goal's marquee promise) is legitimate — it has `Prerequisite` edges to the
  capabilities it composes and goes 🟢 only when all of them do.

---

## Boundaries

- This skill manages the `capability` category and its three edge types ONLY. It does not store ADRs,
  patterns, lessons, conventions, or procedures (those have their own skills/owners).
- Structural changes are uni-zero / vision judgment (often research-fed). Status→proven is a
  delivery/vision-guardian act bound by the firewall. Keep the two separated.
- Capability maps live in Unimatrix (the graph). A per-goal markdown view is acceptable only as
  short-lived alpha scaffolding; it cannot represent cross-goal nodes without duplication and does not
  scale past a couple of goals.

## Relationship to the rest of the process

- **uni-zero** owns the structural side (decompose goals, synthesize research, record gaps).
- **Design protocol** maps each feature to the capability(ies) it delivers + the behavioral test that
  will clear `done_when`.
- **Vision-guardian / Gate 3c** enforces the firewall: a feature may mark a capability `proven` only
  with attached behavioral evidence; a claimed-but-unproven capability is a variance, not a pass.
- **Retro** feeds discovered gaps back as `proven → partial` corrections (sharpened `done_when`).
