# Test Plan: extraction-rules (Wave 2)

## Unit Tests: Extraction Rules

### T-ER-01: ExtractionRule trait compliance (AC-01)
- Define a custom test rule implementing ExtractionRule
- Call evaluate() with empty observations
- Expected: Compiles and returns empty Vec

### T-ER-02: KnowledgeGapRule produces gap entries (AC-02)
- Input: Synthetic observations with context_search PostToolUse in 2 sessions,
  both with response_size=0, same query "deployment rollback"
- Expected: One ProposedEntry with category="gap", title contains "deployment rollback"

### T-ER-03: KnowledgeGapRule single feature produces nothing (AC-02b)
- Input: Same zero-result search but in only 1 session
- Expected: Empty Vec

### T-ER-04: ImplicitConventionRule 100% consistency (AC-03)
- Input: 3 sessions, all accessing "product/features/" via Read
- Expected: One ProposedEntry with category="convention"

### T-ER-05: ImplicitConventionRule partial consistency (AC-03b)
- Input: 5 sessions, pattern present in only 4
- Expected: Empty Vec (not 100%)

### T-ER-06: ImplicitConventionRule min features (AC-03)
- Input: 2 sessions with same pattern (below min 3)
- Expected: Empty Vec

### T-ER-07: DeadKnowledgeRule access cliff (AC-04)
- Input: 8 sessions, store with entry accessed in sessions 1-3 but absent in 4-8
- Expected: ProposedEntry with category="lesson-learned", tags include "dead-knowledge"
- Note: Requires Store (test-support feature)

### T-ER-08: DeadKnowledgeRule still accessed (AC-04b)
- Input: Entry accessed in most recent session
- Expected: Empty Vec

### T-ER-09: RecurringFrictionRule 3+ features (AC-05)
- Input: Observations triggering same detection rule in 3 sessions
- Expected: ProposedEntry with category="lesson-learned"

### T-ER-10: RecurringFrictionRule 2 features (AC-05)
- Input: Same detection rule fires in only 2 sessions
- Expected: Empty Vec

### T-ER-11: FileDependencyRule chains (AC-06)
- Input: Read(A)->Edit(B) within 60s in 3 sessions
- Expected: ProposedEntry with category="pattern"

### T-ER-12: FileDependencyRule no pattern (AC-06)
- Input: Read and Write of same file (not A->B)
- Expected: Empty Vec

### T-ER-13: default_extraction_rules returns 5 rules
- Expected: Vec with len() == 5

## Unit Tests: Quality Gate

### T-QG-01: Rate limit rejects after 10/hour (AC-09)
- Setup: ExtractionContext with rate_count = 10
- Input: Valid ProposedEntry
- Expected: Reject { check_name: "rate_limit" }

### T-QG-02: Rate limit resets on hour boundary (R-07)
- Setup: Context at hour N with count=10, advance to hour N+1
- Input: Valid ProposedEntry
- Expected: Accept (counter reset)

### T-QG-03: Content validation rejects short title (AC-10b)
- Input: ProposedEntry with title.len() < 10
- Expected: Reject { check_name: "content_validation" }

### T-QG-04: Content validation rejects short content
- Input: ProposedEntry with content.len() < 20
- Expected: Reject { check_name: "content_validation" }

### T-QG-05: Cross-feature validation per rule minimum (AC-10)
- Input: ProposedEntry with source_rule="implicit-convention", source_features.len()=2
- Expected: Reject { check_name: "cross_feature" } (needs 3)

### T-QG-06: Cross-feature validation passes at minimum
- Input: Same but source_features.len()=3
- Expected: Accept

### T-QG-07: Confidence floor rejects < 0.2 (AC-11)
- Input: ProposedEntry with extraction_confidence=0.15
- Expected: Reject { check_name: "confidence_floor" }

### T-QG-08: All existing tests pass after Wave 2 (AC-20c)
- Command: cargo test --workspace
- Expected: All tests pass

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-01 (low-quality entries) | T-QG-01 through T-QG-07 |
| R-07 (rate limit reset) | T-QG-01, T-QG-02 |
