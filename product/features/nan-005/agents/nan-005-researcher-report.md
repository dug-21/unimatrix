# nan-005-researcher Report

## Agent ID
nan-005-researcher

## Task
Explore the problem space for nan-005 (Documentation & Onboarding) and produce SCOPE.md.

## Artifacts Produced
- `/workspaces/unimatrix-nan-005/product/features/nan-005/SCOPE.md`

## Key Findings

### 1. README exists but is stale and internally-focused
The current README.md (~310 lines) was written for Unimatrix developers, not adopters. It contains multiple stale references to redb (the project migrated to SQLite in nxs-008). The architecture table, data layout, project structure, and crate descriptions all reference the old backend. The tool table lists 11 entries despite the product having 12 tools.

### 2. No documentation update mechanism exists
Neither the design nor delivery protocols include any step for documentation updates. The README was written once and has drifted. Without a systematic mechanism, every shipped feature increases the gap between documentation and reality.

### 3. Documentation agent fits naturally in delivery Phase 4
The delivery protocol's Phase 4 (after code, tests, and gates pass) is the right insertion point. The documentation agent would run after PR creation but before `/review-pr`, so documentation updates are included in the reviewed PR. This is additive -- no existing phases or gates need restructuring.

### 4. Skills are the primary user interaction surface but undocumented
14 skills exist in `.claude/skills/`. These are the primary way users interact with Unimatrix (via slash commands), but neither the README nor any external document lists them with trigger conditions and usage guidance.

### 5. Operational constraints are learned the hard way
Session boundaries, feature cycle naming, phase prefixes in commits, category discipline, and cold-start mitigation are all constraints that affect user experience but are not documented anywhere accessible to new users. They exist in CLAUDE.md (for Unimatrix's own development) and in scattered skill files.

### 6. nan-005 is markdown-only -- no code changes
Unlike nan-001/002 (CLI subcommands) or nan-004 (npm packaging), nan-005 produces only markdown artifacts and protocol edits. No Rust code, no schema changes, no new tools.

## Scope Boundaries

**In scope:**
- README.md rewrite (comprehensive, external-facing)
- MCP tool reference (all 12 tools with usage guidance)
- Skills reference (all 14 skills with trigger conditions)
- Operational constraints documentation
- Documentation agent definition (`uni-docs.md`)
- Delivery protocol modification (optional doc update step)

**Out of scope:**
- API docs / rustdoc
- Tutorials or guided walkthroughs
- Documentation website
- Changelog (nan-004)
- Per-repo onboarding (nan-003)
- Internal development workflow documentation

## Open Questions
1. Documentation agent placement: before or after `/review-pr`? (Recommendation: before)
2. README length management: single file vs. split? (Recommendation: single file, split at ~800 lines)
3. Stale detection scope: incremental per-feature vs. full validation? (Recommendation: incremental + periodic manual)
4. npm install path: document speculatively or wait for nan-004? (Recommendation: document both paths with note)

## Knowledge Stewardship
- Queried: /query-patterns for "documentation conventions README onboarding" -- no directly relevant patterns found (5 results, none about documentation maintenance)
- Queried: /query-patterns for "documentation agent protocol auto-update docs" -- no relevant patterns found
- Stored: entry "README documentation drift pattern: stale backend references survive major migrations" via /store-pattern
