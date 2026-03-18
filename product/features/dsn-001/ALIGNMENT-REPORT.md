# Alignment Report: dsn-001

> Reviewed: 2026-03-18 (re-review after preset system scope expansion)
> Artifacts reviewed:
>   - product/features/dsn-001/architecture/ARCHITECTURE.md
>   - product/features/dsn-001/specification/SPECIFICATION.md
>   - product/features/dsn-001/RISK-TEST-STRATEGY.md
> Scope source: product/features/dsn-001/SCOPE.md (updated — preset system added)
> Scope risk source: product/features/dsn-001/SCOPE-RISK-ASSESSMENT.md (updated)
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Preset system resolves VARIANCE-1; all prior WARNs closed |
| Milestone Fit | PASS | Wave 0 scope maintained; no W1/W2/W3 capabilities introduced |
| Scope Gaps | PASS | All SCOPE.md goals fully addressed in source documents |
| Scope Additions | WARN | `[confidence]` field semantic differs from vision's W0-3 example — namespace divergence, not a delivery blocker |
| Architecture Consistency | PASS | All SCOPE-RISK-ASSESSMENT risks resolved; ADR-005/006 close SR-02/SR-09/SR-10/SR-11/SR-13 |
| Risk Completeness | PASS | RISK-TEST-STRATEGY covers 22 risks with 95+ scenarios; preset-specific risks R-01–R-12 fully specified |

**Overall: WARN — one documentation-level namespace divergence to acknowledge; no delivery blockers.**

---

## Prior Variances — Disposition

All three WARNs from the first alignment report are resolved:

| Prior Variance | Resolution | Status |
|---------------|------------|--------|
| VARIANCE-1: Confidence weights not in `ConfidenceParams` — W3-1 cold-start path broken | Preset system extends `ConfidenceParams` to 9 fields (ADR-001); `resolve_confidence_params()` populates all six `w_*` fields; AC-27 mandates W3-1 reads this struct directly (ARCHITECTURE §W3-1, SCOPE.md §AC-27) | CLOSED |
| VARIANCE-2: `[cycle]` label doc-fix accepted as sufficient | Confirmed: hardcoded rename of `context_retrospective` → `context_cycle_review` and `CycleParams.topic` doc neutralisation are delivered as FR-11/FR-12; `CycleConfig` stub removed per ADR-004 update | CLOSED |
| VARIANCE-3: `default_trust = "permissive"` default correctness | Confirmed as correct default consistent with W0-2 deferral rationale; source documents are internally consistent | CLOSED |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | None | All SCOPE.md goals (1–9) and acceptance criteria (AC-01 through AC-27) are present in source documents |
| Addition | `[confidence]` semantic payload differs from vision's W0-3 example | Vision's W0-3 `[confidence] weights` shows lambda/coherence-gate weights (`freshness=0.35, graph=0.30, contradiction=0.20, embedding=0.15`); source docs' `[confidence] weights` carries the six scoring factor weights (`w_base`, `w_usage`, etc.). Same TOML key, different semantics. See Variances section. |
| Simplification | Coherence gate lambda weights excluded | SCOPE.md non-goal, consistent with first review. Source docs confirm exclusion; `coherence.rs` constants remain hardcoded. Justified: operators cannot tune these without ML expertise. No change from prior report. |
| Simplification | `[cycle]` runtime label config excluded | CycleConfig stub removed (ADR-004 update). Hardcoded rename + doc-fix accepted as sufficient. No change from prior report. |
| Simplification | Bootstrap agent list not configurable | Only `default_trust` and `session_capabilities` externalised. Full bootstrap list configurability explicitly out of scope. Consistent across all documents. |

---

## Variances Requiring Approval

### WARN-1: `[confidence]` TOML key semantic divergence from vision's W0-3 example

**What**: The vision's W0-3 config block (PRODUCT-VISION.md line 261–264) shows:

```toml
[confidence]
# Lambda dimension weights — previously hardcoded in confidence.rs
weights = { freshness = 0.35, graph = 0.30, contradiction = 0.20, embedding = 0.15 }
```

These are the four **coherence gate lambda weights** used in `compute_lambda()` (`confidence_freshness`, `graph_quality`, `embedding_consistency`, `contradiction_density`).

The source documents' `[confidence]` section (SCOPE.md §Config Schema, ARCHITECTURE §ConfidenceConfig, SPECIFICATION §FR-03) carries the six **confidence scoring factor weights**:

```toml
[confidence]
weights = { base = 0.16, usage = 0.16, fresh = 0.18, help = 0.12, corr = 0.14, trust = 0.16 }
```

These are a different set of weights — they feed `compute_confidence()`, not `compute_lambda()`. The lambda coherence weights are explicitly out of scope (SCOPE.md non-goals, third bullet from last).

