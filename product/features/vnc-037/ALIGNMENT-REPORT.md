# Alignment Report: vnc-037

> Reviewed: 2026-06-15 (re-check after human-directed next-hop reframe)
> Artifacts reviewed:
>   - product/features/vnc-037/architecture/ARCHITECTURE.md (REVISED)
>   - product/features/vnc-037/specification/SPECIFICATION.md (REVISED)
>   - product/features/vnc-037/RISK-TEST-STRATEGY.md (REVISED)
>   - product/features/vnc-037/SCOPE.md (REVISED) + SCOPE-RISK-ASSESSMENT.md (REVISED)
> Vision source: product/PRODUCT-VISION.md
> Goals consulted: #4671 (root vision), #4673 (proactive-delivery), #4677 (self-learning), #4678 (domain-agnostic), #4946 (personal-cloud)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Surfaces the typed graph (Principle 4) at the richest read; advances proactive-delivery + self-learning by closing the assert→surface feedback loop. |
| Milestone Fit | PASS | Vinculum (MCP surface) read-path feature; no future-milestone capability pulled forward. |
| Scope Gaps | PASS | All SCOPE goals + AC-01…AC-12 are carried verbatim into spec/arch/risk. |
| Scope Additions | WARN | OQ-03 internal-caller opt-out is an architect recommendation beyond SCOPE's stated decisions; advisory but partly promoted into ACs/tests. One soft item only. |
| Architecture Consistency | PASS | ADR-001 reconciliation (OQ-04) is flagged and owned; ranked-variant isolation from the shared neighbors path is consistent across all three docs. |
| Risk Completeness | PASS | SR-01…SR-14 fully traced to R-01…R-20; reframe's new failure modes (canonicalization, ranking, latency) get Critical/High discriminating coverage. |

## Reframe-Specific Assessment (the spawn question)

**Does "ranking as core / product judgment in the feature" introduce scope creep or vision drift? — No.**

The reframe (edge dump → ranked ≤3 next-hop affordance) was **human-directed at scope approval** and is recorded as such in SCOPE.md ("locked in uni-zero scoping") and the SCOPE-RISK-ASSESSMENT header. The three source docs faithfully implement the human's intent; they do not invent it. Specifically:

