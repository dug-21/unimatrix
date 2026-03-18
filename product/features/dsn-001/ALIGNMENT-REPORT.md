# Alignment Report: dsn-001

> Reviewed: 2026-03-18
> Artifacts reviewed:
>   - product/features/dsn-001/architecture/ARCHITECTURE.md
>   - product/features/dsn-001/specification/SPECIFICATION.md
>   - product/features/dsn-001/RISK-TEST-STRATEGY.md
> Scope source: product/features/dsn-001/SCOPE.md
> Scope risk source: product/features/dsn-001/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | WARN | Confidence dimension weights and lambda weights deliberately dropped from scope — vision W0-3 section includes them; decision is justified but is a vision deviation requiring acknowledgement |
| Milestone Fit | PASS | Correctly placed at Wave 0; prerequisites satisfied; no W1/W2/W3 capabilities built prematurely |
| Scope Gaps | PASS | All SCOPE.md goals are fully addressed in the three source documents |
| Scope Additions | PASS | No items in source docs beyond what SCOPE.md asked for |
| Architecture Consistency | PASS | All SCOPE-RISK-ASSESSMENT risks resolved by ADRs; architecture is self-consistent |
| Risk Completeness | PASS | RISK-TEST-STRATEGY covers all 13 risks with prioritised scenarios; security risks fully analysed |

**Overall: WARN — one documented vision deviation requires human acknowledgement before delivery.**

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `[confidence]` lambda weights | In SCOPE.md non-goals (deliberately excluded). PRODUCT-VISION W0-3 includes them as interim fix. Source docs carry forward the exclusion — see Variances section. |
| Simplification | `[cycle]` label parameters | In SCOPE.md non-goals. PRODUCT-VISION W0-3 includes `work_context_label`/`cycle_label` as runtime-configurable. Source docs implement doc-fix only. — see Variances section. |
| Simplification | `[agents].bootstrap` list | PRODUCT-VISION W0-3 shows a configurable `bootstrap` agent list. SCOPE.md explicitly excludes `agent_bootstrap_defaults()` configurability. Source docs confirm exclusion with rationale. Consistent across all three docs. |
| Simplification | `[agents].default_trust` default | Vision shows `"restricted"` as the `default_trust` default; source docs use `"permissive"` (matching current compiled constant). A "permissive" default is consistent with the deferred W0-2 rationale (no security value before OAuth), but diverges from the vision's stated default string. |
| Addition | `ConfidenceConfig` and `CycleConfig` forward-compat stubs (ADR-004) | Not in SCOPE.md goals, but explicitly recommended in SCOPE-RISK-ASSESSMENT SR-04. Stubs are justified hedges for W3-1 compatibility. Addressed in SPECIFICATION FR-008/FR-009 and ARCHITECTURE §forward-compat stubs. These directly serve the vision's W3-1 section. |

---

## Variances Requiring Approval

### VARIANCE-1 (WARN): Confidence dimension weights (`[confidence] weights`) excluded from W0-3

**What**: SCOPE.md non-goals explicitly drop the four lambda weights
(`confidence_freshness`, `graph_quality`, `embedding_consistency`, `contradiction_density`)
from W0-3 scope. All three source documents are consistent with this exclusion.
The `ConfidenceConfig` struct is reserved as an empty stub (ADR-004) so W3-1 can add
fields without a format break.

However, the PRODUCT-VISION W0-3 section includes these weights as a first-class
deliverable:

> "The confidence dimension weights (freshness 0.35, graph 0.30, contradiction 0.20,
> embedding 0.15) are domain-specific constants. A legal knowledge base needs high
> `w_trust`, low `w_fresh`. An air quality deployment needs the inverse. Externalizing
> these is the interim fix that bridges the gap until W3-1's GNN learns them
> automatically. Without this, the confidence system remains domain-coupled through
> all of Wave 1 and 2."

The SCOPE.md rationale for dropping them is:

> "W3-1 GNN will learn these automatically from usage. Externalising them provides
> no value: operators have no basis for tuning ML weights, and the GNN cold-start
> initialises from internal defaults, not operator config."

**Why it matters**: This is a direct tension with the vision's stated reasoning.
The vision's argument is that the confidence dimension weights are *domain-specific
operator constants* needed for non-dev domains before W3-1 converges. The SCOPE.md
argument is that they are *ML weights* that operators cannot meaningfully tune and
that W3-1 cold-starts from hardcoded defaults anyway.

The W3-1 section of the vision explicitly states:

> "A missing or stale weight vector degrades gracefully to the config-defined defaults
> (W0-3 `[confidence] weights`)."