**Why it matters**: The `[confidence]` namespace in TOML now carries a different semantic payload than what the product vision described. If a future feature attempts to externalise the lambda weights by adding to `[confidence]`, it would collide with the six scoring factor weights already occupying `[confidence].weights`. W3-1's cold-start design references `W0-3 [confidence] weights` (vision lines 705-706, 708-712) — the vision's author intended the lambda weights; the source docs deliver the scoring factor weights. Both are needed for W3-1. If W3-1's design takes the vision literally and looks for `[confidence].weights = { freshness, graph, contradiction, embedding }`, it will find a differently-structured field.

**Important nuance**: The source docs' six scoring factor weights are what W3-1 actually needs for cold-start (the GNN outputs `[w_base, w_usage, w_fresh, w_help, w_corr, w_trust]` — vision line 698). The vision's W3-1 section correctly describes the cold-start requirement. The vision's W0-3 config example shows the wrong weights (lambda weights instead of scoring factor weights). The source documents are arguably more correct than the vision example on this point.

**Recommendation**: Accept the source documents' interpretation. Update the vision's W0-3 config block to show the six scoring factor weights (matching the source docs) rather than the lambda weights. Lambda weight externalization (if ever needed) should target a distinct TOML key (e.g., `[coherence]`). This is a documentation correction to the vision, not a code change.

No delivery blocker. The source documents are internally consistent and correctly satisfy W3-1's cold-start requirement per AC-27.

---

## Preset System Alignment Assessment

The following questions were evaluated per the review mandate.

### Q1: Lifecycle vocabulary ("authoritative", "operational", "empirical", "collaborative") vs. domain-agnostic framing

**Finding: PASS**

The vision's domain-agnostic principle is "Any knowledge-intensive domain... runs on the same engine, configured not rebuilt." The preset names are knowledge *lifecycle archetypes*, not domain names. Naming them by knowledge behaviour (authoritative, operational, empirical, collaborative) rather than by industry (legal, SRE, science, dev) is *more* domain-agnostic than the alternatives.

Evidence of vision alignment:
- `authoritative` — encodes long-lived high-trust knowledge regardless of domain (legal, policy, standards). A legal operator and an aviation-standards operator both recognise their knowledge in this preset.
- `operational` — encodes action-oriented time-sensitive knowledge (runbooks, incident procedures, SOPs) without naming any specific industry.
- `empirical` — encodes measurement-derived knowledge (sensors, metrics, feeds) applicable to environmental monitoring, scientific research, SRE metrics alike.
- `collaborative` — encodes team-built knowledge under revision (dev, research), matches the current compiled defaults.

The vision explicitly lists "environmental monitoring, SRE operations, scientific research, regulatory compliance" as target domains (story section, line 15). All four map cleanly onto these archetypes without the preset names being dev-specific.

### Q2: Preset system's relationship to W3-1 GNN cold-start

**Finding: PASS — VARIANCE-1 fully resolved**

The vision's W3-1 section (lines 705-712) states:

> "A missing or stale weight vector degrades gracefully to the config-defined defaults (W0-3 `[confidence] weights`). W3-1's cold-start initializes from the weights in `[confidence] weights` config, not hardcoded dev-domain constants."

The source documents satisfy this requirement:
1. `ConfidenceParams` is extended to nine fields including all six `w_*` weights (ADR-001, ARCHITECTURE §2, SPECIFICATION FR-04).
2. `resolve_confidence_params()` is the single site that converts preset selection to a populated `ConfidenceParams` (ADR-006, ARCHITECTURE §1, SPECIFICATION FR-10).
3. AC-27 explicitly mandates: "W3-1 reads this struct for GNN cold-start without any additional config parsing."
4. `ConfidenceParams` in `unimatrix-engine` is the natural W3-1 integration point — W3-1 will add `Option<LearnedWeights>` to the struct without changing any call site using `Default` (ARCHITECTURE §2, SPECIFICATION §Domain Models/ConfidenceParams).

A non-dev domain operator selecting `preset = "authoritative"` cold-starts W3-1 with w_trust=0.22 (dominant), w_fresh=0.10 (minimal) — a legal domain's correct starting point. The GNN then refines from calibrated starting weights rather than the dev-domain collaborative defaults.

The prior gap (weights never entering `ConfidenceParams`) is resolved. The W3-1 prerequisite is satisfied.

### Q3: `custom` preset exposing raw weights — vision principle conflict

**Finding: PASS** (one minor observation noted as part of WARN-1 above)

