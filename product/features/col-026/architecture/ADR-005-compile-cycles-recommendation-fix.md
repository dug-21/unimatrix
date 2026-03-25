## ADR-005: compile_cycles Recommendation Text Correction (AC-19)

### Context

AC-19 requires that the `compile_cycles` recommendation text describes repeated compilation
errors as the likely cause, with no mention of allowlists or permission prompts.

The current text in `unimatrix-observe/src/report.rs` line 62 reads:
```
"Add common build/test commands to settings.json allowlist"
```

This text is wrong in two ways:
1. `compile_cycles` counts how many times `cargo check` / `cargo build` / `cargo test` was
   invoked, not how many permission prompts were generated. When Claude Code runs in
   skip-permissions mode, allowlists are irrelevant — there are no permission prompts to
   suppress.
2. High compile cycle counts arise from iterative per-field changes to structs, resolving
   cascading type errors, or repeated test runs while fixing failures — not from permission
   friction.

The `permission_friction_events` metric has its own detection path (`PermissionRetriesRule` in
`detection/friction.rs`) and a separate recommendation template in `report.rs`. These two paths
are confirmed independent: no cross-contamination was found in the existing code. The confusion
comes only from the `compile_cycles` recommendation text erroneously suggesting an allowlist
action that belongs to permission friction.

### Decision

Change the `compile_cycles` recommendation text in `unimatrix-observe/src/report.rs` at the
two relevant sites (lines 62 and 88):

**Old** (both sites):
```
"Add common build/test commands to settings.json allowlist"
```

**New** (both sites):
```
"Batch field additions before compiling — high compile cycle counts typically indicate iterative
per-field struct changes or cascading type errors; complete type definitions and resolve
compiler errors in-memory before each build"
```

The `rationale` field for the recommendation (if populated) should read:
```
"Each compile-check-fix loop adds 2–6 compile events; batching changes to logical units
(complete struct definitions, full impl blocks) before building reduces total compile count"
```

No change to detection logic in `detection/agent.rs`. `COMPILE_CYCLES_THRESHOLD` is
unchanged. The `threshold` field on the finding is unaffected.

Also confirm that the `permission_friction_events` recommendation template (if one exists) does
NOT reference compile cycles or allowlists. Per code inspection, `permission_retries` does not
appear to produce a `Recommendation` via `recommendations_for_hotspots` (it is handled by
`PermissionRetriesRule` which produces its own claim but no recommendation template in
`report.rs` at the lines visible during design). If a recommendation is found, it must use
"reduce tool cancellations" framing, not allowlist framing.

### Consequences

Easier:
- `compile_cycles` recommendations are actionable for agents running in skip-permissions mode.
- No false "add to allowlist" noise for users who do not use permission allowlists.
- The detection and recommendation paths for `compile_cycles` and `permission_friction_events`
  are clearly separated.

Harder:
- Existing test `test_recommendation_compile_cycles_above_threshold` in `report.rs` asserts the
  old allowlist string. The test assertion must be updated to match the new text. This is
  expected test maintenance.
- No other tests should break: detection rules are untouched.
