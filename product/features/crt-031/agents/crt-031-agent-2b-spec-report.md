# Agent Report: crt-031-agent-2b-spec

## Task

Produce SPECIFICATION.md for crt-031 (Category Lifecycle Policy + boosted_categories
de-hardcoding), incorporating locked decisions, SR-03/SR-09 mitigations, and the #409
dependency contract.

## Artifact Produced

`product/features/crt-031/specification/SPECIFICATION.md` — overwritten with full specification.

## Key Decisions Made

1. **27 ACs total.** All 23 from SCOPE.md (AC-01 – AC-23) plus four additions: AC-24 and AC-25
   address SR-03 (parallel list zeroing trap and isolated validate_config test), AC-26 and AC-27
   address SR-09 (pre-implementation grep precondition and Default impl guard test).

2. **Locked decisions encoded as FRs.** All eight locked decisions from the spawn prompt are
   expressed as numbered functional requirements (FR-01 through FR-20), each with an explicit
   verification method.

3. **OQ-01 (eval harness) written as conditional, non-blocking.** AC-19 and FR-15 specify the
   fix assuming `config_overrides` is accessible; OQ-01 documents the architect investigation
   path. The spec does not block on this.

4. **FR-19 as mandatory pre-implementation step.** The SR-09 grep precondition is expressed as a
   functional requirement (FR-19), not just a note — so the tester and risk strategist see it as
   a deliverable, not optional cleanup.

5. **Dependency contract section added.** Five numbered contract points define the interface
   between crt-031 and #409: `is_adaptive` as entry point, the maintenance tick stub as
   insertion point, no schema provision, operator-only config changes, and `add_category` is
   always pinned.

6. **SR-03 affected test helpers enumerated.** AC-24 names `config_with_custom_weights`,
   `config_with_categories`, and the `test_empty_categories_documented_behavior` test as the
   minimum audit set for parallel-list zeroing.

## Open Questions for Architect

- **OQ-01**: Accessibility of `profile.config_overrides.knowledge.boosted_categories` at Step 12
  of `eval/profile/layer.rs` — one-line fix vs threading change.
- **OQ-02**: categories.rs module split decision (SR-01, 500-line ceiling).
- **OQ-03**: `BackgroundTickConfig` composite vs raw `Arc<CategoryAllowlist>` parameter (SR-02).
- **OQ-04**: Circular dependency check for `default_boosted_categories_set()` helper (SR-08).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — 20 entries returned. Entries #3772, #3770,
  #3771, #3774, #2312, and #86 were directly applied to the specification content. No new
  patterns generalizable beyond crt-031 were identified.