The vision's security section for W0-3 states: "Confidence weights must sum to ≤ 1.0 and each weight must be in [0.0, 1.0]; reject config on violation." The source documents use `(sum - 0.92).abs() < 1e-9` as the invariant (ADR-005, SPECIFICATION §Constraints #2, RISK-TEST-STRATEGY R-03/R-09), not `≤ 1.0`. This discrepancy is between the vision's security note and the ADR. The ADR governs delivery (confirmed by SPECIFICATION and RISK-TEST-STRATEGY). The vision's `≤ 1.0` comment was approximate and is superseded by ADR-005. Not a functional problem.

The `custom` preset concept is consistent with the vision's principle that operators who have domain science justification can use raw weights. The preset system makes `custom` the escape hatch rather than the primary interface — which aligns with the vision's intent that operators "identify their knowledge type, not ML weights." The design choice (named presets first, custom as expert path) is more aligned with the vision than the vision's own config example, which shows raw weights with no higher-level abstraction.

### Q4: Weight values in ADR-005 vs. vision's domain examples

**Finding: PASS**

The vision's W0-3 description (line 291-295) gives directional guidance: "legal knowledge base needs high w_trust, low w_fresh. An air quality deployment needs the inverse."

Against the ADR-005 locked weight table (ARCHITECTURE §Preset Weight Table, SPECIFICATION AC-23):

| Domain example | Matching preset | w_trust | w_fresh | Vision direction |
|---------------|----------------|---------|---------|-----------------|
| Legal / policy | `authoritative` | 0.22 (highest across all presets) | 0.10 (lowest) | High trust, low fresh — MATCH |
| Air quality / sensors | `empirical` | 0.20 | 0.34 (highest) | High fresh, lower trust — MATCH |
| SRE / incidents | `operational` | 0.10 (lowest) | 0.24 | Action-oriented, time-sensitive — CONSISTENT |
| Dev / research | `collaborative` | 0.16 | 0.18 | Balanced — MATCH (compiled defaults) |

All four vision domain examples map correctly onto the preset weight table. The ordering relationships the vision implies are satisfied by the numeric values.

---

## Detailed Findings

### Vision Alignment

The core vision principle — "any knowledge-intensive domain runs on the same engine, configured not rebuilt" — is now fully operationalised by this feature. The preset system resolves the most significant prior gap (confidence weights domain-coupled through compiled defaults) without requiring operators to understand ML weight vectors.

Vision Critical Gaps addressed by this feature (from PRODUCT-VISION.md §The Critical Gaps):
- "Freshness half-life hardcoded at 168h" — CRITICAL — addressed via preset built-in values + `[knowledge] freshness_half_life_hours` override (FR-07, AC-04)
- "`lesson-learned` category name hardcoded in scoring" — CRITICAL — addressed by `boosted_categories` externalisation (FR-06, AC-03)
- "SERVER_INSTRUCTIONS const uses dev-workflow language" — HIGH — addressed (FR-08, AC-05)
- "Initial category allowlist hardcoded" — HIGH — addressed (FR-05, AC-02)
- "`context_retrospective` tool name is SDLC-specific" — MEDIUM — addressed (FR-11, AC-13)
- "Confidence weights hardcoded — cannot adapt to domain or usage" (Intelligence & Confidence table) — HIGH — addressed via preset system (FR-03/FR-04, AC-22–27)

The only vision Critical Gap in the domain-coupling table not addressed remains "Lambda dimension weights hardcoded" — these are the coherence gate weights, not the scoring factor weights. This is a deliberate, documented exclusion (SCOPE.md non-goals). The exclusion is correct: these weights feed `compute_lambda()`, not `compute_confidence()`, and W3-1 does not reference them for cold-start.

The vision's security requirements for W0-3 are all satisfied by FR-13/FR-14/FR-15 and their corresponding acceptance criteria (AC-08 through AC-20, AC-24–26). The RISK-TEST-STRATEGY provides full coverage of all five security risk surfaces.

### Milestone Fit

dsn-001 remains correctly positioned as a Wave 0 prerequisite after scope expansion.

The preset system addition does not introduce any Wave 1+ capabilities. Evidence:
1. No new MCP tools, no schema changes, no ML components (ARCHITECTURE §Overview).
2. No GNN training infrastructure introduced — `ConfidenceParams` is extended as a data carrier for W3-1, not as an ML subsystem.
3. The W3-1 forward-compatibility design (`Option<LearnedWeights>` slot noted in ARCHITECTURE §2 and SPECIFICATION §Domain Models) is a structural anticipation of a future extension, not pre-implementation of W3-1 logic.
4. The hook path, bridge mode, export/import subcommands, and background tick are correctly excluded from config loading (ARCHITECTURE §Config distribution, SCOPE.md §Server Startup Path).

Milestone discipline is maintained.

### Architecture Review

The architecture is self-consistent across all six ADRs. Post-expansion additions are sound:

