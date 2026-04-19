# Gate 3a Report: vnc-013

> Gate: 3a (Component Design Review — Rework Iteration 2, FINAL)
> Date: 2026-04-17
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All ADR decisions reflected; OVERVIEW.md and wire-protocol.md now show correct two-path dispatch |
| Specification coverage | WARN | FR-01.1/FR-01.6 signature departure documented with clear rationale; FR-04.1 defense-in-depth departure also documented; behavioral outcomes identical |
| Risk coverage | PASS | All 13 risks have mapped test scenarios; R-08 scenario 3 removal documented |
| Interface consistency | PASS | All four previously-failing items resolved; interfaces consistent across all pseudocode and test plan files |
| Knowledge stewardship | WARN | pseudocode agent missing required `Stored: nothing novel to store -- {reason}` format; content present but format non-compliant |

---

## Detailed Findings

### Architecture Alignment

**Status**: PASS

All ADR decisions are reflected in the pseudocode:

- **ADR-001** (Claude Code names canonical): `normalize_event_name` and `map_to_canonical` both return Claude Code canonical names. Match table in OVERVIEW.md correct.
- **ADR-002** (provider field on wire protocol): `HookInput.provider` and `ImplantEvent.provider` specified with `#[serde(default)]` in wire-protocol.md.
- **ADR-003** (named `mcp_context` field): `HookInput.mcp_context: Option<serde_json::Value>` specified as a named field, with note that serde named-field priority prevents capture in `extra` flatten.
- **ADR-004** (Approach A for DB read paths): Sites B and C use `registry.resolve_source_domain()` with `DEFAULT_HOOK_SOURCE_DOMAIN` fallback. Constant placement resolved (OQ-A → observation.rs, pub(crate) re-export).
- **ADR-005** (rework gate = `provider != "claude-code"`): PostToolUse arm gates on `provider_val != "claude-code"` with citation. Rework path only for `"claude-code"` provider.
- **ADR-006** (Codex `--provider codex-cli` mandatory): reference-configs.md documents mandatory flag and caveat text.

**OVERVIEW.md data flow** (previously WARN): Step 2 now shows two-path dispatch correctly:
```
Step 2: two-path dispatch on provider_hint_from_cli:
          if provider.is_some() → canonical_name = map_to_canonical(raw_event)
          else                  → (canonical_name, provider_str) = normalize_event_name(raw_event)
```

**wire-protocol.md Initialization Sequence** (previously WARN): Lines 111-131 now show the correct two-path dispatch matching normalization.md. No longer shows the old 2-argument `normalize_event_name(&event, provider_hint)` call.

Component boundaries, layer separation, technology choices, and six-file blast radius all match ARCHITECTURE.md exactly.

---

### Specification Coverage

**Status**: WARN

All functional requirements are addressed. Two documented departures from the literal specification text exist; both have explicit rationale and produce identical behavioral outcomes.

**Departure 1 — FR-01.1 and FR-01.6 signature change** (WARN, not FAIL):

FR-01.1 specifies `normalize_event_name(event: &str, provider_hint: Option<&str>) -> (&str, &str)`.
FR-01.6 specifies the function is called in `build_request()`.

The pseudocode redesigns to:
- `normalize_event_name(event: &str) -> (&'static str, &'static str)` — inference path only, called in `run()`
- `map_to_canonical(event: &str) -> &'static str` — private helper called in `run()` hint path

The rationale (normalization.md "Design Decision" section) is clear: the 2-argument form requires returning a dynamic `&str` hint value from a `&'static str` return type — a lifetime mismatch. The 1-argument factoring keeps the return type honest with zero allocations on all known paths. The behavioral outcome satisfies FR-01.2, FR-01.3, FR-01.4, FR-01.5 identically. The ARCHITECTURE.md data flow diagram also shows normalization in `run()`, not `build_request()`, confirming the architecture document and pseudocode agree and the specification text is the outlier.

**Departure 2 — FR-04.1 defense-in-depth arms** (WARN, not FAIL):

