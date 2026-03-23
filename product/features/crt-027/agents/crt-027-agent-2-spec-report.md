# Agent Report: crt-027-agent-2-spec

## Output

- **SPECIFICATION.md**: `product/features/crt-027/specification/SPECIFICATION.md`
- **AC count**: 25 (AC-01 through AC-25, plus AC-SR01)

## Key Decisions Made

1. **SR-01 treated as BLOCKING open question** — AC-SR01 added with explicit OPEN
   status. Architecture cannot proceed on WA-4a without confirming Claude Code injects
   SubagentStart stdout. Marked as `OQ-SR01 (BLOCKING for architecture)` in the spec.

2. **SR-04 enumerated as AC-16 through AC-21** — Six individual ACs covering each
   surviving invariant from `format_compaction_payload` tests: byte budget (AC-16),
   UTF-8 truncation (AC-17), empty result handling (AC-18), confidence sort order
   (AC-19), token limit override (AC-20), histogram block (AC-21). Old
   `CompactionCategories`-based tests are deleted; their invariants are re-expressed for
   the flat table.

3. **SR-05 expressed as AC-22 and AC-23** — AC-22 covers exact 4-word / 5-word boundary
   for UserPromptSubmit. AC-23 confirms SubagentStart with a 1-word prompt is unaffected
   by `MIN_QUERY_WORDS` and still routes to ContextSearch.

4. **SR-03 (EffectivenessStateHandle) made a hard compile requirement** — FR-15 and the
   `IndexBriefingService` domain model both specify that `EffectivenessStateHandle` is a
   required constructor parameter, not optional. Missing wiring is a compile error.

5. **SR-02 feature flag** — AC-24 requires `cargo test` without `--features mcp-briefing`
   to pass all `handle_compact_payload` tests. FR-14 confirms the flag guards only the
   MCP tool registration; `IndexBriefingService` compiles unconditionally (C-07).

6. **UNIMATRIX_BRIEFING_K deprecated (not replaced)** — FR-13 and C-08 specify the env
   var is deprecated and ignored on the new index path. No replacement var is
   introduced. A code comment is required at the removal point.

7. **`derive_briefing_query` shared helper** — AC-10 and FR-11 require both call sites
   to use the same shared function to prevent query derivation divergence.

## Open Questions

- **OQ-SR01 (BLOCKING)**: SubagentStart stdout injection unverified — architect must
  confirm before delivery.
- **OQ-SR08 (Low risk)**: Step-3 fallback quality for feature-ID queries that return
  zero knowledge-base results.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `hook injection SubagentStart ContextSearch
  knowledge delivery specification pattern` — found #3230 (SubagentStart routing
  pattern, crt-027 pre-existing) and #3231 (BriefingService caller map, crt-027
  pre-existing). No conflicting AC patterns from prior features found.
