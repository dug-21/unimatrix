# Gate 3a Report: nan-018

> Gate: 3a (Design Review — pseudocode + test plans)
> Date: 2026-06-09
> Result: PASS (re-validation, iteration 1 — stewardship remediation accepted)
>
> Original result: REWORKABLE FAIL (single failing check: Knowledge Stewardship).
> Re-validation 2026-06-09: stewardship blocks added to pseudocode/OVERVIEW.md and
> test-plan/OVERVIEW.md. Verified present, adequate, and grounded (all cited IDs resolve
> to real Unimatrix entries with matching rationale). Sole failure closed → **PASS**.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | Components, boundaries, ADR decisions all honored in pseudocode |
| 2. Specification coverage | PASS | Every FR/NFR/AC maps to pseudocode; no scope additions beyond authorized 5th shape |
| 3. Risk coverage (test plans) | PASS | R-01…R-18 each map to ≥1 named test scenario; truth tables complete |
| 4. Interface consistency | PASS | Integration Surface names/types/signatures used exactly; OVERVIEW shared types match per-component usage |
| 5. Knowledge stewardship compliance | **PASS** (after rework) | `## Knowledge Stewardship` blocks now present in pseudocode/OVERVIEW.md (7 `Queried:` IDs) and test-plan/OVERVIEW.md (6 `Queried:` IDs), each with cited rationale + a `Stored:` nothing-novel reason. All cited IDs verified to resolve to real Unimatrix entries with matching titles. |
| Wave-1 three backstop tests present, not deferred | PASS | R-09 audit, R-04 sensitivity matrix, R-15 non-vacuous AC-14 all planned, marked "may NOT be deferred" |
| Wave-2 zero-code-coupling (NFR-04) | PASS | docs.md / band3 reference Wave-1 conceptually only; Wave-1-alone test owned in corpus-loader |
| Clamp-coupling (ceiling = config clean_replacement) | PASS | engine-penalty.md clamps to `params.clean_replacement`, not const; verified against live graph.rs:531 |
| Two-site / background.rs-not-touched | PASS | search-threading.md threads only :727/:729; background.rs explicitly excluded; verified against live search.rs |
| OQ #1 (confidence_dimension_names) | PASS (delivery-time read) | Grounded in real `ConfidenceWeights` (6 fields); access pattern fixed, names deferred — sound |
| OQ #2 (R-04 manifest column list) | WARN | Column list left as a named placeholder + delivery read; acceptable but enumerate-at-pseudocode would be safer |
| OQ #3 (multiplier per-field-override mechanism) | WARN | Equals-default heuristic is ambiguous; pseudocode flags it and offers the sound `Option<f64>` alternative |

**Result (re-validation): PASS** — the sole failing check (knowledge stewardship) is closed.
All 10 technical design checks remained PASS and were not re-litigated. The three flagged
open questions are delivery-time reads or non-blocking WARNs, not design defects (carried
forward to delivery). No SCOPE FAIL conditions.

**Original result: REWORKABLE FAIL** — single failing check (knowledge stewardship).

---

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS
**Evidence**:
- Component decomposition in `pseudocode/OVERVIEW.md` (§Components) matches ARCHITECTURE §2 one-to-one: penalty-config, engine-penalty, search-threading, trust, cost, corpus-loader, corpus-fixtures, shape-hash, report-extensions, docs, band3.
- Technology choices consistent with ADRs: ADR-001 (penalty config threaded via `graph_penalty_with`), ADR-002 (branch (b) embed-model-in-hash, ordered versioned manifest, SHA-256), ADR-003 (token-weighted cost, faithful subword tier + word×1.3 fallback, char/4 rejected), ADR-004 (property ground truth + `Assertion` class with exactly the three Wave-1 variants), ADR-005 (recommendation-only Band-3, two-corpus documented-not-typed), ADR-006 (eval-only penalty boundary stated in penalty-config.md and docs.md).
- Engine-penalty body matches live `graph.rs` branch order exactly (orphan → dead_end → partial → clean depth-1 → hop-decay depth≥2 → defensive dead_end); verified against source.

