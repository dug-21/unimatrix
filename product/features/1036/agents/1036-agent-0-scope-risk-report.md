# 1036-agent-0-scope-risk report

Output: product/features/1036/SCOPE-RISK-ASSESSMENT.md (17 risks: 12 rated, High 6 and Med 6; 5 path-divergence risks, unrated by template).
Top 3: SR-13 (init env differs from harness env, then twin removal leaves the harness dark), SR-09 (status/mcpNode private path fallbacks), SR-06 (check-ignore on a tracked file means the exclude file is appended to on every run).

Unimatrix entries consulted: #5803, #5837, #5911, #4473, #5896, #5763

## Knowledge Stewardship
- Queried: context_briefing, context_search for lesson-learned/gate rejection and risk patterns. Relevant: #5911 (test home/env isolation), #5837 (own every surface), #4473 (warn+continue masks failure-path tests).
- Stored: nothing novel to store. The env-scrub concern extends #5911 and is specific to this feature until it recurs.
