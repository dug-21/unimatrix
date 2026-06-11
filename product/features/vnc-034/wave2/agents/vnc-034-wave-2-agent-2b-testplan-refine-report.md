# Agent Report — vnc-034-wave-2-agent-2b-testplan-refine

REFINEMENT pass on vnc-034 Wave 2 test plans (issue #727) for Stage 3a locked
decisions D4/D5/D6 + funnel-honesty record. Updated four files in place; D1 security
table (my_project→REJECT / 64→REJECT / 63→ACCEPT) left intact. D2/D3 negative tests intact.

## Files updated
- product/features/vnc-034/wave2/test-plan/projects-config.md
- product/features/vnc-034/wave2/test-plan/project-registry-cli.md
- product/features/vnc-034/wave2/test-plan/project-router.md
- product/features/vnc-034/wave2/test-plan/OVERVIEW.md

## New test names per decision

### D5 reserved-slug (registry-CLI §A.2 + projects-config RESERVED-SLUG TABLE)
- test_register_rejects_reserved_tools_shadowing (THE critical shadowing test)
- test_register_reserved_is_separate_from_charset (discriminator)
- test_register_rejects_reserved_route_segments (v1/health/observe)
- test_register_reserved_exact_match_only (no over-broad prefix match)
- test_reserved_check_is_separate_from_charset (config mirror)
- test_reserved_set_exact_match_only (config mirror)
- Table T-RSV-01..05

### D4 delete/purge/re-attach (registry-CLI §C)
- test_delete_deregisters_and_preserves_data_dir
- test_purge_requires_slug_confirmation_or_no_destroy (loud-destroy)
- test_purge_with_confirmation_removes_dir_and_deregisters
- test_deregister_reregister_reattaches_to_preserved_chain (HIGHEST-VALUE)
- test_purge_then_register_is_fresh_store (contrast guard)
- test_register_delete_reregister_roundtrip (lifecycle view)

### D6 register two-state (registry-CLI §A.3)
- test_register_already_routing_errors_loud
- test_register_dir_exists_deregistered_reattaches
- test_register_two_states_distinct_messages

### Funnel no-bypass (project-router)
- test_no_residual_fixed_adapter_path (unit/structural)
- test_dispatch_through_adapter_for_no_fixed_bypass (HTTP integration, ≥2 slugs + Default)

## Risk / AC mapping
- D5 → R-03 (allowlist family), AC-W2-R4
- D4 → R-04, R-11, AC-W2-R4
- D6 → R-11, AC-W2-R4
- Funnel → R-01, AC-CT-C4
OVERVIEW now carries a "Stage 3a refinement → test mapping" table and updated AC rows
(AC-W2-R4, AC-CT-C4).

## Conflicts resolved
- registry-CLI old `test_register_idempotent_or_errors_on_existing` and
  `test_register_rejects_reserved_slug` replaced by D5 (§A.2) and D6 (§A.3) tests.
- registry-CLI old `test_delete_removes_or_retires_store_dir` (ambiguous "removed or
  retired") replaced by D4's explicit de-register-preserves-dir semantics.
- Edge-case note "clean re-register, fresh store" (contradicted D4) corrected to
  re-attach-on-deregister / fresh-on-purge.

## Open questions
1. `--purge` confirmation mechanism: re-typed slug as a positional/flag value
   (e.g. `--purge alpha --confirm alpha`) vs interactive prompt. Tests assert the
   contract (no-destroy without matching slug confirmation) but pseudocode owns the
   exact surface. Non-interactive form preferred for testability.
2. Re-attach detection: how the registry recognizes an existing dir as the SAME store
   (presence of hash-chain head / store manifest) to re-attach vs. provision fresh —
   pseudocode/architecture concern; the test asserts the observable (chain head H1
   preserved), not the mechanism.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-005 (#4949, /v1/tools default
  alias — confirms the tools-shadowing rationale for D5), ADR-006 (#4952, wave→issue),
  seam-wiring pattern (#4957). Applied ADR-005 to the T-RSV-01 shadowing rationale.
- Stored: nothing novel to store -- this is a test-plan refinement applying already-locked
  human decisions to existing plans; no new reusable test infrastructure pattern emerged.
