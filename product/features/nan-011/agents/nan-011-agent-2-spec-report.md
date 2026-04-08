# Agent Report: nan-011-agent-2-spec

Feature: nan-011
Role: Specification Writer

## Output

`product/features/nan-011/specification/SPECIFICATION.md`

## Key Decisions Made

### SR-06 Handling
The approved vision statement (verbatim, AC-01) includes the phrase "before agents need
to ask for it". The SCOPE risk assessment (SR-06) flagged this as overselling current
capability. The specification resolves this by requiring (FR-1.2) a mandatory
one-sentence qualifier placed immediately after the vision block in README, clarifying
that delivery is workflow-phase-conditioned, not unconditional injection. This satisfies
SR-06 without modifying the approved verbatim text.

### Bare Invocation Detection Pattern (SR-04)
The SCOPE defines AC-10 as "zero bare tool name invocations" but noted that a naive grep
would catch prose references too (false positives). The specification (FR-8.2, FR-8.3,
AC-10 pass criteria) distinguishes: the prohibited pattern is the tool name immediately
followed by `(` — the invocation form. Prose occurrences without `(` are exempt. The
AC-10 grep pattern provided is precise to invocation context only.

Known existing violation confirmed: `uni-seed` SKILL.md line 49 contains
`context_status()` as a bare call. This is explicitly noted in FR-8.3 as a known
violation the implementer must fix.

### Dual-Copy Maintenance (SR-03)
The specification makes the dual-copy obligation explicit as both a functional
requirement (FR-11.3) and a non-functional requirement (NFR-4) with a specific ordered
step: edit source first, then copy, then diff to confirm identity. This is required
before the PR is opened.

### INITIAL_CATEGORIES Authority
FR-10.3 and AC-17 specify that the implementer must read `categories/mod.rs` at
delivery time. The specification states the current array (5 categories) as a reference
but explicitly notes that `mod.rs` is the authority, not the specification.

### config.rs Verification Artifact (SR-01)
FR-7.2 requires the implementer to produce a verification artifact (inline comment or
delivery checklist table) confirming each default value was read directly from `config.rs`.
This addresses the SR-01 risk of default divergence without adding a non-spec deliverable.

## Findings on Current State

- `uni-retro` SKILL.md: no HookType, NLI, or MicroLoRA references found — AC-12 likely
  passes with no changes needed. Implementer should verify.
- Protocols: grep found zero matches for NLI, HookType, or unimatrix-server in
  `.claude/protocols/uni/` — AC-15 likely passes. Implementer must verify all four files.
- `packages/unimatrix/package.json` already has a `files` array including `skills/` —
  the uni-retro distribution path can follow the same pattern.
- Most skills already use full-prefix invocations. The audit may find only the one
  confirmed violation (uni-seed line 49).

## Open Questions

None forwarded to downstream agents.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — 17 results returned; hit #4148
  (lesson-learned: config.rs field type divergence risk) directly informed the SR-01
  treatment. nan-005 ADRs provided README structural conventions. No new generalizable
  patterns identified.
