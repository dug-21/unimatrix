# Agent Report: crt-027-agent-2-testplan

## Phase

Stage 3a — Test Plan Design

## Output Files

All 9 test plan files produced:

- `/workspaces/unimatrix/product/features/crt-027/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/crt-027/test-plan/hook-routing.md`
- `/workspaces/unimatrix/product/features/crt-027/test-plan/wire-source-field.md`
- `/workspaces/unimatrix/product/features/crt-027/test-plan/listener-dispatch.md`
- `/workspaces/unimatrix/product/features/crt-027/test-plan/index-briefing-service.md`
- `/workspaces/unimatrix/product/features/crt-027/test-plan/service-layer-wiring.md`
- `/workspaces/unimatrix/product/features/crt-027/test-plan/context-briefing-handler.md`
- `/workspaces/unimatrix/product/features/crt-027/test-plan/index-entry-formatter.md`
- `/workspaces/unimatrix/product/features/crt-027/test-plan/protocol-update.md`

## Risk Coverage Summary

| Risk ID | Priority | Component File | Test Count |
|---------|----------|----------------|-----------|
| R-01 (Critical) | C | wire-source-field.md, hook-routing.md, listener-dispatch.md | 5 scenarios |
| R-02 (High) | H | index-briefing-service.md | 3 scenarios |
| R-03 (Critical) | C | listener-dispatch.md | 11 named tests |
| R-04 (High) | H | hook-routing.md | 6 scenarios |
| R-05 (High) | H | index-entry-formatter.md | 4 scenarios |
| R-06 (High) | H | index-briefing-service.md, context-briefing-handler.md | 6 scenarios |
| R-07 (High) | H | hook-routing.md | 4 scenarios (1 manual) |
| R-08 (Med) | M | service-layer-wiring.md | 3 scenarios (CI gate) |
| R-09 (Med) | M | index-briefing-service.md, service-layer-wiring.md | 3 scenarios |
| R-10 (Med) | M | index-briefing-service.md, context-briefing-handler.md | 3 scenarios |
| R-11 (Med) | M | protocol-update.md | 3 static checks |
| R-12 (Med) | M | listener-dispatch.md | 3 scenarios |
| R-13 (Low) | L | wire-source-field.md | 1 scenario |
| R-14 (Low) | L | listener-dispatch.md | 2 scenarios |

## Integration Suite Plan

Suites to run in Stage 3c: `smoke` (mandatory), `tools`, `lifecycle`, `edge_cases`, `protocol`

New integration tests planned for infra-001:
- `suites/test_tools.py`: 4 new tests (flat table format, active-only, default-k-20, k-override)
- `suites/test_lifecycle.py`: 2 new tests (WA-2 session boost, compact payload flat format)

Suites NOT required: `security`, `volume`, `confidence`, `contradiction`

## Open Questions

1. **`BriefingParams` struct after migration** — `tools.rs` currently requires `role` as a
   mandatory field. The spec says `role` is retained for backward compat but ignored. If
   `BriefingParams` gains a `topic` field as required, the three existing `BriefingParams`
   deserialization tests must be updated. Implementer must confirm the final struct definition
   before finalizing those tests.

2. **`write_stdout_subagent_inject` testability** — The function writes to global `stdout()`.
   If the function signature does not accept a `Write` impl, testing requires process-level
   stdout capture (e.g., using `std::process::Command` to invoke a helper binary). Recommend
   implementing `write_stdout_subagent_inject_to(writer: &mut impl Write, ...)` internally
   and exposing a thin `write_stdout_subagent_inject(entries_text: &str)` wrapper. This
   makes AC-SR02 and AC-SR03 testable without stdout redirection.

3. **`source` field with empty string value** — `wire-source-field.md` flags the case where
   `source: Some("")` would result in observation `hook = ""`. This is technically valid
   per the current design (no validation at wire level), but may be undesirable. Consider
   clamping to `"UserPromptSubmit"` when `source` is `Some("")`. No AC covers this; flag
   for implementer decision.

4. **`context_search_is_not_fire_and_forget` update required** — The existing test at line
   ~818 of `hook.rs` constructs `HookRequest::ContextSearch` without `source` field. This
   is a compile error after the field is added. The implementer must add `source: None` to
   that struct literal before Stage 3c can run tests.

## Manual Gate Item

AC-SR01 must be marked OPEN or CONFIRMED in RISK-COVERAGE-REPORT.md before Gate 3c approval.
This is a manual test: spawn a subagent with a known `prompt_snippet` and verify Unimatrix
injection text appears in the subagent's initial context.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for crt-027 architectural decisions — found 5 ADR entries
  (#3242-#3246) confirming all design decisions are already stored.
- Queried: `/uni-knowledge-search` for hook routing injection testing patterns — found entries
  #315, #252, #264, #2928, #314 (gateway injection pattern, string-refactor test patterns).
  These informed the test structure for source-field backward compat and the rewritten-test
  pattern verification approach.
- Stored: entry #3253 "Non-Negotiable Test Name Verification Pattern for Rewritten Test Suites"
  via `/uni-store-pattern` — captures the grep-per-test-name gate procedure for when a feature
  rewrites a batch of tests rather than adding new ones. Novel because it concretizes the
  lesson #2758 procedure with specific application context for rewritten (not just new) test suites.
