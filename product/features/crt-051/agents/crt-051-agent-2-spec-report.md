# Agent Report: crt-051-agent-2-spec

## Output

SPECIFICATION.md written to:
`product/features/crt-051/specification/SPECIFICATION.md`

## Decisions Made

1. **OQ-1 resolved (pair count)**: Applied human confirmation — raw pair count, not
   unique-entries-in-pairs. Formula: `1.0 - (contradiction_pair_count as f64 / total_active as f64)`.

2. **OQ-2 resolved (cold-start)**: Score 1.0 is correct in both cold-start and clean-DB
   cases. `contradiction_scan_performed` boolean is the operator-visible distinguishing
   signal. No code change needed.

3. **OQ-3 resolved (JSON schema)**: No JSON schema change. Changelog entry confirmed.

4. **SR-02 clarification**: The `coherence: 0.7450` field in `make_coherence_status_report()`
   is independently hardcoded — it is NOT computed from the dimension scores in the fixture.
   Changing `contradiction_density_score` from `0.7000` to `1.0` does not invalidate the
   `text.contains("0.7450")` assertion in `test_coherence_markdown_section`. Only the
   `contradiction_density_score` field in the fixture needs updating.

5. **AC-14 through AC-17 added**: Four new acceptance criteria added beyond SCOPE.md
   AC-01–AC-13, covering SR-01 (test rewrite), SR-02 (fixture update), SR-03 (phase
   ordering comment), and SR-05 (cold-start dedicated unit test).

## Test Sites Discovered

| File | Lines | Change Type |
|------|-------|-------------|
| `infra/coherence.rs:196,201,206` | 3 unit tests | Full rewrite (names + params + assertions) |
| `mcp/response/mod.rs:1422` | 1 fixture field | Value update `0.7000` → `1.0` |
| `services/status.rs:747–748` | 1 call site | Arg swap `total_quarantined` → `contradiction_count` |
| `services/status.rs:544` | Default initialization | No change needed (already `1.0`) |
| `mcp/response/mod.rs:616,710,971,1050,1131,1204,1281` | 7 fixture fields | No change (already `1.0`) |

## AC Gaps Found

None. SCOPE.md AC-01 through AC-13 are all present in SPECIFICATION.md. Four additional
ACs (AC-14 through AC-17) were added from risk assessment findings SR-01, SR-02, SR-03,
SR-05.

## Open Questions for Architect

None. All open questions from SCOPE.md are resolved.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — 14 results returned. Entries #4258 and #4257
  (patterns for scoring function semantic changes and Lambda dimension input audits) applied
  directly to test site enumeration and fixture discovery.
