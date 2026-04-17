# Gate 3b Report: vnc-013

> Gate: 3b (Code Review — Rework Iteration 1)
> Date: 2026-04-17
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | Two-function design (map_to_canonical + normalize_event_name) matches pseudocode exactly; build_request arms, rework gate, mcp_context promotion all confirmed |
| Architecture compliance | PASS | 4-layer boundary maintained; ADR decisions followed; all six blast-radius files updated per C-11 table |
| Interface implementation | PASS | HookInput.provider, HookInput.mcp_context, ImplantEvent.provider serde attrs correct; normalize_event_name signature matches pseudocode; DEFAULT_HOOK_SOURCE_DOMAIN pub(crate) in observation.rs |
| Test case alignment | PASS | Compile error fixed (main_tests.rs:25 uses `..`); AC-11 test added with feature-path signal assertion; AC-20 test added with hint/inference disambiguation |
| Code quality | WARN | cargo build succeeds; all tests pass (0 failures); pre-existing clippy issue in auth.rs (collapsible_if); pre-existing oversized files; no stubs/todo!/unwrap in vnc-013 code |
| Security | PASS | No hardcoded secrets; serde default on new fields; provider value used only for string comparison; mcp_context deserialized as Value with typed extraction |
| Knowledge stewardship | PASS | rust-dev agent report contains Queried and Stored entries |

## Detailed Findings

### Pseudocode Fidelity
**Status**: PASS

The implementation follows the pseudocode's two-function design:

- `map_to_canonical(event: &str) -> &'static str` — private, used in hint path
- `normalize_event_name(event: &str) -> (&'static str, &'static str)` — public, inference path only

The architecture specification defines a single two-parameter function; the pseudocode resolves this to a two-function split with documented rationale (separation of concerns, honest return types). The pseudocode is the authoritative design for Gate 3b. The implementation matches it exactly at `hook.rs` lines 63-118.

`run()` two-path logic (lines 146-168): hint path calls `map_to_canonical()` and uses CLI flag verbatim; inference path calls `normalize_event_name()`. Both set `hook_input.provider = Some(...)` before `build_request()` so `ImplantEvent.provider` is always `Some` (FR-02.4 / AC-05).

The `__unknown__` sentinel and raw event passthrough (`effective_event`) are implemented as specified in pseudocode/normalization.md.

`build_request()` debug_assert at entry (lines 437-443) fires in debug builds if provider-specific names bypass normalization. Rework gate at `"PostToolUse"` arm uses `provider_val != "claude-code"` check (ADR-005). `mcp_context` promotion via `Cow<HookInput>` pattern correctly passes the cloned/mutated input to `build_cycle_event_or_fallthrough()`.

### Architecture Compliance
**Status**: PASS

All six blast-radius files have changes matching the architecture's C-11 table:

| File | Change | Status |
|------|--------|--------|
| `wire.rs` | `provider` + `mcp_context` on HookInput; `provider` on ImplantEvent | PASS |
| `hook.rs` | `normalize_event_name()`, rework gate, mcp_context promotion | PASS |
| `listener.rs` | Site A uses `DEFAULT_HOOK_SOURCE_DOMAIN`; AC-16 debug_assert present | PASS |
| `background.rs` | Site B Approach A registry-with-fallback | PASS |
| `services/observation.rs` | Site C Approach A; `_registry` prefix removed | PASS |
| `main.rs` | `Hook` variant has `provider: Option<String>`; dispatch passes to `run()` | PASS |
| `.gemini/settings.json` | Present with 4 hooks and mcp_unimatrix_.* matcher | PASS |
| `.codex/hooks.json` | Present with --provider codex-cli on all events and bug #16732 caveat | PASS |

ADR-001 through ADR-006 decisions are followed. `DomainPackRegistry` is unchanged (AC-13 / C-11 invariant).

The `content_based_attribution_fallback` sync closure in `listener.rs` uses `DEFAULT_HOOK_SOURCE_DOMAIN` rather than `event.provider` because the registry is not accessible in that sync closure — this is the accepted implementation decision from the first gate report and is consistent with the architecture's known limitation (OQ-4).

### Interface Implementation
**Status**: PASS

Interfaces match architecture contracts:

```rust
// wire.rs — HookInput additions
#[serde(default)] pub provider: Option<String>,
#[serde(default)] pub mcp_context: Option<serde_json::Value>,

// wire.rs — ImplantEvent addition
#[serde(default, skip_serializing_if = "Option::is_none")] pub provider: Option<String>,

// hook.rs — function signatures
pub fn normalize_event_name(event: &str) -> (&'static str, &'static str)
pub fn run(event: String, provider: Option<String>, project_dir: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>>

// observation.rs — constant
pub(crate) const DEFAULT_HOOK_SOURCE_DOMAIN: &str = "claude-code";
```

