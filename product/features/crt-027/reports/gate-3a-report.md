# Gate 3a Report: crt-027

> Gate: 3a (Design Review)
> Date: 2026-03-23
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 8 components map cleanly to ARCHITECTURE.md; ADRs 001–006 faithfully reflected |
| Specification coverage | PASS | All 19 FRs and 25 ACs have corresponding pseudocode; no scope additions |
| Risk coverage | PASS | All 14 risks (R-01 through R-14) have test plans; 15 non-negotiable test names present |
| Interface consistency | WARN | `IndexBriefingParams` OVERVIEW.md shared-type definition omits `category_histogram` field; per-component pseudocode resolves it correctly |
| Knowledge stewardship | PASS | Both agent reports have `## Knowledge Stewardship` with `Queried:` entries; testplan agent stored pattern #3253 |

---

## Detailed Findings

### Architecture Alignment

**Status**: PASS

**Evidence**:

Each component pseudocode file maps to the corresponding ARCHITECTURE.md component section and all six ADRs.

**ADR-002** — SubagentStart arm placement: `hook-routing.md` explicitly instructs placement "before the `_` fallthrough" (line 40-41). The pseudocode match block shows `"SubagentStart" =>` before `_ =>`. `MIN_QUERY_WORDS=5` constant is defined at module level and applied only to the `"UserPromptSubmit"` arm. SubagentStart retains only `.trim().is_empty()` guard. Correct per ADR-002.

**ADR-006** — `write_stdout_subagent_inject`: `hook-routing.md` specifies the JSON envelope `{"hookSpecificOutput": {"hookEventName": "SubagentStart", "additionalContext": entries_text}}` and branches in `run()` on `req_source.as_deref() == Some("SubagentStart")`. UserPromptSubmit path retains plain-text `write_stdout`. Correct per ADR-006.

**ADR-001** — `source` field: `wire-source-field.md` specifies `#[serde(default)] source: Option<String>` added to `ContextSearch`. `listener-dispatch.md` uses `source.as_deref().unwrap_or("UserPromptSubmit").to_string()` for the observation hook column. Correct per ADR-001.

**ADR-003** — `IndexBriefingService`: `index-briefing-service.md` constructor requires `effectiveness_state: EffectivenessStateHandle` as non-optional (no `Option<>` wrapping). `default_k: 20` is hardcoded in `new()`. No `UNIMATRIX_BRIEFING_K` read. `service-layer-wiring.md` shows deprecation comment at the `parse_semantic_k()` removal site. Correct per ADR-003.

**ADR-004** — CompactPayload migration: `listener-dispatch.md` deletes `CompactionCategories`, rewrites `format_compaction_payload` to accept `&[IndexEntry]`, preserves histogram block and session context header. All 11 named tests listed. Correct per ADR-004.

**ADR-005** — `IndexEntry` typed contract: `index-entry-formatter.md` defines `IndexEntry` with exactly the five fields specified (id, topic, category, confidence, snippet), `SNIPPET_CHARS = 150`, and `format_index_table`. The `format_retrospective_report` function is explicitly retained. Correct per ADR-005.

**Component boundary fidelity**: The OVERVIEW.md sequencing constraints (wire-source-field → index-entry-formatter → index-briefing-service → service-layer-wiring → parallel hook-routing/listener-dispatch/context-briefing-handler) match the dependency graph in ARCHITECTURE.md.

---

### Specification Coverage

**Status**: PASS

**Evidence**:

All 19 functional requirements (FR-01 through FR-19) are addressed:

- **FR-01/FR-02**: `hook-routing.md` SubagentStart arm with `prompt_snippet` extraction and `.trim().is_empty()` guard.
- **FR-03**: `wire-source-field.md` adds `#[serde(default)] source: Option<String>`; `listener-dispatch.md` uses it for observation tagging.
- **FR-04**: `hook-routing.md` confirms `is_fire_and_forget` excludes `ContextSearch` (no change needed).
- **FR-04b**: `hook-routing.md` `write_stdout_subagent_inject` and `run()` branching on `req_source`.
- **FR-05**: `MIN_QUERY_WORDS: usize = 5` constant; UserPromptSubmit arm uses `.trim().split_whitespace().count()`.
- **FR-06**: `hook-routing.md` error handling section specifies exit code always 0.
- **FR-07/FR-08/FR-09**: `index-briefing-service.md` — service returns Active-only, delegates to SearchService for fused ranking.
- **FR-10**: `IndexEntry` with `snippet = entry.content.chars().take(SNIPPET_CHARS).collect()`.
- **FR-11**: `derive_briefing_query` free function with three-step priority defined in `index-briefing-service.md`.
- **FR-12**: `format_index_table` flat table columns in `index-entry-formatter.md`.
- **FR-13**: `default_k = 20` hardcoded; `UNIMATRIX_BRIEFING_K` deprecated, not read; deprecation comment at removal site.
- **FR-14**: `BriefingParams` schema struct unchanged in `context-briefing-handler.md`; `role` ignored but accepted.
- **FR-15**: `EffectivenessStateHandle` is required, non-optional constructor parameter.
- **FR-16/FR-17**: `listener-dispatch.md` migrates `handle_compact_payload` to `IndexBriefingService`, preserves histogram block.
- **FR-18**: `BriefingService` and all related types listed in OVERVIEW.md "Deleted Structures" section; no dead code.
- **FR-19**: `protocol-update.md` specifies all 6 insertion points with `max_tokens: 1000`.