**ADR-005 (Preset Enum and Weight Table)**: The enum design (`#[serde(rename_all = "lowercase")]` with no catch-all variant) is the correct approach — invalid strings fail at deserialization before `validate_config()` runs, providing the earliest possible rejection. The locked weight table (ARCHITECTURE §Preset Weight Table) provides a single authoritative source for delivery. The constraint that all rows must sum to exactly 0.92 (not ≤ 1.0) is consistently stated and the SR-10 mandatory test enforces the `collaborative` row equality invariant.

**ADR-006 (Preset Resolution Pipeline)**: Single resolution site in `resolve_confidence_params()` with an explicit precedence chain is the right architectural choice. The `freshness_half_life_hours` precedence table (ARCHITECTURE §`freshness_half_life_hours` Precedence Chain) covers all four cases including the `custom`+absent startup-abort case. No ambiguity remains about which value wins.

**`from_preset(Custom)` panic design**: The deliberate panic on direct `Custom` invocation (enforced by design, documented in SPECIFICATION Constraint #3, tested by R-18) is an acceptable pattern given that `resolve_confidence_params()` is the only valid call site. Code review is the enforcement mechanism; no type-system guard exists, which is noted but acceptable.

**ContentScanner ordering constraint** (ARCHITECTURE §Integration Points): The explicit `let _scanner = ContentScanner::global()` warm call at the top of `load_config` with a required comment is a runtime ordering guarantee. This is a delivery-team responsibility (ARCHITECTURE Constraint #10), not a structural guarantee. RISK-TEST-STRATEGY R-13 correctly categorises this as a code-review gate.

### Specification Review

The specification is complete and precisely specified. All 27 acceptance criteria from SCOPE.md are present and traceable. The preset additions (FR-03, FR-04, FR-10, AC-22–27) are fully coherent with the base config spec. Notable precision:

- The `[knowledge] freshness_half_life_hours` precedence matrix (AC-25) covers all four combinations explicitly, matching ADR-006.
- The weight sum invariant (SPECIFICATION §Constraints #2) correctly uses `(sum - 0.92).abs() < 1e-9` and flags the SCOPE.md config-comment error (`≤ 1.0`) as incorrect.
- The SR-05 rename checklist covers 31 specific file locations across 14 files — comprehensive and correct.
- The domain model for `ConfigError` enumerates all required variants including preset-specific ones (`CustomPresetMissingWeights`, `CustomPresetMissingHalfLife`, `CustomWeightSumInvariant`).

One specification item from the prior review remains open: empty `categories = []` boundary behaviour is not defined. SPECIFICATION §Domain Models and §FR-05 describe the per-element validation constraints but do not specify a minimum count. The RISK-TEST-STRATEGY EC-01 flags this as requiring documentation. **This is a minor open item for the delivery team to resolve** (accept empty list as degenerate-but-valid, or reject with a minimum-count constraint). No acceptance criterion exists for this case.

### Risk Strategy Review

The RISK-TEST-STRATEGY is thorough and correctly expanded for the preset system. Preset-specific risks are well-specified:

- **R-01 (ConfidenceParams call site migration)**: Concrete verification — grep for constant names outside Default impls, plus behavioral unit test using `empirical` w_fresh (0.34 vs 0.18 provides sharp signal). Critical priority is correct.
- **R-02 (SR-10 regression)**: The mandatory test with the required comment text (`"SR-10: If this test fails, fix the weight table, not the test"`) is the single most important safety net in the entire test strategy. Correctly designated Critical.
- **R-03 (sum invariant)**: The `0.95` test case (detecting the `≤ 1.0` implementation mistake) is the critical regression detector. Both sides of the `1e-9` boundary tested.
- **R-08 (`[confidence]` silently active for named presets)**: Low likelihood but high severity — warn-and-ignore behavior tested separately from the resolved value test. Correct.
- **R-09 (wrong sum invariant)**: Overlaps R-03 — both correctly specified and both required.

The five mandatory pre-PR gates identified in the RISK-TEST-STRATEGY (SR-10 test, grep sweep, AC-25 unit tests, 0.92 sum invariant, named-preset immunity to `[confidence]`) are the right set.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for topic `vision` — result: entry #2063 (single-file topology scope/vision language check, nxs-011). Not directly applicable to this feature. No prior alignment patterns for preset system design exist.
- Stored: entry #2298 "Config key semantic divergence: same TOML key, different weights payload than vision example" via `/uni-store-pattern` — the `[confidence]` namespace carries scoring factor weights in source docs vs. lambda/coherence weights in the vision example. Generalizable pattern: verify TOML key semantic payload matches vision example, not just key name. Applicable to any future feature that externalizes a second class of "weights" into config.
