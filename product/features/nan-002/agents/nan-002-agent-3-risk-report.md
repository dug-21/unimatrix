# Agent Report: nan-002-agent-3-risk (Architecture Risk)

## Task
Produce RISK-TEST-STRATEGY.md for nan-002 (Knowledge Import) in architecture-risk mode.

## Outcome
Completed. RISK-TEST-STRATEGY.md written to `product/features/nan-002/RISK-TEST-STRATEGY.md`.

## Risk Summary

- **Critical**: 2 risks (R-01 SQL/DDL divergence, R-02 deserialization edge cases) -- 9 test scenarios
- **High**: 3 risks (R-03 counter collision, R-04 destructive --force, R-05 embedding after commit) -- 10 test scenarios
- **Medium**: 7 risks -- 14 test scenarios
- **Low**: 3 risks -- 3 test scenarios
- **Total**: 15 risks, 36 test scenarios

## Top Risks for Human Attention

1. **R-01 (Critical)**: Direct SQL INSERT statements must mirror the schema DDL exactly. Any column mismatch causes silent data loss. The shared format.rs types (ADR-001) help at compile time, but the SQL INSERT strings are hand-written and must be verified against PRAGMA table_info.

2. **R-02 (Critical)**: 8 table types with nullable fields, JSON-in-TEXT columns, and unicode content create a large deserialization surface area. Historical pattern #371 confirms deserialization-heavy code needs explicit edge-case coverage.

3. **R-05 (High)**: ADR-004's decision to embed after DB commit creates a two-phase success/failure mode. If embedding fails, the database is valid but search is broken. The tester must verify that error messaging clearly distinguishes which phase succeeded.

## Scope Risk Traceability
All 9 SR-XX risks traced. SR-01, SR-02, SR-08, SR-09 map to architecture risks. SR-04, SR-07 addressed by ADR decisions. SR-03, SR-05, SR-06 resolved at specification level.

## Knowledge Stewardship
- Queried: /knowledge-search for "lesson-learned failures gate rejection" -- found #1105 (nan-001 outcome), #141 (glass box validation convention), no directly relevant failures
- Queried: /knowledge-search for "risk pattern import export serialization" -- found #371 (migration compatibility pattern), #343 (JSON-Lines format pattern), #344 (Store::open + Raw SQL pattern) -- all informed R-01 and R-02
- Queried: /knowledge-search for "SQLite migration direct SQL" -- found #336 (ADR-004 import raw SQL), #374 (in-place migration procedure) -- confirmed direct SQL risk patterns
- Queried: /knowledge-search for "outcome rework nan" -- found #1101 (nan-001 pass), #1116 (nan-003 pass) -- no rework evidence
- Stored: nothing novel to store -- risks are feature-specific; the "direct SQL divergence from DDL" pattern is already captured in #344