If W0-3 ships without a `[confidence] weights` field, W3-1's cold-start path as
described in the vision has no config-defined default to fall back to — it must either
define its own config format (creating a potential conflict) or use hardcoded dev-domain
constants (defeating the domain-agnosticism goal for non-dev deployments).

The `[confidence]` empty stub (ADR-004) reserves the TOML namespace, so a format break
is avoided. But the *semantic* gap remains: W3-1 will need to add `weights` to
`ConfidenceConfig` before its cold-start can use operator-configured values.

**Recommendation**: Human must decide one of:
1. **Accept deferral** — Acknowledge that W3-1 will add `weights` to `ConfidenceConfig`
   when that feature is scoped. Update the vision's W0-3 description to reflect that
   lambda weights are deferred to the W3-1 design phase. The empty stub ensures no
   format break. Current dsn-001 scope is correct as written.
2. **Add minimal stub fields** — Add empty `weights` field stubs to `ConfidenceConfig`
   now (with `None` defaults) so the W3-1 design has a concrete hook point and the
   vision's cold-start fallback path is addressable without a later format change.
3. **Restore scope** — Add the lambda weights to W0-3 scope. This aligns with the
   vision text but contradicts the SCOPE.md rationale.

**Note**: This is a scope decision made with clear reasoning. It does not block
delivery — the empty stub fully satisfies the forward-compatibility requirement.
Human approval of the deferral is the lowest-friction resolution.

---

### VARIANCE-2 (WARN): `[cycle]` label configuration excluded from W0-3

**What**: SCOPE.md non-goals exclude the `[cycle]` section from runtime configuration.
The source docs implement a hardcoded doc-fix on `CycleParams.topic` (FR-019) and
reserve `[cycle]` as an empty stub (FR-009). No runtime-configurable label parameters
are delivered.

The PRODUCT-VISION W0-3 section includes:

> ```toml
> [cycle]
> # context_cycle tool parameter labels — rename for non-dev domains
> work_context_label = "feature"   # label shown in tool descriptions
> cycle_label = "cycle"
> ```

SCOPE.md rationale for the exclusion:

> "The tool concept is already domain-neutral; 'feature' vocabulary in the tool
> description is addressed by the hardcoded rename/doc fix (Goal 7/8), not by
> runtime config."

**Why it matters**: The vision includes `[cycle]` label configurability as part of
the domain-agnosticism unlock. The SCOPE.md approach of a hardcoded doc-fix partially
addresses the concern (the doc comment will reference multiple domain examples, not
only "feature"). However, an operator deploying for SRE or legal cannot change the
tool description vocabulary without recompiling.

This is a lower-severity gap than VARIANCE-1: the `CycleParams.topic` doc-fix
(FR-019) provides meaningful improvement, and the stub reserves the namespace.
The tool's parameter is a free-form string field anyway, so non-dev operators can
supply their domain's identifier without a config change.

**Recommendation**: Human must decide one of:
1. **Accept deferral** — The doc-fix (FR-019) is sufficient for W0-3. The `[cycle]`
   stub reserves the namespace. Label configurability can be added if a real deployment
   requires it. Current dsn-001 scope is correct as written.
2. **Restore scope** — Add `work_context_label` and `cycle_label` to `CycleConfig`
   and wire them to the tool description generation path. This aligns with the vision
   but adds implementation complexity not justified by current operator demand.

**Note**: Given that `feature_cycle` and `topic` are free-form strings (per the vision's
own note on these fields), the practical impact of not having configurable labels is
lower than VARIANCE-1's impact on W3-1 cold-start.

---

### VARIANCE-3 (WARN): `[agents].default_trust` default diverges from vision's stated value

**What**: PRODUCT-VISION W0-3 shows `default_trust = "restricted"` as the example
config value. All three source documents use `"permissive"` as the compiled default,
matching the current `PERMISSIVE_AUTO_ENROLL = true` constant.

SCOPE.md does not explicitly address this default value discrepancy.

**Why it matters**: The vision's W0-2 deferral rationale explains that `PERMISSIVE_AUTO_ENROLL=false`
adds no security value before OAuth. However, showing `"restricted"` in the vision's
W0-3 config example creates an implicit signal that the default should change with
config externalization. If operators read the vision's example config and set nothing,
they may expect `"restricted"` behavior but get `"permissive"`.

This is a documentation inconsistency rather than a functional variance — the source
docs are internally consistent and correctly preserve the current default.

**Recommendation**: Confirm that `"permissive"` is the intended compiled default for
W0-3 (consistent with the W0-2 deferral rationale). Update the vision's W0-3 config
example to show `"permissive"` to avoid operator confusion, or add a note explaining
the default. No code change required.

---

## Detailed Findings

