# Alignment Report: vnc-044

> Reviewed: 2026-07-05
> Artifacts reviewed:
>   - product/features/vnc-044/architecture/ARCHITECTURE.md
>   - product/features/vnc-044/architecture/ADR-001-two-axis-format-verbosity-contract.md
>   - product/features/vnc-044/architecture/ADR-002-context-graph-adoption.md
>   - product/features/vnc-044/specification/SPECIFICATION.md
>   - product/features/vnc-044/RISK-TEST-STRATEGY.md
> Scope source: product/features/vnc-044/SCOPE.md, product/features/vnc-044/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md + goal entries #4671, #5219 (self-learning), #5474 (integrity)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Serves the self-learning orientation workflow (goal:self-learning, #913); fixes a category-error + silent parse-and-drop. |
| Milestone Fit | PASS | Additive projection/serialization change on an existing tool (vnc-018/019/020 lineage). No future-milestone over-building; the ADR is a contract, not premature implementation. |
| Scope Gaps | PASS | All SCOPE items (ADR, context_graph impl, lean projection, backward-compat, #913 orientation) covered; AC-01..AC-09 traced. |
| Scope Additions | PASS | No unrequested scope. Notably clean for a tightly-constrained spec — the risk of architect-added scope (pattern #3742) did not materialize. |
| Architecture Consistency | WARN | ADR-001/ADR-002/ARCHITECTURE/SPEC/RISK are mutually consistent, but SPEC OQ-A still hedges the axis spelling as "placeholder pending ADR ratification" though ADR-001 §2 already ratified `detail` / `summary|full`. Stale doc-sync nit. |
| Risk Completeness | PASS | R-01..R-14 map SR-01..SR-09; UTF-8 slice-panic DoS, shared-enum blast radius, golden byte-equality, and the SR-09 doc-gate all covered. |

Verdict: **6 checks — 5 PASS, 1 WARN. No VARIANCE, no FAIL.** No item requires human approval; the three flagged concerns are aligned and addressed (details below). Two WARN-level items are raised for human awareness.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE goal and AC is addressed in the source docs. |
| Addition | ADR-001 §5 "per-tool field-set override" | Not literally in SCOPE, but a *narrowing/flexibility* clause that mitigates SR-03; consistent with D-6's "documented per-tool exceptions allowed." Not a scope expansion — accept. |
| Simplification | AC-06 orientation delivers structure, not a delivery-status tally | Rationale: D-3/SR-09 — projection carries lifecycle `EntryRecord.status` only; delivery-status promotion is named follow-up #3. Explicitly in-scope-as-defined, not a silent cut. |
| Simplification | Graph output stays JSON-only; `format=markdown` rejected | Rationale: Non-Goal 2 / D-4 — no graph-markdown renderer exists; rejection is loud, not a silent fallback. Human-settled. |

## Variances Requiring Approval

**None.** No VARIANCE or FAIL findings. The three items the spawn asked to scrutinize were evaluated and found consistent with the vision; see Detailed Findings. Two WARN items are logged for awareness, not approval.

## Detailed Findings

### Vision Alignment
The motivating incident (GH #913) is a vision-root orientation pull — the exact self-learning/orientation workflow the graph tool exists to serve (goal #5219 self-learning; #913 carries the `goal:self-learning` label). Today the traversal returns ~135KB byte-identical regardless of `format`, overflowing an agent's context window and forcing out-of-band parsing. The lean-by-default projection makes the traversal agent-consumable in one call, replacing multi-step choreography. This advances self-learning's "surface the right knowledge in an agent-consumable form" and proactive-delivery's minimum-context posture.

Secondary integrity/correctness value: the feature fixes a genuine category error (`format` fused serialization with verbosity) and a silent no-op (`format` parsed at `graph_read.rs:59-81`/`ResponseFormat` then discarded at `graph_read.rs:251`). Removing a silently-ignored parameter is a correctness/consistency win consistent with the "one mental model" aspiration in PRODUCT-VISION lines 21-25 and the integrity goal's contradiction-free-serving intent (#5474). No architectural principle is violated — shared `EntryRecord`/`EdgeRecord`/`ResponseFormat` are untouched (Principle-neutral); the change is graph-local.

**Flagged item 1 — suite-wide ADR contract with a single adopter (WARN, not variance).** ADR-001 binds the *entire* context-tool suite (axis names, `summary|full` values, default-summary, the 256-byte constant, the summary field set) while vnc-044 implements and exercises it only for `context_graph`. SR-03 rates this High/Med: an unexercised contract can drift from what later adopters (`context_get`, `context_search`, mutations, `context_briefing`) actually need. Assessment: this is a legitimate architectural move, not over-reach. It aligns with the vision's "suite converges on one mental model" and prevents the graph adoption from being a one-off hack. The over-building risk is genuinely mitigated in-artifact: ADR-001 §5 makes field sets **per-tool overridable** ("fixes the shape and shared constants, not a one-size-fits-all field list"), and §Single-source primitives keeps 256 / `Detail` / `content_preview` in one module so downstream cannot re-drift them. This is the disciplined opposite of the "avoid overstating defensive/structural claims" failure mode (memory: avoid-overstating-defensive-structure) — the ADR scopes its own claims and names the temporary suite inconsistency as a designed, disclosed state (ADR-001 §Consequences "temporarily inconsistent by design"). WARN rationale for human awareness: locking suite-wide *values* (spelling, 256, canonical field set) before a second tool exercises them carries a real revision cost if a later adopter needs different values; the human should confirm they accept locking these now versus after ≥2 adopters. Recommendation: accept; treat the first non-graph adopter as the ADR's ratification-under-load, and expect a possible ADR-001 amendment then.

### Milestone Fit
Appropriately scoped as an incremental enhancement to a shipped tool. Traversal semantics (BFS, `max_depth`, `max_nodes`, supersession, truncation — vnc-018/019/020) are explicitly unchanged (NFR-6, Non-Goal 4). The new axis is additive under the `GraphParams` layout lock (ADR-003, #4490/#4491). No future-milestone capability is built ahead of need: the four dependent follow-ups (other-tool migration, crt-057 fold-in, delivery-status promotion, graph-markdown renderer) are explicitly deferred and tracked, not smuggled in. Authoring the ADR now is documentation of a contract the graph adoption must reference — not premature implementation of future tools. PASS.

### Architecture Review
ADR-001 (suite contract) and ADR-002 (graph adoption) cleanly separate the suite-wide contract from the graph-specific decisions; ARCHITECTURE.md wires both to concrete seams; SPECIFICATION.md restates them as FR-1..FR-12 / AC-02..AC-09 with a traceability table. Consistency checks that held:
- Axis spelling ratified once (ADR-001 §2 `detail` / `summary|full`) and single-sourced; SR-03's "ratify once, never restate in the issue body" is respected in the ADR.
- Projection is a distinct type (`NodeSummary` + `GraphSummaryProjection`) in a new module; shared `EntryRecord`/`EdgeRecord` get **no** `skip_serializing_if` (SR-06/SR-07, C-2/C-3). Shared `ResponseFormat`/`parse_format` untouched — graph uses its own `resolve_graph_output`. This holds the "graph-local code change" line the scope demands.
- 256 single-sourced as `CONTENT_PREVIEW_BYTES` in `response/verbosity.rs` (SR-03, C-9).
- Pre-existing over-limit debt (`graph_read_subgraph.rs` at 742 lines) is correctly flagged as out-of-scope cleanup, and the new projection is routed to a new module rather than added to it (SR-08).

**WARN (doc-sync):** SPECIFICATION.md front-matter and OQ-A still describe `detail`/`summary`/`full` as "placeholder pending ADR ratification (OQ-A)," but ADR-001 §2 has since ratified exactly that spelling. The placeholder hedging is now stale and, per pattern #3337 (informal wording diverging from the ratified source causing test/assertion drift), should be reconciled — replace the OQ-A hedge with a reference to ADR-001 §2 as ratified. Non-blocking; the semantics already match, so no rework of logic is implied.

**Flagged item 2 — default full→summary behavior change (aligned; PASS).** The default flip (FR-3, AC-05, SR-04) is a genuine behavior change to existing `context_graph` callers that passed no verbosity axis and relied on full output. It is not a scope variance: it is human-settled (SCOPE D-2), explicitly marked an "accepted behavior change," backward-compat is preserved for legacy `format=summary` callers (FR-9/AC-07) and full output stays reachable byte-for-byte via `detail=full` (FR-10/AC-04, golden-tested R-04), and the divergence is disclosed in the tool description during the migration window (FR-12, ADR-002 §7). This directly serves the self-learning orientation use case (minimum-context-by-default). Aligned. The only residual is the deliberate, disclosed temporary suite inconsistency (graph defaults `summary`; other tools still default `full`) — acceptable per ADR-001 §3 with the per-tool disclosure requirement.

### Specification Review
FR/AC set is complete against SCOPE AC-01..AC-09 and the SR register. FR-6/FR-7 pin the UTF-8-floored preview and the byte-compare `content_truncated` contract; NFR-2 forbids shared-type mutation; NFR-4 correctly scopes the win to payload/context size, not DB cost (SR-01) — no over-claimed performance benefit. The spec carries the SR-09 caveat as FR-12 (tool description must state lifecycle-not-delivery status) and in the Domain Models section ("the single most important nuance"). Only nit is the stale OQ-A placeholder noted above.

### Risk Strategy Review
Coverage is proportional and complete. Critical risks (R-01 UTF-8 slice-panic, R-02 truncated-flag false-negative, R-03 all-five-modes projection + envelope-metadata preservation, R-04 full byte-equality golden) map to the security-relevant and regression-relevant hotspots. Security section correctly identifies the request-triggered panic/DoS on attacker-influenceable `content` straddling byte 256 as the highest-blast-radius issue and mandates the char-boundary floor + boundary table. Shared-enum blast radius (R-06, #4831) guarded by a `--no-run` compile check plus code-review gate.

**Flagged item 3 — SR-09 lifecycle-vs-delivery status gap, honestly carried (aligned; PASS, and exemplary).** The #913 orientation use case wants a capability delivery-status tally (`missing|partial|proven|claimed`), but the projection carries only lifecycle `EntryRecord.status` — so a capability subgraph returns `active` for every node. Left implicit, this would let the feature *look* like it answers #913 while answering a different question. The artifacts do the opposite of overstating: the gap is stated plainly in ADR-001 §7 ("make this loud"), ADR-002 §7, the tool description (FR-12), AC-06's own criterion text, the Domain Models section, and R-11 as a documentation/expectation gate that explicitly instructs testers **not** to treat delivery-status absence as a defect. This is precisely the posture the "avoid overstating defensive/structural claims" and "honestly-carried gap" discipline call for: ship the enabling half (consumable payload), disclose the limitation, name the follow-up (#3), do not imply the tally is delivered. Fully aligned with both the self-learning goal (the payload win is real and unlocks one-call orientation) and the integrity goal's contradiction-free / accurate-serving intent (#5474). No action required beyond holding the doc-gate at delivery.

## Knowledge Stewardship
- Queried: `/uni-query-patterns` (context_search) for vision alignment / scope-addition patterns tagged `vision` — found #3742 (architecture/risk diverging from scope deferral → WARN pattern), #3337 (informal doc wording diverging from ratified source → assertion drift), #2298 (config semantic divergence from vision example). #3742 and #3337 applied directly (scope-addition check came back clean; #3337 informs the SPEC OQ-A stale-placeholder WARN).
- Stored: nothing novel to store — the vnc-044 findings are feature-specific (a clean, well-disciplined feature with only doc-sync nits), not a recurring generalizable misalignment. The "honestly-carried gap disclosed across ADR + tool-desc + AC + risk-gate" is already captured as a project posture (memory: avoid-overstating-defensive-structure); no new pattern warranted.
