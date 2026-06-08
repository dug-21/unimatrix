# vnc-030 Pseudocode Agent Report — vnc-030-agent-1-pseudocode

## Deliverables

OVERVIEW.md + 9 per-component pseudocode files under
`product/features/vnc-030/pseudocode/`. All anchored to real file:line against the
merged-vnc-027 tree. No TODO/placeholder; gaps flagged explicitly per file.

Components covered: cycles, index-decoration (incl. UDS-stamp regression seam),
state-canary, wire-cycle-stamp, feature-source, listener-stamp-read,
topic-source-migration, protocol-redeclaration, docstring-driveby.

## OQ resolutions surfaced for delivery

- **OQ-A (topic_source='vote' write site)**: pinned in listener-stamp-read.md and
  feature-source.md. NO dedicated row-level vote write exists. `'vote'` rows are
  reachable ONLY via the enrich decision-tree `Inferred(Voted)` arm (unstamped +
  no extraction + registry feature set by eager #198). Session-level majority vote
  resolves `sessions.feature_cycle`, never rows. FR-21 one-source-per-write-site
  holds. Delivery confirms at Gate 3.
- **OQ-E (canary independence)**: the production increment call site lives in
  index-decoration.md (`subagentContext` → `bumpStampMiss`); `bumpStampMiss`
  itself (state-canary.md) is branch-agnostic. Test-time invariant ships either
  branch; Branch B gates the production call to no-op. Probe pins which stdin field
  marks depth≥1.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) and
  context_briefing -- surfaced ADR-001..006 entries (#4836/#4837/#4815/#4816/#4829),
  vnc-027 hook-client trap (#4822), hook-normalization pattern (#4298). All ADRs
  also read from files. No additional reusable pattern needed beyond those already
  stored.
- Deviations from established patterns: none. Pseudocode follows pattern #4772
  (sanitize-inside), #4092 (pragma-guarded ALTER), #3486 (per-site round-trip +
  shared helper), vnc-026 content-free breadcrumb, F3 fail-open atomics.
