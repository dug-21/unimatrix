# SCOPE — ass-096: Novel failure-mode discovery from behavioral signal — feasibility, method, and the safety boundary

Origin: uni-zero consult following ass-095 (product/research/ass-095/FINDINGS.md).
This spike is the **discovery half** of a proposed closed loop
(recognize novel failure mode → solidify → capture resolution → real-time injection).
It scopes ONLY discovery + the safety boundary. The injection vertical-slice is deliberately
out of scope (separate proof).

## Framing

ass-095 established the substrate: the only **untainted ground truth** for failure is the behavioral
event (`PostToolUseFailure` — the system reporting a real error), NOT the `lesson-learned` corpus
(agent-authored interpretation, 1.1% literal density, circular). It also measured the opening:
**273 of 639 real failures matched NO known signature** — an unmatched residual that is, by
construction, the novel-mode feed and requires no human/agent authorship to appear.

The question this spike answers: **can we turn that behavioral residual into recognized, characterized
failure modes — safely — using the semantic stack we already have?** "Safely" is not a footnote:
the eventual loop surfaces resolutions, and a resolution to a *security control firing correctly*
(e.g. the write-block capability denial) is an authz-bypass coaching. Discovery must separate
assist-safe friction from controls-working-as-designed, and fail safe when unsure.

## Goal (answerable questions)

- **G1 — Discovery method (empirical).** Can the unmatched-residual failure events (the `-32003`,
  `-32602`, and uncatalogued `PostToolUseFailure` snippets + surrounding event context) be clustered
  into coherent candidate failure modes using our existing embedding/HNSW stack? Measured on our real
  corpus: how many coherent novel modes emerge from the ~273 unmatched, and how much is irreducible
  noise (e.g. routine `file_not_found`)?
- **G2 — Solidify / promotion criteria (empirical).** What distinguishes a *failure mode* from a
  one-off or noise cluster — recurrence across sessions/cycles, cluster stability, distinctness from
  known signatures? Define a promotion threshold (anomaly → recognized mode) grounded in the corpus.
- **G3 — Characterization (empirical/directional).** Once a cluster is a mode, what can we reliably
  extract to make it actionable — tool, precondition / what-preceded, error class — deterministically
  vs. semantically? Where (if anywhere) does local inference (Principle 5, with fallback) earn its
  place over deterministic characterization?
- **G4 — The safety boundary (LOAD-BEARING).** Can discovery classify a mode as **assist-safe
  friction** vs. **a security control firing correctly** (authz / capability / policy denial), so the
  downstream loop never coaches a principal past a control? The write-block `-32003` capability denial
  is the canonical trap — friction-shaped, but authz. What signal separates them (error-code taxonomy,
  event structure, principal/capability context)? What is the fail-safe default (unclassified → do NOT
  assist)? How does the injection channel itself bound blast radius (SLN1 poison surface)?
- **G5 — Resolution-capture feasibility (forward-looking, MUST NOT block this spike).** Is behavioral
  **recovery-sequence mining** (action-after-failure → subsequent success) a viable *untainted* source
  of candidate resolutions? What is the noise/causation risk, and what validation would a resolution
  need before it could ever be injected? Horizon read only.
- **G6 — Go/no-go + recommended discovery design.** Given G1–G5: is behavioral+semantic novel-mode
  discovery tractable on our data? Recommend the discovery method + promotion criteria + safety-
  classification scheme, or report the blocking gap.

## Breadth
`code+ecosystem` — code-dominant. Internal: observation store, embed/HNSW stack, error-code taxonomy,
injection machinery. Ecosystem: clustering / anomaly-taxonomy method prior art (lightly — reproduce,
don't re-derive).

## Approach
`measurement` + `proof-of-concept` for G1–G3 (ephemeral clustering harness under
`product/research/ass-096/`, run on our real residual); `investigation` for G4–G5 (design + taxonomy,
no build required).

## Confidence required
`empirical` for G1–G3 (data from clustering our real corpus). `directional` for G4–G5 (a defensible
classification scheme + feasibility read; not a validated build).

## Target outputs
- Go/no-go on discovery tractability.
- Recommended discovery method + promotion criteria.
- A **safety-classification scheme** (assist-safe friction vs. control-firing-correctly) with a
  fail-safe default and blast-radius note.
- Resolution-capture feasibility read (G5).
- Design input for the eventual closed loop.

## Constraints

**Hard (fixed):**
- Ground truth is the **behavioral event**, not `lesson-learned`/KB text (agent-authored interpretation,
  not ground truth).
- Reuse the existing embedding/HNSW stack; local inference only within Principle 5 (graceful
  degradation — absent/failed model = previous behavior).
- Research only: no committed product code, no PR, no Unimatrix writes; harness is ephemeral.
- **The safety boundary is non-negotiable**: the eventual loop must never coach a principal past a
  security control. Capability/authz/policy denials (Principle 3 — capability checks at the service
  layer) are **controls, not friction**; unclassified → do NOT assist (fail-safe).
- No raw transcript persisted to disk (NG-1, #4721/#4850).

**Hypothesis (challengeable — positions to test, not givens):**
- The unmatched residual is a coherent novel-mode feed (ass-095 measured 273/639 unmatched but never
  clustered them — could be dominated by junk like `file_not_found`).
- Semantic clustering yields *actionable* modes, not just groupings.
- Recovery-sequence mining yields trustworthy resolutions.
- Friction vs. control is cleanly separable from event signal (error code + structure + principal).
- Local inference earns its place over deterministic characterization.

## Dependencies
Builds on **ass-095** (the residual measurement, the event surface, the signature baseline — read as
prior art, do not re-run). If `go`, unblocks (a) the injection vertical-slice proof and (b) a
capability under self-learning ∩ proactive-delivery.

## Prior art
- ass-095 FINDINGS — the 273/639 residual, event shape (`PostToolUseFailure` + `response_snippet`, no
  exit_code field), the precision lesson (anchor on event structure, not prose), the 22-rule baseline.
- Semantic stack: embeddings + HNSW (`context_search` path).
- Proactive injection machinery (`goal:proactive-delivery`) + the `PreToolUse` hook surface.
- Principle 5 (local ML NLI/GNN/GGUF with fallback); Principle 3 (capability checks at service layer);
  SLN1 (#5528 — training/injection poison resistance; the injection channel is a poison vector).
- #898 Q4 (local inference over transcript — adjacent, do not conflate) · #941 (cross-run analytics).
- The write-block case (`-32003`, lesson #5465, memory "Unimatrix Write needs agent_id"): the canonical
  example that is simultaneously a known mode, a known fix, AND a capability control — the G4 trap.