Non-functional requirements: NFR-01 (HOOK_TIMEOUT preserved), NFR-02 (existing MCP timeout), NFR-03 (budget enforcement in `format_compaction_payload`), NFR-04 (UTF-8 boundary via `.chars().take()`), NFR-05 (`IndexBriefingService` not gated), NFR-06 (tests rewritten not deleted), NFR-07 (`max_tokens: 1000` on all SM calls) — all addressed in pseudocode.

No scope additions: none of the 8 pseudocode files implement features outside the crt-027 scope. All NOT-IN-SCOPE items from SPECIFICATION.md are absent from the pseudocode.

---

### Risk Coverage

**Status**: PASS

**Evidence**:

All 14 risks from RISK-TEST-STRATEGY.md have corresponding test scenarios in the test plans.

**Critical risks (R-01, R-03)**:
- **R-01** (source field backward compat): 5 scenarios covered across `wire-source-field.md` (deserialization default, explicit value, round-trip, compile surface) and `listener-dispatch.md` (observation tagging for all three paths).
- **R-03** (format_compaction_payload test coverage): All 11 named tests present in `listener-dispatch.md`. Matching the RISK-TEST-STRATEGY enumeration exactly.

**High risks (R-02, R-04, R-05, R-06, R-07)**:
- **R-02**: EffectivenessStateHandle — constructor compile-time check + `index_briefing_service_effectiveness_influences_ranking` in `index-briefing-service.md`.
- **R-04**: MIN_QUERY_WORDS boundary — 6 scenarios in `hook-routing.md` including 4-word/5-word exact boundary tests.
- **R-05**: WA-5 format contract — 4+ scenarios in `index-entry-formatter.md` including `format_index_table_exact_column_layout` asserting literal column values.
- **R-06**: Query derivation divergence — 6 scenarios in `index-briefing-service.md` testing all three steps of `derive_briefing_query`; code inspection confirmation of single shared helper.
- **R-07**: SubagentStart stdout injection — manual smoke test (AC-SR01) documented; automated unit tests for `write_stdout_subagent_inject` JSON envelope (AC-SR02/SR03); graceful degradation test.

**Medium risks (R-08 through R-12)**: Each has 3 scenarios. Feature flag (R-08): two CI runs documented in `service-layer-wiring.md`. UNIMATRIX_BRIEFING_K (R-09): runtime test + static grep gates. Cold-state (R-10): empty result test. Protocol completeness (R-11): static grep count. Observation mismatch (R-12): three tagging paths.

**Low risks (R-13, R-14)**: R-13 (`HookRequest::Briefing` not removed) has compile-time assertion test. R-14 (empty CompactPayload) covered by `format_payload_empty_entries_returns_none` and histogram-only case.

**15 non-negotiable test names** from RISK-TEST-STRATEGY all present in test plans:
- `format_payload_empty_entries_returns_none` — `listener-dispatch.md` T-LD-04
- `format_payload_header_present` — T-LD-05
- `format_payload_sorted_by_confidence` — T-LD-06
- `format_payload_budget_enforcement` — T-LD-07
- `format_payload_multibyte_utf8` — T-LD-08
- `format_payload_session_context` — T-LD-09
- `format_payload_active_entries_only` — T-LD-10
- `format_payload_entry_id_metadata` — T-LD-11
- `format_payload_token_limit_override` — T-LD-12
- `test_compact_payload_histogram_block_present` — T-LD-13
- `test_compact_payload_histogram_block_absent` — T-LD-14
- `build_request_subagentstart_with_prompt_snippet` — `hook-routing.md` T-HR-01
- `build_request_subagentstart_empty_prompt_snippet` — T-HR-02
- `build_request_userpromptsub_four_words_record_event` — T-HR-06
- `build_request_userpromptsub_five_words_context_search` — T-HR-07

Integration harness section in test-plan/OVERVIEW.md: present. Identifies five required suites (smoke, tools, lifecycle, edge_cases, protocol), lists six new integration tests by function signature, and explains when to file a follow-up vs. expand harness scope.