### Vision Alignment

The overall feature direction — moving hardcoded constants to operator-configurable
TOML config to enable domain-agnostic deployment without recompiling — is fully
aligned with the vision's core principle:

> "Any knowledge-intensive domain — environmental monitoring, SRE operations,
> scientific research, regulatory compliance — runs on the same engine, configured
> not rebuilt."

The vision's Critical Gaps table lists W0-3 items explicitly:
- "Freshness half-life hardcoded at 168h" — CRITICAL — addressed by FR-005 / AC-04
- "`lesson-learned` category name hardcoded in scoring" — CRITICAL — addressed by FR-014
- "SERVER_INSTRUCTIONS const uses dev-workflow language" — HIGH — addressed by FR-017
- "Initial category allowlist hardcoded" — HIGH — addressed by FR-013
- "`context_retrospective` tool name is SDLC-specific" — MEDIUM — addressed by FR-018

The feature delivers all vision-listed items for W0-3 **except** the lambda dimension
weights, which are listed as CRITICAL in the Critical Gaps table
("Lambda dimension weights hardcoded") and included in the vision's W0-3 config block.
This is VARIANCE-1 above.

The vision's security requirements for W0-3 are fully addressed:
- ContentScanner validation of `[server].instructions` — FR-012 / AC-12
- File permission enforcement — FR-011 / AC-08 / AC-09
- Category character allowlist and count ceiling — FR-012 / AC-10
- `boosted_categories` subset validation — AC-11
- Confidence weight sum/range validation — not applicable (weights deferred to W3-1)

The `context_retrospective` → `context_cycle_review` rename is directly supported
by the vision:

> "The rename to `context_cycle_review` is domain-neutral ('review' applies to any
> cycle — post-incident review, campaign review, case review') and makes the pairing
> with `context_cycle` self-evident: you start/stop a cycle, then review it. This
> rename is a W0-3 scope addition — low-effort, high clarity gain."

All three source documents handle this rename comprehensively (SR-05 exhaustive
checklist in SPECIFICATION.md §SR-05; ARCHITECTURE §8; RISK-TEST-STRATEGY R-01).

---

### Milestone Fit

dsn-001 is correctly positioned as a Wave 0 prerequisite. Evidence:

1. The vision states W0-3's purpose explicitly: "This is the single unlock for domain
   agnosticism. Every other domain-coupling gap either disappears... or becomes trivially
   fixable via config."

2. No Wave 1, Wave 2, or Wave 3 capabilities are introduced. The feature is purely a
   startup-path change: config loaded once, values distributed to subsystems. No new
   MCP tools, no schema changes, no ML components.

3. The architecture correctly defers `tokio_main_bridge`, `Command::Hook`, and
   export/import subcommands from config loading (ARCHITECTURE §Config distribution).
   This shows appropriate restraint — no over-engineering.

4. The `[inference]` section from the vision (GGUF model path for W3-3) is not
   introduced in this feature, correctly deferring it to W3-3 scope. The `[confidence]`
   and `[cycle]` stubs are the only forward-compat elements, and both are empty.

5. The vision's W3-1 dependency on `[confidence] weights` is addressed via the empty
   stub (ADR-004), which satisfies the format-compatibility requirement even if the
   semantic content is deferred.

---

### Architecture Review

The architecture is sound and self-consistent. All four SCOPE-RISK-ASSESSMENT risks
assigned to the architecture phase are resolved:

- **SR-02** (ConfidenceParams API change) → ADR-001: `ConfidenceParams` struct.
  Correct choice — absorbs W3-1 API extension without further engine churn. The struct
  includes `alpha0` and `beta0` fields alongside `freshness_half_life_hours`, which is
  a minimal but forward-looking design.

- **SR-04** (forward-compat stubs) → ADR-004: Empty `ConfidenceConfig` and `CycleConfig`.
  These reserve the TOML namespace. However, as noted in VARIANCE-1, the semantic gap
  for W3-1 cold-start remains open — the stub prevents a format break but does not
  provide the operator-facing defaults that the vision's W3-1 section requires.

- **SR-06** (merge semantics) → ADR-003: Replace semantics for list fields.
  This matches SPECIFICATION.md FR-003. The merge uses `Option<T>` intermediate
  deserialization to distinguish "explicitly set to default" from "absent" — this is
  the correct approach to avoid the R-03 false-negative risk.

- **SR-07** (CategoryAllowlist constructor split) → ADR-002: `new()` delegates to
  `from_categories(INITIAL_CATEGORIES)`. Clean resolution — single code path.
  The architecture note on this point (§3) is clear and correct.