FR-04.1 specifies adding Gemini-named match arms to `build_request()` as defense-in-depth guards.

The pseudocode rejects this and installs a `debug_assert!` instead:
```
debug_assert!(
    !matches!(event, "BeforeTool" | "AfterTool" | "SessionEnd"),
    "provider-specific event name reached build_request() without normalization: {event}"
);
```

Rationale: normalization is called unconditionally before `build_request()`, making the arms dead code. A `debug_assert!` enforces the contract in debug builds without runtime cost in production and without misleading future readers about the contract.

The behavioral outcome (Gemini events never reach `build_request()` with provider-specific names) is equivalent. This is a quality improvement, not a gap.

**All other FRs and NFRs**: Fully covered with no gaps or scope additions. AC-01 through AC-20 all have pseudocode paths.

---

### Risk Coverage

**Status**: PASS

All 13 risks from RISK-TEST-STRATEGY.md have mapped test scenarios. Coverage by priority:

**Critical (R-01)**: 4 scenarios in normalization.md gate-prerequisite section. AC-14 established as implementation gate. Integration test (R-01 scenario 4) covers AC-02 and AC-09.

**High (R-02, R-03, R-04, R-05)**: 3-4 scenarios each.
- R-02: `test_implant_event_provider_set_for_record_event_variants`, `test_cycle_event_provider_propagated`, `test_provider_none_falls_back_at_listener`
- R-03: `test_run_codex_provider_hint`, `test_normalize_shared_name_without_hint_defaults_to_claude_code`, config review check
- R-04: `test_approach_a_fallback_for_stop_event`, `test_approach_a_fallback_for_cycle_events`, existing tests preserved
- R-05: `test_gemini_after_tool_skips_rework_path`, `test_codex_post_tool_use_skips_rework_path`, `test_claude_code_post_tool_use_enters_rework_path`