FR-06 contract: `"claude-code"` literal no longer appears as a hardcoded `source_domain` assignment in any of the three production sites. Background.rs references at lines 3541/3553 are test-code struct literals for `ObservationRecord`, not production assignments.

### Test Case Alignment
**Status**: PASS

**Compile error resolved**: `main_tests.rs:25` now uses `Some(Command::Hook { event, .. })` with the `..` wildcard, correctly handling the new `provider` field. All tests compile and pass.

**AC-11 test added** (`test_gemini_before_tool_topic_signal_extraction`, hook.rs lines 4676-4733):
- Calls `normalize_event_name("BeforeTool")` and asserts `("PreToolUse", "gemini-cli")`
- Calls `extract_event_topic_signal("PreToolUse", &input)` with `tool_input` containing feature path `"vnc-013"`
- Asserts signal is `Some("vnc-013")` (not empty, not generic)
- Calls `build_request("PreToolUse", &input)` end-to-end and asserts `ImplantEvent.topic_signal == Some("vnc-013")`

**AC-20 test added** (`test_run_session_start_provider_hint_precedence`, hook.rs lines 4751-4815):
- Tests hint path: `map_to_canonical("SessionStart") == "SessionStart"`; `hook_input.provider == Some("codex-cli")`
- Tests inference path: `normalize_event_name("SessionStart") == ("SessionStart", "claude-code")`
- Final assertion: `input_hint.provider != input_infer.provider` (core AC-20 invariant — hint overrides inference for shared event names)
- Both paths verified to produce `HookRequest::SessionRegister`

**All test suites pass** (`cargo test --workspace`): 0 failures across all test groups. The hook.rs vnc-013 test suite (16 tests) and wire.rs (7 tests) and observation.rs (5 tests) and listener.rs (debug_assert test) all included and green.

### Code Quality
**Status**: WARN

**Build**: `cargo build --workspace` — Finished with 0 errors. 18 warnings in unimatrix-server (pre-existing, not introduced by vnc-013).

**Tests**: `cargo test --workspace` — all test groups pass. No failures. Total passing count exceeds 4000 tests.

**Stubs/placeholders**: None in vnc-013 additions. No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`.

**Unwrap in non-test code**: No `.unwrap()` introduced by vnc-013. `Cow<HookInput>` pattern used correctly.

**File size**: All oversized files are pre-existing; no new files were created by vnc-013. All vnc-013 additions are additions to existing files.

**Clippy** (WARN — pre-existing, not vnc-013): `cargo clippy --workspace -- -D warnings` fails on `crates/unimatrix-engine/src/auth.rs:113` — `collapsible_if` warning treated as error. This is unrelated to vnc-013. Clippy passes on all vnc-013 modified files.

**cargo audit**: `cargo-audit` not installed in this environment. Cannot verify CVE status. (WARN — environment limitation, not a code issue.)

### Security
**Status**: PASS

No hardcoded secrets or API keys in vnc-013 additions. `DEFAULT_HOOK_SOURCE_DOMAIN` is a non-sensitive constant.

New fields use `#[serde(default)]` — malformed or missing fields degrade to `None` without panic (NFR-04, NFR-05).

`provider` field from `--provider` CLI flag flows into `ImplantEvent.provider` and `source_domain`. Value is stored as a plain string field, not a SQL parameter, and is never executed — no SQL injection risk (confirmed in RISK-TEST-STRATEGY.md security section).

`mcp_context` deserialized as `Option<serde_json::Value>` with `as_object()` + `as_str()` typed extraction. Arbitrary JSON is safely sandboxed. Empty or non-object `mcp_context` values cause the promotion to be skipped, not panicked.

The `contains("context_cycle")` check in `build_cycle_event_or_fallthrough()` is the existing guard and its security behavior with bare names is analyzed in pseudocode/normalization.md: an injected name containing "context_cycle" still fails the first condition check (`tool_name != "context_cycle"`) — only exact match proceeds. No additional hardening needed.

### Knowledge Stewardship
**Status**: PASS

The rust-dev agent report (`product/features/vnc-013/agents/vnc-013-rust-dev-report.md`) contains a `## Knowledge Stewardship` section with `Queried:` entries documenting pre-implementation pattern queries and `Stored:` entries documenting knowledge stored after implementation.

## Rework Required

None.

## Knowledge Stewardship

- Stored: nothing novel to store -- the fixes (struct pattern `..` wildcard for new field, substantive topic_signal and hint-precedence tests) are standard Rust patterns not warranting a lesson entry. The gate pass confirms the rework was complete and targeted.