- **SR-08** (crate boundary) → ADR-002 + plain parameter crossing. The architecture
  explicitly avoids `Arc<UnimatrixConfig>` crossing the crate boundary into
  `unimatrix-store`. Plain `Vec<Capability>` values cross instead. This is the
  right call — it avoids circular dependencies and keeps `unimatrix-store` config-agnostic.

One architecture detail warrants delivery-team attention: the `ContentScanner::global()`
warm-up ordering constraint (SR-03) is resolved via a documented invariant
(`let _scanner = ContentScanner::global()` at top of `load_config`) rather than a
type-system guarantee. This is acceptable given the existing codebase pattern but
must be verified in code review (not just by build passing). RISK-TEST-STRATEGY R-04
correctly flags this.

---

### Specification Review

The specification is complete and well-structured. All 21 acceptance criteria from
SCOPE.md are carried forward intact. The specification adds:

- FR-008 and FR-009 (forward-compat stubs) — justified by SCOPE-RISK-ASSESSMENT SR-04
  and fully consistent with the non-goal language in SCOPE.md ("Architect should
  forward-design the `UnimatrixConfig` struct with placeholder sections").
- FR-020 (`dirs::home_dir()` None handling) — not in SCOPE.md but required for
  correctness in container environments. An essential robustness requirement, not scope
  creep.
- FR-021 (malformed TOML error handling) — similarly essential; absence would leave
  an unspecified failure mode.

The SR-05 exhaustive rename checklist (SPECIFICATION.md §SR-05) is comprehensive:
12 Rust file locations + 9 Python test file locations + 3 protocol/skill file locations
+ 1 README location. The zero-match grep verification step is correctly designated
as mandatory (not optional) before PR merge.

The domain models section (§Domain Models) cleanly captures the merge algorithm,
including the key detail that `Option<T>` intermediate types are used during
deserialization to correctly distinguish field-presence from field-value. This
resolves the R-03 risk at the spec level.

One specification gap: SCOPE.md §Edge Cases notes that an empty `categories = []`
list is unaddressed — "the constraint is ≤ 64 but no minimum." This ambiguity is
present in the specification's validation table (no minimum-count constraint). The
RISK-TEST-STRATEGY flags this in §Edge Cases ("Either reject... or accept as degenerate
but valid config. The spec does not explicitly address..."). **The spec should clarify
whether `categories = []` is a valid config.** This is a minor gap but could cause
inconsistent behavior at the boundary.

---

### Risk Strategy Review

The RISK-TEST-STRATEGY is thorough. Key strengths:

1. **R-01** (rename checklist miss) is correctly prioritized as Critical with concrete
   test scenarios — zero-match grep as a mandatory gate, plus integration test asserting
   both presence of new name AND absence of old name. This is the right approach for a
   blast-radius rename.

2. **R-02** (ConfidenceParams migration) identifies 13 specific call-site files.
   This level of specificity is valuable for the delivery agent. The grep-for-constant
   scenario (asserting `FRESHNESS_HALF_LIFE_HOURS` does not appear in non-comment,
   non-Default contexts) is a clean mechanical verification.

3. **R-03** (merge false-negative) correctly identifies the `PartialEq`-with-`Default`
   detection pitfall and recommends the `Option<T>` approach — which the specification
   has already adopted. This alignment between risk strategy and specification is good.

4. **Security risks** are fully analysed: `[server].instructions` injection surface,
   `[knowledge].categories` as knowledge base gate, `[agents].session_capabilities`
   Admin exclusion, and TOCTOU on permission check. Each includes a threat model
   (what untrusted input, damage, blast radius) and a concrete test.

5. The **scope risk traceability table** at the end of RISK-TEST-STRATEGY correctly
   maps all 8 SCOPE-RISK-ASSESSMENT risks to architecture risks and their resolutions.
   This is clean cross-document traceability.

One gap: RISK-TEST-STRATEGY §Edge Cases notes that `categories = []` is an unspecified
boundary (matching the specification gap noted above). R-05 (stub silent acceptance)
identifies a testing requirement but also notes it is an "operator-experience risk"
with no runtime mitigation — acceptable given that the `[confidence]` and `[cycle]`
stubs are explicitly designed as empty forward-compat structures.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for topic `vision` — Unimatrix MCP server not callable
  from this agent context; no results returned. Secondary evidence drawn from prior
  MEMORY.md entries and codebase conventions.
- Stored: nothing novel to store — the vision-vs-scope deviation pattern (documented
  product vision includes items that a scoped feature deliberately excludes with rationale)
  is a general design discipline concern, not a dsn-001-specific pattern. If the same
  type of vision deviation (feature drops scope from vision with rationale) appears
  across two or more additional features, it warrants a stored pattern entry.
