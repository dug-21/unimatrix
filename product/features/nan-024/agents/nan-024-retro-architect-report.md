# nan-024-retro-architect report

Mode: retrospective. Feature: nan-024 (PR #1033, merged as 7c7a573e).

Unimatrix entries consulted: #5894, #5896, #5897, #5898, #5900, #5901, #5902-#5909, #2658, #4796, #5784, #5788, #5790, #4682

## Stewardship (entries from this cycle)
- #5904 -> #5910: removed the stale claim that "shim-entry runs trust first" (it was fixed); added the D-032 rule that non-Linux installed means a credential.
- #5905 -> #5911: added the home-guard.js assertTmpHome guard, the root cause, and What/Why/Scope.
- #5906 -> #5912: item 1 contradicted #5907 (Get-Acl), so it now points to #5907; latency item updated for D-096.
- #5902, #5903, #5907, #5908 kept as stored.

## Patterns
- New #5914: Windows harness commands must use node.exe plus an absolute script path, never a cmd-wrapped quoted .cmd.
- New #5913: rewriting a user config via tmp+rename must copy the existing file's mode (lib/atomic-write.js).

## Procedures
- #5790 -> #5919: CI topology updated (Layer 2 and latency now in CI, test-count guard, ci.yml is PR-only), plus D-032 fixture rules, the darwin-spoof smoke, and running ci.yml before 3c.

## Lessons
- #4796 -> #5915: third recurrence of platform failures first surfacing on the PR's CI after Gate 3c; 3c must cite a ci.yml run on HEAD.
- New #5916: latency ACs on shared runners: measure first, then gate on the median or report only.
- #2658 -> #5917 -> #5918: sleep-polling extended to subagent and CI waits.

## ADR status
- Validated: ADR-001 #5896 (with D-072), ADR-002 #5900 (D-090), ADR-003 #5897, ADR-004 #5901, ADR-006 #5894.
- ADR-005 #5898 contradicted D-094 (absolute per-OS budgets) and D-096 (report-only). The pre-Node check is disabled on every OS (`lib/install-store.js` `PRENODE_ENABLED = false`); the POSIX filter runs only on the node-not-found path. Corrected as #5924 at the coordinator's direction (commit 37f249fd).
- Flag: architecture/ADR-00N-*.md files exist on main, against the Unimatrix-only rule; ADR-001.md lacks D-072.

## Edges
- #5914 -> Supports -> #5896
- #5916 -> Contradicts -> #5898: asserted, then removed after the ADR-005 correction (#5924) carried it forward

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- nan-024 ADRs and nan-023 wiring ADRs; prior lessons #4796/#5788/#5790 showed the post-3c CI failure was a recurrence.
- Stored: #5913, #5914 (patterns); #5916 (lesson); corrected #5910, #5911, #5912, #5915, #5918, #5919; ADR-005 corrected #5898 -> #5924 via context_correct. No new ADRs.
