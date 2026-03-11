# vnc-011 Retrospective Architect Report

Agent: vnc-011-retro-architect
Mode: Retrospective (post-ship knowledge extraction)

## 1. Patterns

### NEW: Domain-Specific Markdown Formatter Module Pattern (Unimatrix #949)

The `retrospective.rs` module establishes a reusable pattern for complex markdown formatters in the response layer. Distinct from the existing Generic Formatter Pattern (#298, which covers parameterized wrappers for near-identical operations like status changes), this pattern covers domain-specific formatters with:
- Private internal structs for intermediate representation (e.g., `CollapsedFinding`)
- A single public orchestrator function (`format_{domain}_markdown`)
- Multiple private `render_{section}()` helpers, each responsible for one markdown section
- Conditional section assembly based on Option/empty checks
- Immutable consumption of the source struct

Applicable when: A future MCP tool needs a markdown format that involves grouping, collapsing, filtering, or transforming a complex struct -- not just serializing it.

### EXISTING: Generic Formatter Pattern (#298) -- No update needed

Still accurate. The retrospective formatter is a different pattern (domain-specific vs. parameterized wrappers). #298 applies to status-change-like operations; #949 applies to complex report formatting.

### SKIPPED: Response module one-concern-per-module convention

Already implicitly documented via ADR-003 (#952). The response/ layer pattern (entries.rs, mutations.rs, status.rs, briefing.rs, retrospective.rs) is an emergent convention, not a standalone pattern. ADR-003 references it as rationale.

## 2. Procedures

### No new procedures identified

The question was whether "adding a new output format to an MCP tool" is a reusable procedure. Analysis: vnc-011 added a `format` parameter to `RetrospectiveParams` with string matching in the handler. Other tools use the `ResponseFormat` enum (Summary/Markdown/Json). The retrospective tool deliberately did NOT use `ResponseFormat` because it has only two formats and a different default. This is tool-specific, not a generalizable procedure. If a future tool needs the same two-format pattern, the Domain-Specific Markdown Formatter Module Pattern (#949) covers the structural approach.

Existing procedure search returned no relevant matches for response formatting (closest was #296, Service Extraction Procedure, which is unrelated).

## 3. ADR Validation

### ADR-001: Format-Dependent evidence_limit Default -- FLAGGED (stale text)

**Unimatrix entry**: #950 (stored with amendment note)

The ADR file (`ADR-001-format-dependent-evidence-limit-default.md`) says `unwrap_or(0)`. The shipped code uses `unwrap_or(3)` per human override. The ADR was never updated after the human override. The Unimatrix entry (#950) documents this discrepancy.

**Status**: Validated in spirit (format-dependent defaults are correct), but the ADR file text is stale. The file should be updated to reflect the shipped behavior (`unwrap_or(3)`) or annotated with the human override. No supersession needed -- the decision concept is sound, only the specific default value is wrong in the file.

### ADR-002: Deterministic Example Selection -- VALIDATED

**Unimatrix entry**: #951

Fully validated by implementation. Evidence pool sorted by `e.ts` ascending (line 275), first 3 taken (line 277). All 5 evidence selection tests pass. The decision to use timestamps over randomness proved correct -- it enabled snapshot testing throughout the test suite (80 formatter unit tests, many using exact string assertions).

### ADR-003: Separate Retrospective Module -- VALIDATED

**Unimatrix entry**: #952

Fully validated. The module shipped at 446 lines of production code (under the 500-line budget). The separation from briefing.rs kept both modules focused. The feature gate and mod.rs registration worked as designed. Gate 3b confirmed compliance.

## 4. Lessons

### NEW: Human overrides must propagate to all downstream artifacts (Unimatrix #953)

**Source**: Gate 3a REWORKABLE FAIL (2 issues, both traced to stale pre-override values in pseudocode)

The human overrode `evidence_limit` default from `unwrap_or(0)` back to `unwrap_or(3)`. The override was recorded in the IMPLEMENTATION-BRIEF but not propagated to the ADR file or OVERVIEW data flow. Pseudocode agents consumed the stale architecture artifacts and reproduced the wrong value. Gate 3a caught this, requiring a rework iteration.

**Generalizable**: Yes. Any human override that changes a value established in an ADR or architecture document creates a propagation gap. The fix is either:
1. Propagate the override to all source artifacts (ADR, ARCHITECTURE.md, OVERVIEW)
2. Instruct pseudocode agents to treat IMPLEMENTATION-BRIEF Resolved Decisions as authoritative over ADR text

This lesson applies to any future feature where a human modifies a design decision after architecture is written but before pseudocode is generated.

## 5. Retrospective Findings Summary

**Ship quality**: Clean. No hotspots, no baseline outliers, all gates passed (3a on second attempt, 3b and 3c on first).

**Gate 3a rework**: The only friction point. Root cause was artifact propagation gap for human overrides. Two issues found and fixed in one rework iteration. Both were value-level errors (wrong default constant), not structural problems. The gate system worked as intended -- it caught the inconsistency before implementation.

**Code quality signals**:
- 446 lines production, 1263 lines tests (80 unit + 3 integration specific to vnc-011)
- No `.unwrap()` in production code
- One defensive improvement over pseudocode (`claims.first().map_or()` instead of `claims[0]`)
- One refactor from pseudocode (shared `sigma_string` helper to eliminate duplication)
- One addition not in pseudocode (`"summary"` as alias for `"markdown"` for consistency with other tools)

**ADR gap**: All three ADRs were written as files during design but never stored in Unimatrix. This retrospective corrected that (entries #950, #951, #952). ADR-001's file text remains stale relative to the shipped behavior.

## Unimatrix Entries Created

| ID | Type | Title |
|----|------|-------|
| #949 | pattern | Domain-Specific Markdown Formatter Module Pattern |
| #950 | decision | ADR-001: Format-Dependent evidence_limit Default (AMENDED BY HUMAN OVERRIDE) |
| #951 | decision | ADR-002: Deterministic Example Selection via Timestamp Ordering |
| #952 | decision | ADR-003: Separate Retrospective Formatter Module |
| #953 | lesson-learned | Human overrides must be propagated to ALL downstream artifacts |
