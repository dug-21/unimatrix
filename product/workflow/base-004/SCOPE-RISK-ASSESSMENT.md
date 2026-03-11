# Scope Risk Assessment: base-004

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Context window bloat from stewardship sections added to 12+ agent definitions; agents already near token limits may lose task-relevant context | High | High | Architect should measure token cost of stewardship additions per agent and set a hard ceiling (e.g., 15 lines max). Concise guidance in agent def, detailed structure in skill. |
| SR-02 | Validator stewardship checks depend on unstructured agent report text ("stored entry IDs or nothing novel"); brittle parsing of free-form prose | Med | High | Define a machine-parseable stewardship block format in agent reports (e.g., `## Stewardship` section with structured fields) rather than relying on grep heuristics. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Scope says "no CLAUDE.md changes" but CLAUDE.md already references stewardship skills; new `/store-pattern` skill may need CLAUDE.md mention to be discoverable | Low | Med | Confirm whether CLAUDE.md skill list is auto-generated or manually curated. If manual, this constraint may need relaxation. |
| SR-04 | Boundary between `/store-pattern` and `/store-lesson` is ambiguous for bug-investigator: root cause patterns could fit either category, leading to inconsistent storage | Med | Med | Spec should define a clear decision rule (e.g., "if triggered by a failure, use /store-lesson; if generalizable regardless of failure, use /store-pattern"). |
| SR-05 | "No automated storage" non-goal conflicts with existing auto-extraction pipeline (col-013 rule-based extraction stores entries with trust_source: "auto"); agents may duplicate auto-extracted entries | Low | Med | Spec should clarify that agent-stored entries coexist with auto-extracted entries and note dedup expectations (or lack thereof). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Retro quality pass queries entries by feature_cycle tag, but agents may not consistently tag stored entries with the correct feature_cycle value | High | Med | Architect should verify that the `/store-pattern` skill auto-injects feature_cycle from session context, not relying on agents to provide it manually. |
| SR-07 | Validator gate checks for stewardship add a new failure mode to every delivery session; if checks are too strict early on, adoption friction stalls delivery | Med | Med | Consider a "warn-only" rollout period before REWORKABLE FAIL enforcement, or make the first iteration advisory. |
| SR-08 | 12 agent definitions modified simultaneously; mid-session agents on active features will see inconsistent stewardship expectations if changes deploy incrementally | Med | Low | Recommend atomic deployment (single commit with all agent def changes) and note backward-compat constraint in spec. |

## Assumptions

1. **Agent report structure is stable** (Constraint 6): Assumes agent reports have a consistent format where stewardship evidence can be located. If report formats vary significantly across agents, validator checks will be unreliable. (SCOPE.md Constraint 6)
2. **Existing store tools accept pattern content** (Non-Goal 3): Assumes `context_store` with category "pattern" and structured what/why/scope content works without tool signature changes. If content validation is server-side, the skill alone cannot enforce the template. (SCOPE.md Non-Goal 3)
3. **Feature_cycle tagging works reliably** (Retro quality pass, Layer 3): The retro quality pass depends on entries being tagged with the correct feature_cycle. If this tagging is inconsistent today, the quality pass will miss entries. (SCOPE.md Proposed Approach, Layer 3)

## Design Recommendations

1. **SR-01**: Measure token cost before and after stewardship additions for the 3 largest agent definitions (validator, architect, rust-dev). Set a per-agent stewardship budget.
2. **SR-02, SR-06**: Define a structured stewardship report format that both agents and the validator can rely on. Include feature_cycle and entry IDs as required fields.
3. **SR-07**: Specify a rollout strategy -- advisory logging before hard enforcement -- to avoid blocking active delivery while agents adapt.
