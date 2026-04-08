# Scope Risk Assessment: nan-011

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `config.toml` default values diverge from compiled defaults in `config.rs` — Unimatrix entry #3817 confirms config has two independent default sites (serde `default_*` fn + the Default impl) that must change atomically; the current config.toml already diverged once (only `[retention]` documented) | High | High | Implementer must read every `default_*` function in `config.rs` and cross-check each field; do not infer defaults from field types or comments |
| SR-02 | npm `npm pack --dry-run` verification (AC-13) requires a working Node.js toolchain in the CI/dev environment — if the build environment lacks Node, this AC cannot be verified without infra changes | Med | Low | Confirm Node.js and npm are available in dev container before delivery; gate the AC on actual `pack --dry-run` output, not manual inspection |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | The SCOPE.md states protocols in `protocols/` are "independent copies" of `.claude/protocols/uni/` files — but any accuracy corrections found during validation (AC-15) must be applied to both copies; this dual-maintenance obligation is easy to miss and creates permanent drift | High | High | Spec must require the implementer to diff both copies post-edit and confirm they are identical; or apply all edits to source first, then copy |
| SR-04 | AC-10 requires zero bare tool name invocations across 14 skills — but the audit scope conflates two different contexts: code-block invocations (which require the prefix) and prose references (which the SCOPE explicitly exempts). A regex grep will match prose too, creating false positives or under-coverage | Med | Med | Spec should provide the exact grep pattern to use (e.g., backtick-wrapped bare names, not all occurrences of `context_search`) |
| SR-05 | `uni-seed` update requires verifying seed entry categories against `INITIAL_CATEGORIES` in `categories.rs` — the allowlist changes with schema migrations; if the implementer reads a stale version or the allowlist changes between delivery and merge, seed entries silently write to uncategorized state | Med | Low | Implementer must read `categories.rs` at delivery time; spec should note that allowlist is the authority, not SCOPE.md assumptions |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | The approved vision statement in SCOPE.md (verbatim, AC-01) includes the phrase "before agents need to ask for it" — this describes proactive delivery which the SCOPE.md non-goals section acknowledges oversells current capability ("Invisible Delivery" bullet is out of scope to fix). A reader of the README alone cannot distinguish marketing copy from implemented behavior | Med | High | Spec should require a footnote or parenthetical in the README alongside the vision statement clarifying that proactive delivery is workflow-phase-conditioned, not hook-injected |
| SR-07 | `uni-retro` skill distribution to npm means that skill's behavior becomes part of the public API surface — any future changes to `context_retrospective` tool signatures or retro report format will break distributed installs that have not updated | Med | Med | Spec should note that distributing `uni-retro` in npm creates a versioning contract; the skill should document minimum compatible Unimatrix version |

## Assumptions

- **SCOPE.md §Deliverable 3** assumes 14 skills are the complete set — confirmed by Glob (14 SKILL.md files present). If a skill is added before delivery closes, the audit count is wrong.
- **SCOPE.md §Config Structure** assumes `UnimatrixConfig` has exactly 8 top-level TOML sections — implementer must verify against `config.rs` struct definitions, not the SCOPE table.
- **SCOPE.md §Non-Goals** states no changes to `InferenceConfig` defaults or compiled-in Rust defaults — this is correct but the config.toml AC-08 requirement (values match compiled defaults) still requires reading those compiled values. The assumption that "no code changes" means "no need to read code" is false.

## Design Recommendations

- SR-01: The spec writer should require the implementer to produce a side-by-side table of config.toml field vs. compiled default as a deliverable artifact, not just assert correctness.
- SR-03: The spec should make the dual-copy update explicit as a step in the delivery checklist, with a required diff verification before PR.
- SR-06: If the vision statement cannot be modified (it is approved verbatim), the spec should add a companion "What's Shipped Today" callout box in the README that maps each capability claim to its implementation status.
