## ADR-004: Config Validation Always Runs — Both Expander Fields Validated Regardless of Flag

### Context

`InferenceConfig::validate()` validates all PPR fields unconditionally (ppr_alpha, ppr_iterations,
ppr_inclusion_threshold, ppr_blend_weight, ppr_max_expand) regardless of whether PPR is
effectively enabled. Three new fields are added in crt-042:
- `expansion_depth: usize` — range `[1, 10]`
- `max_expansion_candidates: usize` — range `[1, 1000]`
- `ppr_expander_enabled: bool` — no range constraint

The question is whether `expansion_depth` and `max_expansion_candidates` should be validated
only when `ppr_expander_enabled = true`, or unconditionally.

**Option A — Conditional validation (only when enabled)**: Mirrors the NLI pattern where some
fields are only validated when `nli_enabled = true`. Avoids "unnecessary" validation on disabled
configurations.

**Option B — Unconditional validation**: Mirrors the existing PPR field pattern. All five PPR
config fields are validated unconditionally at server start regardless of whether PPR is enabled
via the broader inference config.

**Option A** was the source of subtle configuration bugs in NLI. When `nli_enabled = false`,
invalid values in NLI-specific fields were silently accepted. The moment an operator flipped
`nli_enabled = true` in production, previously-invisible misconfiguration (`expansion_depth = 0`,
`max_expansion_candidates = 0`) would cause failures at query time — not at server start.

The SCOPE.md §Design Decisions Q4 explicitly reversed the conditional approach: "Validate both
fields regardless of `ppr_expander_enabled`. Pre-validating catches `expansion_depth = 0` at
server start instead of at the moment someone flips the flag in production. Cost is zero."

**Option B costs nothing**: the validation logic is two integer comparisons per server start.
The benefit is that every TOML configuration is fully validated at startup — an operator cannot
ship a misconfigured `expansion_depth = 0` and discover it only when they enable the flag.

**AC-13 explicitly requires unconditional validation**: "enforces expansion_depth in [1, 10]
and max_expansion_candidates in [1, 1000] — always, regardless of ppr_expander_enabled."

### Decision

`expansion_depth` and `max_expansion_candidates` are validated unconditionally in
`InferenceConfig::validate()`, in the existing PPR validation block (following
`ppr_max_expand` validation). Validation uses the same error type as existing PPR range checks
(`ConfigError::NliFieldOutOfRange` — the name is inherited from the NLI era; it applies to all
inference config range errors).

Validation ranges (enforced as exclusive lower and inclusive upper bounds):
- `expansion_depth`: must be in `[1, 10]` inclusive. `expansion_depth = 0` is rejected at server start.
- `max_expansion_candidates`: must be in `[1, 1000]` inclusive. `max_expansion_candidates = 0` is rejected.

`ppr_expander_enabled` is a boolean; no range validation needed.

The three-level config merge (`InferenceConfig::merged`) must include project-level override
logic for all three new fields, following the existing five-field PPR merge pattern.

### Consequences

- Any TOML with `expansion_depth = 0` or `max_expansion_candidates = 0` fails at server start,
  even when `ppr_expander_enabled = false`.
- Operators cannot silently commit invalid expander configs that only fail at flag-flip time.
- The NLI conditional-validation trap is not repeated.
- Test coverage: a test asserting that `expansion_depth = 0` fails validation must be added,
  matching the existing `ppr_alpha` / `ppr_max_expand` validation test pattern.