**Medium (R-06 through R-10)**: All covered.
- R-07: `test_rework_candidate_guard_fires_in_debug` (#[cfg(debug_assertions)] #[should_panic]), `test_post_tool_use_failure_arm_unchanged`
- R-08: scenario 2 (`test_gemini_session_end_produces_session_close`) present; scenario 3 explicitly removed with documented rationale — `debug_assert!` fires before any match arm if `"SessionEnd"` reaches `build_request()` unnormalized. Removal documented in test-plan/normalization.md with note immediately after `test_gemini_session_end_produces_session_close`.

**Low (R-11, R-12, R-13)**: All covered in normalization.md (R-11), reference-configs.md (R-12), and wire-protocol.md (R-13).

Category 2 passthrough assertion: test-plan/normalization.md now correctly asserts `("__unknown__", "unknown")` for `cycle_start`, `cycle_stop`, `cycle_phase_end` with matching comment that explains these events are not inputs to `normalize_event_name`.

---

### Interface Consistency

**Status**: PASS

All four items that failed in Rework Iteration 1 are resolved:

**Item 1 — normalize_event_name function signature in test plan** (previously FAIL):
test-plan/normalization.md line 17 now shows: `normalize_event_name(event: &str) -> (&'static str, &'static str)` — the 1-argument form matching pseudocode/normalization.md. No 2-argument calls anywhere in the test plan.

**Item 2 — Category 2 passthrough assertion** (previously FAIL):
`test_normalize_event_name_category2_passthrough` (test-plan/normalization.md, around line 260-276) now asserts:
```rust
assert_eq!(canonical, "__unknown__");
assert_eq!(provider, "unknown");
```
Comment correctly explains these are never inputs to `normalize_event_name`. No `None` second argument in the call. Matches pseudocode specification exactly.

**Item 3 — defense-in-depth arm test** (previously FAIL):
The `test_gemini_session_end_defense_in_depth_arm` test is removed. The test plan includes an explicit removal note:
> "R-08 scenario 3 removed — the pseudocode uses a `debug_assert!` in `build_request()` instead of defense-in-depth arms. If `"SessionEnd"` reaches `build_request()` in a debug build, the assert fires (panics) before any match arm is reached. R-08 coverage is adequate via scenario 2..."

**Item 4 — wire-protocol.md stale reference** (previously WARN):
The "Initialization Sequence" section in wire-protocol.md (lines 111-131) now shows the two-path dispatch using `map_to_canonical` and `normalize_event_name` separately. No 2-argument `normalize_event_name` call present.

**Additional interface checks (full pass)**:
- `HookInput` fields: consistent across OVERVIEW.md shared types, wire-protocol.md, and all component references.
- `ImplantEvent.provider`: consistent definition and usage across all files.
- `DEFAULT_HOOK_SOURCE_DOMAIN`: defined in source-domain-derivation.md, referenced consistently by Sites B and C.
- `hook::run()` signature: `(event: String, provider: Option<String>, project_dir: Option<PathBuf>)` consistent across normalization.md and OVERVIEW.md.
- `build_request()` behavior: `debug_assert!` at entry consistent with OVERVIEW.md description and normalization.md design decision section.
- Site A two-location note: source-domain-derivation.md correctly identifies ambiguity in `listener.rs` (OQ-1) and provides pseudocode for both the dispatch-path and content_based_attribution_fallback cases. No contradiction.

---

### Knowledge Stewardship Compliance

**Status**: WARN

All four agent reports contain a `## Knowledge Stewardship` section. Three of four are fully compliant.

| Agent | Queried | Stored/Declined | Format Compliant |
|-------|---------|-----------------|------------------|
| vnc-013-agent-0-scope-risk | Yes (3 queries) | `Stored: nothing novel to store — {reason}` | PASS |
| vnc-013-agent-2-spec | Yes (1 query) | Not present (no stored entry) | WARN |
| vnc-013-agent-3-risk | Yes (1 query) | `Stored: nothing novel to store — {reason}` | PASS |
| vnc-013-agent-1-pseudocode | Yes (3 queries) | "Deviations from established patterns: none..." | WARN |

**vnc-013-agent-1-pseudocode**: The section has `Queried:` entries (evidence of pattern queries before design). However, the storage entry uses non-standard phrasing: "Deviations from established patterns: none. [rationale]" instead of the required `Stored: nothing novel to store -- {reason}`. The intent is equivalent but the format does not match the gate requirement.

**vnc-013-agent-2-spec**: The stewardship section has one `Queried:` entry but no `Stored:` or `Declined:` entry at all. Spec agents are read-only agents per the gate check set definition, so only `Queried:` is required. This is a PASS on re-read — the gate definition states "Read-only agents (pseudocode) have `Queried:` entries" and the spec agent is analogous to a read-only agent.

Revised assessment: vnc-013-agent-2-spec — PASS (read-only agent, `Queried:` present).
vnc-013-agent-1-pseudocode — WARN (storage format non-standard; content is adequate but format doesn't match required template).

These are WARNs, not FAILs. The stewardship block is present in all four reports and all show evidence of Unimatrix queries. No stewardship block is missing.

---

## Gate Decision

All five checks PASS or WARN. No FAIL items remain. The four previously-failing items from Rework Iteration 1 are resolved:

1. test-plan/normalization.md: `normalize_event_name` 1-argument calls throughout — RESOLVED
2. test-plan/normalization.md: Category 2 passthrough assertions corrected — RESOLVED
3. test-plan/normalization.md: Defense-in-depth arm test removed with documented rationale — RESOLVED
4. pseudocode/wire-protocol.md: Stale 2-argument call replaced with two-path dispatch — RESOLVED

The two WARNs (specification departures documented with rationale; pseudocode agent stewardship format) are acceptable per gate policy. WARNs do not block gate progression.

**Result: PASS**

---

## Knowledge Stewardship

- Stored: nothing novel to store -- gate failure patterns from this iteration are feature-specific (all four rework items were test-plan/pseudocode consistency gaps); no cross-feature pattern emerged beyond what is already captured in entries #3492 (blast-radius lesson) and #4298 (hook-normalization-boundary pattern). The Category 2 passthrough / sentinel return type design is vnc-013-specific.