- **The product judgment is bounded and grounded, not open-ended.** The "which 3" rule (D-09) is a fixed, two-tier deterministic ordering: authored-first, then inferred by *existing* `entries.confidence`. It introduces **no new scoring model, no new signal, no tuning surface**. It reuses the already-cached Bayesian composite (`db.rs:549`) read-only. This is consistent with the memory caution against elevating convenience features to vision-level — ranking here is a *display selection rule over existing data*, not a new intelligence component competing with the GNN (self-learning goal #4677).
- **Vision fit is stronger after the reframe, not weaker.** Principle 4 (typed relationship graph — "graph traversal surfaces what vector search alone cannot") is now served as a *curated pointer* at the point of consumption rather than a wall of edges. The honest-uncapped-totals + visible-empty-box mechanism is what closes the author-assert feedback loop (proactive-delivery #4673 / self-learning #4677): authors who declare edges now *see* them, and a zero-edge entry is visibly zero. The cap sharpens that loop rather than diluting it.
- **No drift into orchestration or new storage.** Read-path only, depth-1, no schema migration, no new edge type, no multi-hop (Non-Goals, C-1…C-3). The feature stays inside the knowledge-engine boundary the vision draws (Vision ¶3: "not an orchestration engine").
- **The ass-079 grounding (rank by target confidence, not frozen edge weight) is a correctness decision, not scope expansion.** It removes a misleading signal rather than adding a feature.

**Net:** the reframe *narrows* the delivered surface (cap 10→3, display-only) while *raising* the correctness bar (canonicalization, ranking determinism, latency budget). That is discipline, not creep — with two soft watch-items below.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition (soft) | OQ-03 internal-caller opt-out (`include_edges:false` default-off for hook path / briefing by-ID / by-ID loop fetches) | SCOPE.md raises OQ-03 as an *open question* ("decide whether"). ARCHITECTURE resolves it to a concrete recommendation and the spec/risk docs promote "enumerate + assert each internal call site as a test" (R-14 scenario 3, AC-11 inventory). This is net-new call-site-touching work beyond SCOPE's single-default-on framing. Advisory at the type level (D-01 stays default-on), but it expands the implementation/test footprint. See Variance 1. |
| Simplification | Markdown author/inferred sub-split **dropped** | Rationale (documented, ADR-005 / OQ-02): ranking already front-loads authored edges, so the sub-split is redundant. Consistent across SCOPE D-08, SPEC FR-14, RISK R-17. Acceptable. |
| Simplification | Provenance reduced to `authored` boolean rather than a `provenance` enum | Rationale (documented, D-03 / C-10 / SR-05): NLI is dark, so all live inferred sources are statistical — a boolean is the honest split. `source` string retained underneath with a documented revival trigger. Acceptable. |
| Gap | none | Every SCOPE goal (1–6) and AC-01…AC-12 maps to a spec FR + risk scenario. No dropped requirement. |

## Variances Requiring Approval

### 1. WARN — OQ-03 internal-caller opt-out expands footprint beyond the SCOPE open question

1. **What**: SCOPE.md frames OQ-03 as undecided ("decide whether internal callers should pass `include_edges:false`"). The architecture resolves it to a firm recommendation (hook path + briefing by-ID + by-ID loop fetches default-off), and the spec/risk docs promote enumerating and **asserting each internal call site as a named test** (RISK R-14 scenario 3; SPEC OQ-03 "make each `Some(false)` an asserted test"). This converts an open question into in-scope implementation + test work.
2. **Why it matters**: It is a (small) scope addition relative to SCOPE's "single default-on code path" framing, and it changes default behavior for non-agent-facing call sites. It also has a vision-adjacent edge: defaulting *agent-facing* paths off would weaken the proactive-delivery feedback loop (#4673) — the docs correctly keep the MCP tool boundary default-on, but the human should confirm the internal-vs-agent-facing line is drawn where intended. The architect explicitly marks the recommendation "advisory to the human/spec," so this is surfaced for a decision, not silently absorbed.
3. **Recommendation**: **Accept with confirmation.** The opt-out is well-justified (relieves SR-12 latency on the hottest read without touching the loop) and the docs are internally consistent. Human should confirm (a) the enumerated internal call sites are correct/complete and (b) no agent-facing path is flipped default-off. If accepted, it is a clean addition; if the human prefers to keep a single default-on path, drop the per-call-site opt-out and rely on the AC-12 budget alone.

### 2. WARN — AC-12 latency numbers are provisional (unbacked until measured)

1. **What**: The proposed budget (≤5 ms p50 / ≤15 ms p95 added over the edge-free baseline) is explicitly **not yet measured** (SCOPE AC-12, SPEC NFR-2/C-9, RISK R-13/SR-12). The default-on confidence-JOIN + split `COUNT(*)` land on the hottest read, which also feeds the co-access loop, so cost compounds.
2. **Why it matters**: This is the one genuinely *new* obligation the reframe adds (the prior edge-dump framing had no latency NFR because it was Rust-side slicing). It is correctly scoped — the docs mandate a measured baseline before the numbers lock and escalate to the human if the budget proves unattainable. But until measured, the AC is conditional, and an unbacked budget on `context_get` is a real regression risk. This is a soft constraint the docs themselves flag for human decision.
3. **Recommendation**: **Accept the obligation as written.** Require the measured edge-free baseline (high-degree node in scope) before the AC-12 numbers are locked, per C-9. If the baseline shows the budget unattainable, the documented options (relax budget / mandate OQ-03 opt-out / revisit default-on) go to the human. No change to the docs needed — the escalation path is already specified.

No FAIL-level variances. No vision-drift variances.

## Detailed Findings

### Vision Alignment — PASS

- **Principle 4 (typed relationship graph)** is the principle this feature most directly advances: it makes author-declared and inferred edges traversable *at the point of read*, with `target_id`s as entry points back into `context_graph`. The DISCOVERY-LIST boundary (ARCH §"The DISCOVERY-LIST boundary") keeps `context_get` a thin pointer and `context_graph` the detail/traversal tool — a clean division that respects the existing `EdgeRecord` contract.
- **Principle 7 (in-memory hot path)** is correctly *not* over-applied: the docs deliberately read **live `graph_edges` via SQL** at depth-1 (FR-1, ARCH §1) rather than the in-memory `TypedRelationGraph`, because freshness on a point read matters (a just-written/just-carried-forward edge must be visible). This is the documented, ADR-#4479-grounded exception, not a violation.
- **Proactive-delivery (#4673) / self-learning (#4677):** the assert→surface feedback loop is the strategic payoff. Surfacing closes the loop the author-assert convention opened; the visible empty box is the mechanism. No new learning signal is introduced (the ranking reuses existing confidence), so it does not collide with the GNN relevance function.
- **No over-build / defensive-structure inflation** (memory caution honored): the feature does not elevate edge-integrity to a vision principle; it is a read-path affordance with a bounded ranking rule. Symmetric canonicalization (D-10) and the LEFT-JOIN dangling handling are *correctness* requirements surfaced by the reframe, not gold-plating — each maps to a concrete double-count/drop defect.

### Milestone Fit — PASS

Vinculum (MCP server / connectivity) read-path surface feature. No Cortical (learning/drift), Alcove, or Matrix capability is pulled forward. The confidence column is consumed read-only — no learning-pipeline change. Depth-1 only; multi-hop stays in `context_graph` (Non-Goals). Backfill of historical edges remains deferred (#708). Nothing targets a later milestone prematurely.

### Architecture Review — PASS

- **Ranked-variant isolation (SR-02 / SR-06)** is consistently specified: the plain `query_direct_neighbors` shared with `context_graph` neighbors gains **only** the additive `source` column; all rank/JOIN/canonicalization/`↔`/LIMIT logic lives in a **separate ranked variant** so the neighbors contract stays byte-stable. ARCH §Component Breakdown, SPEC FR-16/C-3, and RISK R-08 agree.
- **ADR-001 reconciliation (OQ-04)** is correctly surfaced as an architect action, not left implicit: ADR-001 (#5009) predates the reframe (describes reusing the plain neighbor query) and must be updated via `context_correct` to the rank-and-limit-in-SQL strategy. SPEC dependencies and ARCH ADR table both flag it. Consistent with the memory rule (use `context_correct`, not deprecate+store, for ADR updates). Owned and tracked — no alignment issue.
- **Serializer seam (ADR-003)** unchanged and consistent: `None ⇒ key absent` invariant preserves byte-identity for the four list-view tools (FR-13/SR-01/R-07), asserted via the real producer (#1268), not a hand-crafted snapshot.
- **Open questions** are appropriately classified: OQ-A (fail vs degrade on edge-query error) leans "fail," flagged non-blocking; OQ-B (file size) pre-authorizes sibling modules; OQ-C/AC-12 measured baseline blocks only the number lock. None block the design.

### Specification Review — PASS

- FRs map 1:1 to SCOPE decisions D-01…D-10 and AC-01…AC-12; nothing relitigated, nothing dropped. The locked `ORDER BY (source='agent') DESC, t.confidence DESC LIMIT 3` (C-8/FR-9) is stated as a hard requirement — the right altitude for the "which 3 is the feature" reframe.
- Guardrails (§Design Guardrails) restate the human-directed boundaries (affordance-not-dump; discovery-list-not-detail; cap-display-only-totals-honest) and bind every downstream requirement — good traceability of the human's intent.
- Documented Non-Bug Behaviors (DNB-1 dangling, DNB-2 corrected-entry transient, DNB-3 visible zero) correctly encode "emptiness is honest, not a defect" as test cases (SR-07).
- OQ-03 (internal-caller) is the only place the spec reaches past SCOPE's open question — see Variance 1.

### Risk Strategy Review — PASS

- **Full SR→R traceability table** present (§Scope Risk Traceability); all SR-01…SR-14 mapped, including the carried-forward pre-reframe IDs SR-03/SR-04.
- The reframe's three new dominant failure modes get the right priority: symmetric canonicalization (R-01, Critical, tested **independently on display AND totals**), ranking correctness (R-02, Critical, discriminating per #3886 with the proof value *outside the cap*), and rank-in-SQL-not-Rust (R-04, Critical, proven at the store boundary). These are the exact risks "ranking as core" introduces, covered as discriminating tests, not smoke (SR-13 honored).
- Latency (R-13/SR-12) correctly requires a measured baseline before locking numbers and specifies the escalation path — consistent with Variance 2.
- Security section is proportionate: read-only path, positional binds mandated, hub-node fan-out bounded by the SQL LIMIT (the mitigation is the design itself). No over-reach.

## Knowledge Stewardship
- Queried: `/uni-query-patterns` (via `context_lookup topic=vision category=pattern`) for vision-alignment patterns -- found #3742 (optional-future-branch scope-addition WARN pattern — applied to the OQ-03 internal-caller assessment), #2298 / #3337 / #4617 (config-key / diagram-divergence / export-hash patterns — not applicable to this read-path feature). Also applied the memory caution against elevating convenience/defensive structure to vision level (ranking judged as a bounded display rule, not a new intelligence component).
- Stored: nothing novel to store -- the relevant cross-feature pattern (an open question resolved into in-scope test work = scope-addition WARN) is already captured as #3742 and applies here verbatim. The reframe-specific items (ranking-as-display-selection-rule; symmetric-canonicalize-before-cap-and-count) are feature-specific to vnc-037 and not yet a 2+-feature pattern; revisit at retro if a recurring "human-reframe narrows surface while raising correctness bar" pattern emerges.
