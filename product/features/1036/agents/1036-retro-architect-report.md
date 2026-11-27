# 1036-retro-architect report (retrospective)

Unimatrix entries consulted: #5968, #5911, #5832, #5128, #5915, #5919, #5957, #5963, #5937, #5960, #5962, #5959, #5955, #5961, #5784, #5788

## 0. Stewardship review
- #5968 (SM, mid-delivery) corrected → #5970. Narrowed to the root cause of the gate miss and added the clean-checkout class and links to prior recurrences. Its original line "platform-gated behaviour must be tested via --bundle rather than skipped" was broader than D-042, which allows skips for local-only behaviour. That rule moved to its own lesson, #5971.
- #5919 (procedure) step 3 told authors to give skips a D-032 reason, which D-042 made obsolete. Corrected → #5973.
- #5911 (pattern) was accurate but incomplete: a tmp HOME is not sufficient. Corrected → #5972.

## 1. Patterns
- New #5974: install-surface writers use a pure `compute(raw) -> {text|null, detail}` through `safe-write.updateFileSync`, and "changed" is decided on parsed content, not bytes. Source: the Gate 3a failure. This is also the template for adding any new harness surface.
- Updated #5911 → #5972: home+env isolation. Ambient override vars, central clearing in home-guard, scrubbedEnv, USERPROFILE, stagedCli.
- Skipped: wire-plan table (ADR-004 covers it) and config-dir resolver (ADR-002 covers it). Neither needs a separate pattern.

## 2. Procedures
- Updated #5919 → #5973 (JS/TS CI contract):
  - Step 3 follows D-042: classify by behaviour, use the `initBundle`/`useStub`/`offlineBundle` helpers, cite #5957 in skip reasons.
  - Step 5 covers central env clearing.
  - New step 7: before 3c, run with an adversarial env and in a clean worktree using `--omit=optional`.
  - Size headroom refreshed: 16 B stripped. D-034 cap added.
  - New: never commit node_modules.

## 3. ADR status
| ADR | ID | Status |
|---|---|---|
| ADR-001 placement | 5937 | Validated |
| ADR-002 config-dir resolver | 5960 | Validated. The CI env leak was in test fixtures, not the resolver. |
| ADR-003 hardened writer | 5962 | Validated. Security review probed 5 symlink escapes, all refused. F4 was a consumer fallback (`\|\| "updated"`) that violated its fail-loud rule; fixed in-PR. |
| ADR-004 wire plan | 5959 | Validated. W3/W4/R7-R9 collapsed into one CX step, which the pseudocode specified and Gate 3b accepted. No ADR change. |
| ADR-005 writer core in trust.js | 5955 | Validated |
| ADR-006 git-exclude | 5963 | **Flagged for correction when #1057 lands.** The ADR does not cover security F1: bare `git` resolves from cwd on Windows. It also does not cover F2: the exclude-file write is not contained (`containWithin` git-common-dir; `-c core.fsmonitor=false`). Not superseded now; the decision is pending in #1057. |
| ADR-007 status reporting | 5961 | Validated |

## 4. Lessons
- #5970 (corrects #5968): gates ran only in the devcontainer, so they never exercised the ambient env, a clean checkout or non-Linux. This is the fourth recurrence (#5128, #5832, #5915). #5915 already prescribed a draft-PR ci.yml run before 3c. It was not applied, and none of the 3c validator's consulted lists included it.
- #5971 (new): classify platform skips by the behaviour each test proves, not by its entry point. Route shared behaviour through `init --bundle` (D-042). macOS CI on ee380e5c validated this.
- Real-HOME fixture lesson: folded into #5972 rather than stored again, because #5911 already owned it.
- node_modules commit: no lesson stored. The accident was a one-off; its prevention is now step 10 in #5973.

## 5. Retrospective findings (human action, not stored)
1. **The knowledge-only remedy has failed four times.** "Run ci.yml on HEAD before Gate 3c" (#5915) and "devcontainer green is not CI green" (#5128, #5832) are stored and tagged `recurring`. They were still not served to, or applied by, the 1036 3c validator. Recommend encoding them in `.claude/protocols/uni/uni-delivery-protocol.md` Stage 3c and the uni-validator 3c check set:
   - a draft PR with the ci.yml run ID on HEAD
   - an adversarial-env run
   - a clean-worktree run with `--omit=optional`
2. **`.gitignore` has no `node_modules` rule.** Local ignore comes only from `.git/info/exclude`, which is not committed. Gate 3c asked for a rule; none landed. A one-line chore.
3. **Windows package cells are not in ci.yml.** The win32 branches of agent-27's rewritten tests (rt07, ac10, and the winSymlink/winModes skips) are unverified.
4. **Hook-client size gate has 16 B stripped headroom.** The next hook-client change will need a cap decision; budget it in design.
5. **coordinator_respawns 86 (6.2σ).** The timeline spans the whole cycle, starting at +0m, but is attributed to pr-review because that was the last phase. Two sessions ran 13 h, including a 6.6 h API-limit stall (STALLS.md) and an 8 h pr-review with human-directed fixes. The metric most likely counts SendMessage resumes (SubagentStart for uni-scrum-master) as respawns. The metric owner should confirm and split resume from respawn; otherwise the baseline is not comparable.
6. **bash_for_search 578 (1.6σ)** is contrary to the CLAUDE.md rule to use Grep/Glob. Agents read code with chained `sed -n` / `cat` over Bash. Report only.
7. **sleep_workarounds 4 (info).** At baseline (mean 4.2); #3561 already covers it. No action.
8. **uni-security-reviewer context_store failure.** It was refused twice with "lacks Write capability". D-041 attributes this to an omitted `agent_id`; the briefing says it failed even with `agent_id` set. Unresolved. The human decides; the Windows bare-git lesson is tracked as #1057.
9. **Transcript candidates** are Reconstructed provenance with `search_complete: false`, mostly tool-event noise. They were given low weight and yielded nothing beyond the artifacts.

## 6. Relationship edges
None. The bar was not met:
- #5974 cites ADR-005 in prose, which is enough to find it.
- #5970/#5971 → #5915/#5128 are recurrence links. They are lessons linking to lessons, which is not one of the three allowed types.
- No decision would be wrong without traversing these links.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing. It returned nan-023 ADRs, #5958 and #5769. The related lessons were found separately: #5911, #5832, #5128, #5915 and #5919.
- Stored:
  - #5970: lesson, corrects #5968
  - #5971: lesson, new
  - #5972: pattern, corrects #5911
  - #5973: procedure, corrects #5919
  - #5974: pattern, new
  - No ADRs created or changed. ADR-006 (#5963) is flagged pending #1057.