---

### Interface Consistency

**Status**: WARN

**Evidence**:

The OVERVIEW.md `Shared Types` section defines `IndexBriefingParams` with four fields:
```
pub(crate) struct IndexBriefingParams {
    pub query: String,
    pub k: usize,
    pub session_id: Option<String>,
    pub max_tokens: Option<usize>,
}
```

The per-component `index-briefing-service.md` notes and resolves a discrepancy: `ServiceSearchParams` requires a pre-resolved `category_histogram`, so `IndexBriefingParams` must gain a fifth field `category_histogram: Option<HashMap<String, u32>>`. This resolution is documented inline in the pseudocode and in the agent report (OQ-1). The callers (`context-briefing-handler.md` and `listener-dispatch.md`) already reference `category_histogram` as part of `IndexBriefingParams`.

**Issue**: The OVERVIEW.md shared types table is stale — it shows the four-field version, not the five-field version that the per-component pseudocode correctly resolves. An implementation agent reading only OVERVIEW.md could miss the fifth field.

**Why WARN not FAIL**: The resolution is unambiguous and documented in the pseudocode that the implementation agent will read. The per-component files are authoritative for implementation. This does not block delivery, but the implementer should note OQ-1 explicitly.

All other shared types are consistent across OVERVIEW.md and the per-component pseudocode:
- `IndexEntry` (5 fields) — identical definition in OVERVIEW.md and `index-entry-formatter.md`
- `IndexBriefingService` struct — identical in OVERVIEW.md and `index-briefing-service.md`
- `HookRequest::ContextSearch` extended form — identical in OVERVIEW.md and `wire-source-field.md`
- `format_compaction_payload` updated signature — identical in OVERVIEW.md and `listener-dispatch.md`

Cross-component data flow: the OVERVIEW.md data flow diagrams are consistent with the individual pseudocode files. The three-step query derivation shared helper is referenced uniformly in both `context-briefing-handler.md` and `listener-dispatch.md` as `crate::services::index_briefing::derive_briefing_query`.

---

### Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

**crt-027-agent-1-pseudocode-report.md** (read-only/pseudocode agent):
```
## Knowledge Stewardship
- Queried: `/uni-query-patterns` for "hook routing injection patterns conventions" — found
  entries #3230, #281, #314, #321
- Queried: `/uni-query-patterns` for "crt-027 architectural decisions" — found entries
  #3242-#3246 (ADR entries)
```
Satisfies the `Queried:` requirement for read-only agents.

**crt-027-agent-2-testplan-report.md** (read-only/test-plan agent):
```
## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for crt-027 architectural decisions — found 5 ADR entries
- Queried: `/uni-knowledge-search` for hook routing injection testing patterns — found #315, #252, #264, #2928, #314
- Stored: entry #3253 "Non-Negotiable Test Name Verification Pattern for Rewritten Test Suites"
```
Satisfies the `Queried:` requirement, and the agent went beyond by storing a novel pattern.

Both agent reports have a `## Knowledge Stewardship` section. No missing blocks.

---

## Implementation Notes for Stage 3b

The following items do not block gate passage but should be read by implementation agents before coding:

1. **OQ-1 (category_histogram field)**: Add `category_histogram: Option<HashMap<String, u32>>` to `IndexBriefingParams`. Both callers must pre-resolve via `session_registry.get_category_histogram()`.

2. **OQ-2 (session_registry on MCP server struct)**: Verify whether `UnimatrixServer` holds a `session_registry` field. If absent, the implementation agent must add it or accept that step 2 of query derivation degrades to step 3. Flag the resolution in the implementation notes.

3. **AC-SR02/SR03 testability**: Recommend implementing `write_stdout_subagent_inject` with a `Write` impl parameter to enable deterministic testing without process-level stdout capture.

4. **Empty `source` string**: `source: Some("")` would write `hook = ""` to observations table. Consider clamping `Some("")` to `"UserPromptSubmit"`. No AC covers this; implementation agent decides.

5. **`context_search_is_not_fire_and_forget` existing test**: Must add `source: None` to the struct literal after the `source` field is added (compile error if not done).

---

## Knowledge Stewardship

- Queried: `context_search` for "gate 3a validation pseudocode design review patterns" — found entries #230, #114, #141, #122 (validator duties and glass box conventions).
- Queried: `context_search` for "gate failure rework patterns lesson learned review" — found entries #1203, #167, #142 (gate result handling, iteration cap, cascading rework lesson).
- Stored: nothing novel to store — findings are feature-specific and belong in this gate report. The WARN pattern (OVERVIEW.md shared types diverging from per-component resolution) is a common design artifact, already captured in existing conventions.
