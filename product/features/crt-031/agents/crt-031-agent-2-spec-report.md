# Agent Report: crt-031-agent-2-spec

## Artifact Produced

`product/features/crt-031/specification/SPECIFICATION.md`

## Key Decisions Made

1. **FR-06 constructor delegation chain**: `from_categories_with_policy` is the canonical
   constructor; `from_categories` and `new` delegate through it. This matches the locked design
   decision and ensures no callsite breakage.

2. **AC-16 added for SR-03**: The spec explicitly enumerates the test-fixture constraint —
   any `KnowledgeConfig` struct literal used with `validate_config` that sets `categories`
   to a non-default value must also zero both `boosted_categories` and `adaptive_categories` to
   prevent false cross-check failures. Existing test helpers (e.g., `config_with_categories`)
   are called out for update.

3. **AC-17 added for SR-05**: A compile-level wiring test is required (analogous to the R-14
   PhaseFreqTableHandle test) asserting that the `CategoryAllowlist` passed through the Arc chain
   responds correctly to `is_adaptive`. This closes the silent-policy-drop risk.

4. **#409 dependency contract section**: A formal contract section was added enumerating exactly
   what #409 may assume about this feature's interface — stable `is_adaptive` signature,
   stub insertion point, pre-wired `Arc<CategoryAllowlist>` in `maintenance_tick`, and the
   prohibition on adding signal mechanics to `CategoryAllowlist` directly.

5. **SR-04 mitigation (intentional asymmetry documentation)**: The spec documents the
   summary-vs-JSON asymmetry as intentional and requires a golden-output test to prevent
   future formatters from silently reversing it.

6. **FR-14 module-split trigger**: The spec makes the 500-line file-size rule a binding
   requirement on the architect, not an advisory note.

## Open Questions for Architect

- OQ-04: Single `RwLock<(HashSet, HashSet)>` vs two separate `RwLock<HashSet>` fields.
- OQ-05: Whether to bundle `Arc<CategoryAllowlist>` into a composite struct to address SR-02
  (22-parameter count).

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — 18 entries returned; relevant entries #3715, #86,
  precedent for #2312 boosted_categories trap applied to SR-03 analysis. No existing lifecycle
  policy pattern found; this feature establishes a new convention.
