# Scope Risk Assessment: nan-005

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | README accuracy at write-time: 12 tools, 14 skills, schema v9+, 8 crates -- any count wrong at authoring time ships incorrect docs immediately | High | High | Author must verify every fact against live codebase, not SCOPE.md claims. Build a checklist of verifiable facts (tool count, skill count, crate count, schema version, test count). |
| SR-02 | Documentation agent effectiveness depends on SCOPE.md/SPECIFICATION.md quality -- if feature artifacts are thin or missing, the agent produces nothing useful | Med | Med | Agent definition should include fallback: read git diff or CHANGELOG when feature artifacts are incomplete. |
| SR-03 | README single-file constraint may exceed practical size for 11 sections covering 12 tools + 14 skills + categories + CLI + architecture | Med | Med | Architect should estimate line count. If >500 lines, consider whether a single file degrades navigability. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Scope overlaps with nan-003 (onboarding skills) -- "operational guidance" in README vs `/unimatrix-init` CLAUDE.md block could diverge or duplicate | Med | High | Define explicit boundary: README documents concepts, nan-003 skills install per-repo config. Cross-reference, don't duplicate. |
| SR-05 | "Documentation agent is optional" (AC-08) means the decay-prevention mechanism has no enforcement -- Delivery Leaders may always skip it, defeating the purpose | Med | High | Spec writer should define clear trigger criteria (e.g., "if feature adds/changes MCP tool or skill, doc step is mandatory"). Pure optionality risks silent decay. |
| SR-06 | AC-12 ("factually accurate against current codebase") is unverifiable by review alone -- no automated check exists to validate README claims against code | Low | High | Accept this as a known limitation. The documentation agent mitigates going forward but the initial snapshot is manual. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Protocol modification (AC-07) adds a step to `uni-delivery-protocol.md` -- if placement is wrong relative to PR creation and `/review-pr`, doc updates may not be included in the reviewed PR | Med | Med | Spec must define exact insertion point in Phase 4 sequence: after implementation commit, before `/review-pr` invocation. |
| SR-08 | Documentation agent spawned in delivery protocol reads README.md -- concurrent feature branches modifying README create merge conflicts | Med | Low | Accept: low likelihood with current team size. If it occurs, conflicts are in markdown (easy to resolve). |

## Assumptions

- **SCOPE.md "Resolved Questions" #1**: Documentation agent runs before `/review-pr`. If review-pr is refactored or reordered in future protocol changes, the doc step may be orphaned. (Ref: Resolved Questions section)
- **SCOPE.md Constraint 2**: "Documentation agent reads artifacts, not code." This assumes feature artifacts always exist and are complete. Features delivered outside the protocol (hotfixes, infrastructure debt) will have no artifacts for the agent to read. (Ref: Constraints section)
- **SCOPE.md Constraint 5**: "README accuracy depends on manual discipline until the documentation agent is operational." The initial README is a point-in-time snapshot. Any features shipped between nan-005 authoring and nan-005 delivery will create immediate drift. (Ref: Constraints section)

## Design Recommendations

- **SR-01**: The spec should include an explicit "fact verification checklist" as a gate criterion -- every numeric claim in README must cite its source (query, file, or command).
- **SR-05**: Consider making the documentation step mandatory when the feature's SCOPE.md contains any of: new MCP tool, new skill, new CLI subcommand, schema change. Optional only for pure-internal features.
- **SR-07**: The spec should specify the exact protocol line after which the documentation agent spawns, with a concrete example of the Phase 4 sequence.
- **SR-04**: The README operational guidance section should explicitly link to `/unimatrix-init` and `/unimatrix-seed` rather than restating their content.
