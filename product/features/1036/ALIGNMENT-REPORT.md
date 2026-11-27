# Alignment Report: 1036

> Reviewed: 2026-11-27 (round 3, after D-030 and the W-1..W-5 fixes)
> Artifacts reviewed:
>   - product/features/1036/architecture/ARCHITECTURE.md
>   - product/features/1036/specification/SPECIFICATION.md
>   - product/features/1036/RISK-TEST-STRATEGY.md
> Also read: SCOPE.md, SCOPE-RISK-ASSESSMENT.md, DECISIONS.md (D-001..D-030), architecture/ADR-INDEX.md; IMPLEMENTATION-BRIEF.md and ACCEPTANCE-MAP.md spot-checked for D-030 drift
> Vision source: product/PRODUCT-VISION.md; goals #5957 (personal-cloud), #5681 (integrity)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Closes silent-dark gaps G1–G3 (`personal-cloud`). D-025 credential and trust-input modes and D-026 project-root containment serve `integrity`. Node-only adapter (Principle 6) |
| Milestone Fit | PASS | Hardening only; no destination moves; #998, #1034, #1053 and F4 stay deferred |
| Scope Gaps | PASS | In-1..In-6 and the amended AC-1..AC-10 are all traced. AC-2's criterion text now carries D-027 (Spec L343) |
| Scope Additions | WARN | Carried from earlier rounds: FR-50/51 resolved lines and same-env note, FR-20a hook-time `trust.json` skip, NFR-04 benchmark. Low cost, traced to SRs |
| Architecture Consistency | PASS | D-025..D-030 are reflected consistently in all three source docs. No open questions remain (Arch L290–301; Spec L409–416; RTS L294–298) |
| Risk Completeness | PASS | Coverage complete; OI-1 is closed by D-030 (RTS L298); labels aligned |

Counts: PASS 5, WARN 1, VARIANCE 0, FAIL 0. V-1 and V-2 were closed in round 2; V-3 is now closed by D-030.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | none | In-1 → ADR-001 #5937. In-2 → FR-01..07, ADR-002 #5960. In-3 → FR-10..17, ADR-004 #5959. In-4 → FR-20..33, ADR-003 #5962 / ADR-005 #5955. In-5 → FR-35. In-6 → FR-40..46, ADR-006 #5943 |
| Addition | Init `resolved:` lines and same-env note (FR-50, FR-51) | From SR-13; supports R3 |
| Addition | Hook-time `trust.json` symlink skip (FR-20a) | Touches hook-time behaviour that D-020 excludes; safe-direction. Awareness only |
| Addition | NFR-04 benchmark (report-only) | Harmless |
| Simplification | AC-1(c) / AC-2(b) real-binary checks skip when the binary is absent | D-012, D-027, #1053 |

## Closure Check

| Item | Evidence | Result |
|---|---|---|
| V-3 RT-03/R4 | DECISIONS D-030 (keep R4; In-3 as written). Arch L78 and L299 (`legacy` keeps `~/.claude/settings.json`); Spec L76 (`resolved: kept (D-030)`) and L416; RTS L16, L32, L298 | Closed. No scope amendment: In-3 is unchanged |
| W-1 stale spec OQ text | Spec L409–416: credential mode attributed to D-025; R5 reference gone | Closed |
| W-2 Twin / Default location definitions | Spec L22–23: twins limited to RT-03/RT-10; `~/.claude.json` is AP-3 under an override. Workflow 2 L364 aligned | Closed |
| W-3 AC-2 criterion text | Spec L343: "(amended, D-027) ... Real `opencode debug config --standalone` ... shows the entry" | Closed |
| W-4 spec inconsistencies | Spec L3 input line D-001..D-030; FR-32 L224 includes `child_process`; FR-31 L220 includes `outside-project` | Closed |
| W-5 RTS labels and wording | RTS L20, L188–189, L204, L275 use AC-2(b); Behavioral-Outcome row 1 (L202) uses `opencode debug config --standalone` | Closed |

## Variances Requiring Approval

None.

## Remaining Drift (outside the three source docs; for the synthesizer and the primary)

These files predate D-030. They do not affect source-doc alignment, but the delivery inputs must not carry them:
1. `IMPLEMENTATION-BRIEF.md` L11 cites "D-001..D-029". L250 still presents the "Apply the D-028 analogy" option as open and names D-030 as a hypothetical amendment. Update it to state D-030 (R4 kept).
2. `ACCEPTANCE-MAP.md` L29 (S-6) is `PENDING` and reads "Changes only if the human amends In-3 (D-030)". D-030 is now the keep decision, so this wording is wrong. Reword it to cite D-030 as the decision that confirms R4.
3. `SCOPE.md` L3 cites "D-001..D-029". This is cosmetic; D-030 does not change scope.

Awareness: D-030 is recorded as decided by `uni-zero-design`, not `human`. It keeps In-3 as written and amends no AC, so it falls within the primary's authority. The round-2 brief had framed this as a human question. If the human wanted to make this call personally, the primary should surface D-030 to them.

## Detailed Findings

### Vision Alignment
- `personal-cloud` (#5957): one-command install; `status` names a lost local-scope entry (FR-35) and alternate profiles (FR-38). Real-binary proof covers Claude (AC-1(c)) and OpenCode (AC-2(b)).
- `integrity` (#5681): enforced modes on `remote.json` and the trust inputs (Spec FR-24; Arch §3.2); repo symlinks contained to the project root (Spec FR-22a; RTS R-03).
- D-030 removes the duplicate owned-hook source, consistent with lesson #5803 (hook scopes merge additively) and AC-6. RTS L32 correctly notes that a false "different" result on R4 is harmless, because the project file carries the hooks.

### Milestone Fit
No destination moves (D-001, D-003); `mcp-stdio` and the launcher are unchanged (D-002). The spec's NOT-in-Scope section matches the SCOPE Non-Goals.

### Architecture Review
Every section reflects D-025..D-030 (§3.1 `legacy`/`alternates`, §3.2 modes and containment, §3.3 R4 kept and R5 gone, §3.4 D-029, §6a, §8, §9). The ADR index carries the superseding versions.

### Specification Review
FR-10 RT-03..RT-10, AP-1..AP-3, FR-17, AC-1..AC-6 and the Open Questions section are consistent with D-019/D-028/D-030. No internal contradictions remain.

### Risk Strategy Review
The register is unchanged in shape (8 High, 11 Med, 6 Low). R-05 is re-described for the R4 impact under D-030. OI-1 is closed. AC-2(b) labelling and the mechanism wording are aligned with the spec and architecture.

## Knowledge Stewardship
- Queried: `context_search` (vision alignment patterns). #3611/#3612 (a correction applied to one doc leaves stale references in the others) apply to the remaining drift in BRIEF/ACCEPTANCE-MAP. #5803 supports D-030.
- Stored: nothing novel. Already covered by #3611/#3612.