### Check 2 — Specification coverage
**Status**: PASS
**Evidence**: FR-01…FR-31, NFR-01…NFR-08 each have corresponding pseudocode:
- FR-01/02/03/04 → penalty-config (7 levers + multiplier) + search-threading (two sites) + engine-penalty.
- FR-06…FR-11 → trust-metric (`evaluate_trust`, three property types, asymmetric semantics).
- FR-12/12a/13/14 → cost-metric (Σ token_proxy, ε=0.0 advisory, k secondary, proxy labeling).
- FR-15…FR-20 → corpus-loader + corpus-fixtures (five shapes, alias-resolution, two-corpus).
- FR-21…FR-23 → shape-hash (deterministic ordered manifest, severity split, embed-in-hash).
- FR-24…FR-30 → docs.md + band3-recommendation.md (Wave 2).
- FR-31 → corpus-fixtures AC-14 §.
- No scope additions beyond the optional 5th "dead-end chain" fixture shape, which the ALIGNMENT-REPORT already justified (SCOPE AC-06 "at minimum"; needed to keep DEAD_END_PENALTY sweep non-degenerate).

### Check 3 — Risk coverage (component test plans)
**Status**: PASS
**Evidence**: `test-plan/OVERVIEW.md` §2 master table maps every R-01…R-18 to a named component test plan and headline test. Spot-checks:
- R-11 (vacuous-pass, the highest-likelihood correctness bug): `test-plan/trust-metric.md` carries a complete truth table per property type, with `test_rank_below_b_absent_fail` named explicitly as the load-bearing asymmetry sentinel.
- R-01/R-02 (default-shift / dual-default): triangulation across engine-penalty + penalty-config + search-threading, plus the enumerated-site grep guard and empty-TOML byte-identity at engine, config-resolution, and service levels.
- R-03 (hash non-determinism): N≥100 in-process, permuted-input, cross-process, float-format — all four scenarios planned in shape-hash test plan.
- Edge cases (RISK-TEST §Edge Cases) and Security risks (path traversal, range validation, deserialization) are each covered (corpus-loader, penalty-config test plans).
- Test conventions codified: assert-the-value-not-the-path (#3548), non-trivial round-trip (#3557), no-literal-IDs (#703).

### Check 4 — Interface consistency
**Status**: PASS
**Evidence**: `pseudocode/OVERVIEW.md` §Shared types reproduces the IMPLEMENTATION-BRIEF Integration Surface verbatim. Per-component usage is consistent:
- `GraphPenaltyParams` (7 fields, Copy, Default=consts) identical in engine-penalty + OVERVIEW + search-threading.
- `GraphPenaltyConfig` (mirror + `multiplier: Option<f64>`, `#[serde(default)]`) identical in penalty-config + OVERVIEW.
- `ExpectedAssertions` / `EntryRef = String` / `TrustOutcome` / `ProfileResult` additions (`cost_tokens: f64`, `trust: TrustOutcome`) consistent across trust-metric, corpus-loader, report-extensions, OVERVIEW.
- `evaluate_trust(&[ScoredEntry], &ExpectedAssertions, &AliasMap) -> TrustOutcome` signature consistent (note: the brief's §Function Signatures showed `alias_map: &AliasMap`; trust-metric and corpus-loader agree — `AliasMap` is the producer's type, defined in corpus-loader). No contradictions found between component files. Data flow (corpus loader produces alias_map → trust consumes; shape stamp ↔ corpus-fixtures) is coherent.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS (after rework — original FAIL closed)

**Original finding (REWORKABLE FAIL)**: No `## Knowledge Stewardship` block existed in any
Stage-3a artifact; no `Queried:` evidence for the design phase.

**Re-validation (2026-06-09)** — both blocks added and verified:
- `pseudocode/OVERVIEW.md` §Knowledge Stewardship (lines 131–142): `Queried:` lists 7 cited
  IDs (#4897 ADR-001 clamp coupling, #4895 ADR-002 hash manifest, #4896 ADR-003 token-proxy,
  #4075 submodule convention, #4888 positive-relevance baseline, #4064 dual-default trap,
  #2610 hash determinism), each tied to a specific invariant/component it shaped. `Stored:`
  carries a reasoned nothing-novel rationale (single-feature instances of recorded ADRs;
  reassess at retro). Deviations: none.
- `test-plan/OVERVIEW.md` §Knowledge Stewardship (lines 128–138): `Queried:` lists 6 cited
  IDs (#4895/#4898 nan-018 ADRs, #3557 dual-direction serde, #3548 assert-the-value lesson,
  #3526 dual-type-copy round-trip, #4070 multi-site bit-for-bit, #2610 hash determinism),
  each tied to a risk→test mapping. `Stored:` carries a reasoned nothing-novel rationale
  with an explicit named candidate (the "instrument-measures-not-executes" lens) correctly
  held as a one-instance observation pending retro.

**Groundedness check**: spot-verified all 9 distinct cited IDs via `context_get` — every one
resolves to a real Unimatrix entry whose title matches the cited rationale (no fabricated
IDs). Blocks are adequate (cited IDs + a reasoned `Stored:`/nothing-novel line), not bare
placeholders. Per gate rule, this clears the WARN condition too ("Present but no reason
after nothing novel = WARN").

The sole failing check is closed. Gate 3a result is now **PASS**.

---

## Assessment of the Three Flagged Open Questions

These were assessed for whether any is a genuine design defect that must block.

**OQ #1 — `confidence_dimension_names()` concrete field list (delivery-time read).**
Not a defect. The pseudocode (shape-hash.md) fixes the access pattern (sorted field names from the live `ConfidenceWeights`/`ConfidenceParams` struct) and defers only the concrete names. Verified the struct is real: `ConfidenceWeights` (infra/config.rs:235) has 6 fields (base, usage, fresh, help, corr, trust); `ConfidenceParams` (engine/confidence.rs:141) also exists. Because only deterministic field-name enumeration is needed for the manifest, deferring the literal list to delivery is sound and carries no hash-correctness risk (the R-04 sensitivity matrix will exercise each declared dim). PASS as a delivery-time read.

**OQ #2 — R-04 manifest column list (must be fixed so sensitivity tests enumerate correctly).**
WARN, not a blocker. shape-hash.md leaves `RETRIEVAL_RELEVANT_COLUMNS` as an enumerated-but-named-placeholder list ("status, supersedes/superseded_by, category, confidence-bearing columns"). The test plan (shape-hash test plan, R-04 matrix) parameterizes one test per declared column, so the test scaffold adapts to whatever the delivered list is. The residual completeness question is correctly routed to the **named human delivery gate** (§7.3 LOCKED), which no test can close. Fixing the exact column list at pseudocode time would be marginally safer (it removes a delivery-time judgment call from the agent), but the design explicitly assigns that judgment to a human reviewer, so leaving it as a declared placeholder is consistent with the locked design. Acceptable; noted for delivery.

**OQ #3 — Multiplier "per-field override wins" mechanism (ADR-001 requires per-field overrides win).**
WARN, not a blocker — the pseudocode is *sound but flags its own ambiguity and offers the clean fix*. penalty-config.md implements override-detection via an **equals-default heuristic**: the multiplier scales a severity only if that field currently equals its const default (i.e. presumed unset). This correctly satisfies ADR-001's "per-field override wins" in every case **except** a deliberate set-to-the-default-value, which the heuristic mis-reads as "unset" and therefore scales. The pseudocode explicitly calls this out (penalty-config.md §Multiplier semantics) and recommends the unambiguous alternative — per-field `Option<f64>` to distinguish "set to default" from "unset" — flagging delivery to prefer it and document the chosen mechanism in Band-2. The R-13 precedence test (`test_with_rate_config_per_field_override_wins_over_multiplier`) will catch a non-default override; it would NOT catch the set-to-default edge under the heuristic. This is a known, documented, low-severity ambiguity with a stated sound resolution path — it does not block Gate 3a. Recommend delivery adopt the `Option<f64>` mechanism (removes the edge entirely) and that the R-13 test plan add a "set-to-default explicit override" case if the heuristic is retained.

None of the three is a genuine design defect requiring a block.

---

## Rework Required — RESOLVED (iteration 1)

| Issue | Which Agent | What to Fix | Status |
|-------|-------------|-------------|--------|
| Missing `## Knowledge Stewardship` block for Stage-3a design phase (no `Queried:` evidence) | uni-pseudocode + uni-test-planner (or Design Leader) | Add a stewardship block with `Queried:` entries naming consulted patterns/procedures and a `Stored:`/nothing-novel line. | **CLOSED** — blocks added to pseudocode/OVERVIEW.md (7 IDs) and test-plan/OVERVIEW.md (6 IDs); all cited IDs verified real; reasoned nothing-novel rationale present. |

### Non-blocking carry-forward to delivery (not rework; do not re-spawn for these)
- OQ #3: prefer per-field `Option<f64>` over the equals-default heuristic for multiplier override-detection; if the heuristic is kept, add a set-to-default-value test case to R-13.
- OQ #2: the named human R-04 column-manifest completeness review is a delivery gate (§7.3) — ensure it is scheduled before delivery acceptance.

## Scope Concerns
None. Scope, technology, and architecture all support the requirements. No SCOPE FAIL.
