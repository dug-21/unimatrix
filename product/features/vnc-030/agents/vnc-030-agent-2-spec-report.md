# Agent Report — vnc-030-agent-2-spec

**Role**: Specification writer · **Date**: 2026-06-08 · **Feature**: vnc-030 (#699)

## Artifact

- `product/features/vnc-030/specification/SPECIFICATION.md`

## Coverage

- 28 functional requirements (FR-01..28), 8 non-functional (NFR-01..08), 13 constraints (C-01..13).
- All nine SCOPE acceptance criteria (AC-01..AC-09) mapped to requirements with verification methods.
- AC-07 methodology pinned per scope: accuracy denominator = declared protocol sessions only; never-declare sessions only in the fallback regression sample; strengthened per SR-06 (multiple never-declare shapes + before/after live-DB `topic_source` distribution).
- SR risks folded in: SR-01 (pinned CLI 2.1.167 + canary trigger semantics, FR-10/NFR-08/C-12), SR-02 (size budget NFR-01/OQ-C), SR-03 (#3486 both-ends round-trip, FR-13), SR-04 (`topic_source` write-site table, FR-21), SR-05 (registry fence, C-09), SR-06 (AC-07 strengthening), SR-07 (AC-04 "marker when present" wording + follow-up-issue-before-gate-exit, C-13), SR-08 (#588 resolved/residue list, FR-26), SR-09 (seam-survival assertion, FR-28), SR-10 (minimal-diff + crt-052 citable interface, FR-18/C-10), SR-12 (#574 no-race + expiry condition, FR-27).
- Carry-item AC-09 (protocol re-declaration line, FR-24), drive-by docstring fix (FR-25), #588 disposition (FR-26), #574 no-race verification (FR-27) all specified.
- Worktree probe consumed: AC-08 asserts existing F3 gitdir-port behavior; C-11 forbids raw-cwd hashing.

## Key Decisions

1. `topic_source` write-site mapping pinned as a normative table (FR-21) — one source per code path; the `vote` row-level write site is the one ambiguity, escalated as OQ-A rather than guessed.
2. `stamp_miss` canary defined as an observability invariant, not an alert system (FR-10): nonzero growth in a declared session = investigation-triggering; F6 gate must consult it. Active surfacing left to the architect (OQ-B) due to size/fail-open budgets.
3. AC-04 worded "stamp → (marker when present) → vote-on-NULL" so the missing MARKER tier is explicit (SR-07); follow-up issue existence made a gate-exit constraint (C-13).
4. #588 close decision made mechanical (FR-26): explicit resolved list (stamped write-time inversion, both close/sweep inversions, unstamped-window FeatureSource) vs residue list (historical rows, unstamped tally noise, hookless clients).

## Open Questions (for architect / leader)

- OQ-A: row-level write site for `topic_source='vote'`.
- OQ-B: canary surfacing mechanics within budgets.
- OQ-C: post-vnc-027 byte headroom + fallback (estimate table required in ARCHITECTURE.md).
- OQ-D: marker-recovery follow-up issue must be filed before design gate exit (leader action).

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned vnc-027 ADR-005 (C-04 gate: comment-stripped 100,000 B primary + raw 160,000 B backstop) and ADR-001 (server-side preformatted UDS sync responses), both used in NFR-01/OQ-C; remaining hits (vnc-013/015/018-020 ADRs) not directly applicable.
