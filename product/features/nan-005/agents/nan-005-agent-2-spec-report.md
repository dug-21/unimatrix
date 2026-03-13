# Agent Report: nan-005-agent-2-spec

## Summary

Produced SPECIFICATION.md for nan-005 (Documentation & Onboarding).

## Key Decisions Made

**1. Tool count discrepancy surfaced as OQ-01**
SCOPE.md claims 12 tools; `tools.rs` contains exactly 11 `#[tool(...)]` annotated handlers. The README must state the verified count, not the SCOPE.md estimate. Raised as Open Question OQ-01 for the implementation agent to confirm before authoring.

**2. FR-04e: `maintain` parameter documented as silently ignored**
Code inspection of `tools.rs` (lines 776-797) reveals that as of col-013, the `maintain` parameter on `context_status` is silently ignored and background tick handles all maintenance. SCOPE.md describes the old behavior (inline maintenance). The spec requires documenting the actual current behavior to satisfy AC-12 (factual accuracy).

**3. SR-05 mitigation: mandatory vs. optional criteria specified**
SCOPE-RISK-ASSESSMENT.md flagged pure optionality as a risk. The spec defines explicit mandatory trigger criteria (new MCP tool, skill, CLI subcommand, category, schema version) and skippable conditions (pure refactor, test-only). This prevents the documentation agent step from being universally skipped.

**4. SR-04 mitigation: nan-003 boundary preserved**
The operational guidance section references `/unimatrix-init` and `/unimatrix-seed` rather than restating their content. The spec explicitly names this as a constraint (C-06).

**5. Fact Verification Checklist added**
Addresses SR-01 (accuracy risk). Every numeric claim the README will make is listed with its verification command. The implementation agent must populate this checklist before authoring.

**6. `unimatrix-learn` crate included**
Current workspace has 9 crates (`ls crates/` shows `unimatrix-learn` in addition to the 8 listed in the current README). FR-08c requires the correct count. Added to the crate list and flagged in OQ-03.

**7. Schema version v11 confirmed**
`migration.rs` contains `CURRENT_SCHEMA_VERSION: u64 = 11`. The README currently says "17-table" (a redb-era claim). The correct SQLite table count is 19 (from `grep -c 'CREATE TABLE IF NOT EXISTS' db.rs`).

## Open Questions for Architect/Implementer

- **OQ-01**: Confirm exact tool count from `tools.rs`. Is there a 12th tool in a separate file?
- **OQ-02**: Should MicroLoRA technical details (InfoNCE, EWC++) appear in README Core Capabilities or be replaced with user-facing framing only?
- **OQ-03**: What is `unimatrix-learn`'s user-facing role? How should it be described in the crate table?

## Output

- `/workspaces/unimatrix-nan-005/product/features/nan-005/specification/SPECIFICATION.md`

## Knowledge Stewardship

- Queried: `/query-patterns` for documentation agent and delivery protocol patterns -- no results; this is the first documentation-focused feature. Patterns from this feature (README accuracy discipline, documentation agent behavior) are candidates for retrospective extraction after delivery.
