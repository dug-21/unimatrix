# Agent Report — crt-056-agent-2-spec

**Role:** Specification writer
**Output:** `product/features/crt-056/specification/SPECIFICATION.md`

## Summary
Authored SPECIFICATION.md from SCOPE.md + SCOPE-RISK-ASSESSMENT.md. 18 functional requirements
(FR-1..FR-10 Wave 1 config-parity, FR-11..FR-18 Wave 2 per-slug tick + work-unit seam), 9
non-functional requirements, AC-1..AC-7 turned into verifiable requirements with verification
methods, domain model / ubiquitous language section, and 10 constraints. All scope risks
SR-01..SR-10 mapped into constraints/ACs.

## Key requirement interpretations
- AC-4 specified as the load-bearing N=2 behavioral two-slug test, explicitly doubling as the
  AC-7 concurrency-readiness proof (SR-07, #4974 precedent).
- AC-1 specified as field-by-field equality with the daemon's RESOLVED config, not a subset
  (SR-05).
- Resolved OQ-1 (per-slug ServiceLayer owns one handle set, shared serve/tick), OQ-3 (per-slug
  counters), OQ-5 (adapt_service + session_capabilities both per-slug, in parity scope) in-spec.
- Left OQ-2 (interface altitude) and OQ-4 (constructor shape: required vs Option) open for the
  architect — both forms satisfy AC-6.
- AC-6 specified as additive: test-default constructor preserved.

## Open question for human
- HQ-1: confirm no near-term large-N OSS deployment expectation that would make the serial tick
  fall behind before Step B (assumption A4). Step B stays out of crt-056 regardless.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- general delivery/wave + prior crt/vnc decision
  entries; none override scope. #3756 (two-wave by dependency order) aligns with Wave 1→2;
  #3753 (search.rs reads phase/typed-graph handles) corroborates FR-15 serve/tick handle
  identity. No new knowledge stored (read-only tier).

---

## Correction Pass (design-review, 2 corrections — all refs re-verified against source)

### 1. session_capabilities OUT of parity (FR-10 ↔ ADR-006 alignment)
FR-10 previously recorded `session_capabilities` as per-slug, in-parity — conflicting with ADR-006
("OUT"). Re-verified: it is the `[agents]` capability allowlist (`infra/config.rs:441`), enforced
per-server via `AgentRegistry` (`main.rs:691`, `:1263`) — a per-session negotiated surface, not a
config-driven analytics/retrieval field. Changed FR-10, AC-1, the "config parity" domain entry, and
OQ-5: `session_capabilities` OUT; AC-1 checklist = the 8 ADR-006 config fields; `adapt_service`
per-slug (same config, independent state) kept in. FR-10 and ADR-006 now agree.

### 2. Line attribution corrections (every ref read-confirmed)
| Claim | Old → New | Verified at |
|-------|-----------|-------------|
| Singleton handle extract | (no ref) → main.rs:957-961 | main.rs:957-961 |
| spawn_background_tick call | (no ref) → main.rs:968-991 | main.rs:968-991 |
| Tick ops | background.rs:363-794 → 363 = `run_single_tick` call; ops in `run_single_tick` ~441-803 (maint 463, co-access 552, TypedGraph 566, PhaseFreq 629, contradiction 703, extraction 744, NLI 780, enrichment 794) | background.rs |
| build_project_server call site | main.rs:1084-1091 → main.rs:1085-1091 (loop `for slug` at 1084) | main.rs:1084-1092 |
| build_project_server signature | (added) http_provision.rs:125-131 | http_provision.rs:125-204 |
| Daemon ServiceLayer (AC-1 ref) | main.rs:880-898 (unchanged) | main.rs:880-898 ✓ |
| Test-default ServiceLayer | server.rs:306-333 (unchanged) | server.rs:306-333 ✓ |
