# Gate 3a Report: 1036

> Gate: 3a (Design Review)
> Date: 2026-11-27
> Result: REWORKABLE FAIL

Unimatrix entries consulted: #5768, #2577, #3644, #1204, #5763 (briefing); ADR-INDEX rows #5937 #5960 #5962 #5959 #5955 #5943 #5961 resolved from ARCHITECTURE.md/ADR-INDEX.md summaries.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | WARN | Boundaries, plan order W/R/X, gating, ADR-001..007 followed. Git-exclude leg naming differs from ARCH §3.3/§6 (WARN). |
| Specification coverage | FAIL | C8 `computeMerged` decides "unchanged" by byte-compare; violates FR-16 "a file with no owned change is never rewritten" / D-022 (pseudocode Q6). One-line fix. |
| Risk coverage (R-01..R-25, ACs, S-1..S-6) | PASS | All 25 risks mapped in test-plan/OVERVIEW; High risks have assembled-path tests; AC-1..10, S-1..S-6 map to spawned/assembled tests. |
| Interface consistency | WARN | `before`, `value`, git leg names, and two pseudocode slips (below). |
| Knowledge stewardship | PASS | All design-phase reports carry Stewardship + "Unimatrix entries consulted". Test-plan "nothing novel" has a reason. |

## Detailed Findings

### Architecture alignment — WARN
- C1..C17 match ARCH §2. Order store, W1, W2(+R1), R2-R4, R6, Codex, W5, R10-R12, X1 matches §3.3. Gating (`OK_ACTIONS`, `skipped-dest-unwired`, throw ends run), D-007 `sameRealFile`, D-015 prior opt-in, D-019/D-028 alternates never stepped, D-021 trust.json refusal text (§6a), D-025 enforced modes, D-026 containment (realpath root, nearest-ancestor, dry-run too), §3.8 dir swap, D-029/D-024 parse posture all present.
- Codex W3/W4/R7-R9 run inside one `CX` step (`maybeWireCodex`) rather than separate table rows. Acceptable: order and "no gate" match.
- **Git-exclude naming (judged WARN).** ARCH §3.3 X1 and §6 say harness `git` / surface `exclude`; SPEC FR-40/FR-46/Domain Model, ACCEPTANCE-MAP (AC-5/AC-8), IMPLEMENTATION-BRIEF, pseudocode (C4, C5, C6, C17) and test plans all say `claude-code` / `git-exclude`. §6a does not list it. No logic, gating or AC differs; the effect is the leg's `harness` value and the printed prefix. Spec form is the only one tests and ACs key on; the leg also runs "exactly when the Claude hooks leg runs" (FR-40), so `claude-code` is coherent. Not a behavioral contradiction needing architecture to win. Action: architect amends §3.3, §6 and §6a to `claude-code`/`git-exclude` (no human approval needed). 3b implements the spec form.

### Specification coverage — FAIL
- FR-01..FR-63 have pseudocode homes; NFR-01..07 addressed; no scope additions found.
- **FAIL: FR-16 / D-022 (Q6).** merge-settings.md `computeMerged` returns `text: serialized === raw ? null : serialized`. A `settings.local.json` that already holds the correct owned entries but is not in canonical form (no trailing newline, CRLF, compact) is rewritten (`updated`) on first init. FR-16: "a file with no owned change is never rewritten. Bytes and mtime are unchanged." The other modules decide semantically (claude-json `changed` flag, codex `changed||!present`, opencode `deepEqual(existing, entry)`), so this is the single outlier. The pseudocode justifies it as "hand edits only"; that is a documented departure from an approved FR, not an approved decision.
- Fix: decide change from the merge result (a `changed` flag, or deep-equal of parsed content before/after), return `text: null` when no owned change; serialize only when something changed. Q6's remaining part (writer-level byte-equal short-circuit, FR-26) is fine as is.

### Risk coverage — PASS
- test-plan/OVERVIEW maps R-01..R-25 to plan files and named tests. R-01 scenarios 1-9 named in init.md/install-store.md (mode-only repair, umask matrix, control files). R-02 fixpoint on every mutate (wire-plan, merge-settings, git-exclude). R-03 scenarios incl. escape-and-return (final realpath decides, consistent with FR-22a and pseudocode `checkContained`), swap at `before-rename`, prefix sibling, parent-dir link, dry-run + real. R-07 parity harness; R-09 sentinel run + `assertTmpHome`; R-11/R-12 fault seam through assembled `wire()`; R-14 one real-git fixture per FR-42 row; R-25 real-binary AC-1(c)/AC-2(b) with skip text naming #1053.
- Integration plan correctly states infra-001 is not functionally applicable; smoke as no-regression.
- Minor gap (WARN-level, not blocking): R-08's "CRLF skill byte-compare" is not an explicit test line; the skill path uses Buffer `writeFileSync` so a byte-equal assertion in init.md AC-5 fixtures would cover it.

### Interface consistency — WARN
- Additive `before` on `updateFileSync` (Q1): needed so leg detail derives from the same bytes as the decision; consistent usage across C7-C10, C5, C8. Accept. Pseudocode safe-write.md header still shows the §6 return type `{action,file,target}`; the "add `before: raw` to every return" note must be applied. Architect to record in §6.
- `ConfigDirs.value` (Q3): additive, needed for FR-34/FR-50 `<VAR>=<value>`. Accept. Slip: status.md calls it `_values` and returns `_dirs` in `Info`; implementer must keep both out of `status --json` (§6a fixes the `configDirs` field set) and use one name.
- Q4 (legacy CODEX_HOME text), Q5 (`not-regular` for symlink loop; spec silent, within ARCH code set), Q7 (`writeRootMarker` return shape; tests listed for update): accept.
- Pseudocode slips (fix at 3b, no rework): wire-plan.md uses `legacyOc` before assignment; `Step.run` typed `(dryRun, results)` but called `s.run(dryRun)`.
- Test-plan open items: Q1 status suite file (`launcher-status.test.js` per NFR-07) accept; Q2 resolved above; Q3 R-23 latency has no numeric budget, record against nan-024 ADR-005 at 3c, accept.
- DECISIONS D-001..D-033 respected (D-028 no R5; D-030/D-033 R4 unconditional, no discovery flag; D-016/D-024 vs D-029 postures split correctly).

### Knowledge stewardship — PASS
- Architect: Queried + Stored (7 ADRs). Risk: Queried + Stored-declined with reason. Pseudocode: Queried + deviations. Test plan: Queried + "nothing novel -- plan-stage only". All have consulted lines. Note: pseudocode agent did not `context_get` the ADRs (read from ARCHITECTURE summaries); content matches.

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| FR-16/D-022: byte-compare `serialized === raw` rewrites already-correct non-canonical settings.local.json | uni-pseudocode | In merge-settings.md `computeMerged`: return `text: null` when the merge made no owned change (changed flag / deep-equal on parsed content); update Q6 note and add a test line to test-plan/merge-settings.md: file with correct owned entries but no trailing newline/CRLF/compact -> `unchanged`, bytes and mtime equal |
| Fold in while reopening (non-blocking) | uni-pseudocode | Fix `legacyOc` ordering and `Step.run` signature in wire-plan.md; unify `value`/`_values`/`_dirs` in status.md and state they are excluded from JSON; apply `before` to safe-write.md header |
| Contract sync (non-blocking, no re-gate) | uni-architect | Amend ARCH §3.3/§6/§6a: git leg = `claude-code`/`git-exclude`; `updateFileSync` returns `before`; ConfigDirs per-harness `value` |

## Scope Concerns
None.
