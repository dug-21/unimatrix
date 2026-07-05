# Agent Report: vnc-044-agent-0-scope-risk

**Mode:** scope-risk
**Deliverable:** product/features/vnc-044/SCOPE-RISK-ASSESSMENT.md

## Summary

9 scope-level risks identified (SR-01..SR-09). By severity:
- High: 5 (SR-02, SR-03, SR-04, SR-06, SR-07)
- Med: 3 (SR-01, SR-05, SR-08)
- Low: 1 — with SR-09 raised as a recommendation-level gap, not a table row.

## Top 3 for Architect Attention

1. **SR-09 — lifecycle status ≠ delivery status.** The lean projection carries `EntryRecord.status` (lifecycle) but the #913 motivating use case wants capability *delivery* status. The feature can appear to satisfy #913 while returning `active` for every capability node. Must be stated prominently in the ADR, tool description, and AC-06; delivery-status stays a named follow-up.
2. **SR-03 — suite-wide ADR authored with one adopter.** The ADR binds the whole context-tool suite (axis spelling, 256 constant, summary field set) but only `context_graph` is exercised. Untested contract can drift from later adopters' needs. Single-source the locked values; allow per-tool summary field-set overrides.
3. **SR-06 / SR-07 — shared-type blast radius.** `ResponseFormat`/`parse_format` and `EntryRecord`/`EdgeRecord` are suite-shared and wire-locked. vnc-044's code change must stay graph-local (distinct projection type, no shared-enum behavior change, no `skip_serializing_if` on shared structs). Evidence: patterns #4831 (enum blast radius), #4975 (ADR value drift).

Also flagged: SR-02 (UTF-8 char-boundary truncation correctness — panic risk on naive byte slice) and SR-04 (default full→summary flip is a real behavior change for existing graph callers).

## Knowledge Stewardship
- Queried: /uni-knowledge-search (context_search) for lesson-learned on behavior-change/gate-rejection, patterns on shared-enum/serialization blast radius, and ADR-standard-drift. Findings: #4831 (adding a field to a shared wire enum = ~45-site blast radius + exhaustive-match breakage) directly grounds SR-06/SR-07; #4975 (a locked ADR value drifting downstream when only partially exercised, guard with discriminator test) directly grounds SR-03 and Recommendation 3. Gate-failure lessons (#4473, #4147) confirm behavior-change ACs must be testable at spec phase — reinforces SR-04.
- Stored: nothing novel to store — the two operative patterns (shared-type blast radius #4831; locked-ADR-value drift #4975) already exist and are referenced. The suite-standard-with-one-adopter drift is captured feature-specifically as SR-03 and does not yet have clean 2+-feature generalization beyond the existing #4975 pattern.
