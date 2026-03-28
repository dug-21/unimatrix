# Security Review Report: bugfix-437

**PR**: #438
**Branch**: bugfix/437-recurring-friction-wrong-write-path
**Reviewer**: bugfix-437-security-reviewer
**Risk Level**: LOW

## Summary

The diff removes a store-writing extraction rule and replaces it with pure computation. The change is a net reduction in attack surface. No new trust boundaries, input validation paths, privilege changes, or deserialization patterns are introduced.

## OWASP Assessment

### Injection (OWASP A03)
No new SQL, shell, or format string interpolation paths introduced. `compute_friction_recommendations()` and `compute_dead_knowledge_recommendations()` produce format strings from rule names and integer counts — both are from internal detection rule output, not user-supplied input. No injection risk.

### Access Control (OWASP A01)
`friction_signals` and `dead_knowledge_signals` are appended to `maintenance_recommendations` inside the existing `context_status` handler, which already requires `Capability::Read`. No new capability boundary introduced. The unconditional append is architecturally correct — both functions produce read-only observational strings.

### Deserialization (OWASP A08)
No new deserialization paths. `Vec<String>` additions to `TickMetadata` are in-memory only and not persisted or deserialized from external input.

### Input Validation (OWASP A03/A04)
`compute_friction_recommendations()` applies the ADR-005 `source_domain == "claude-code"` filter as the first operation — mandatory domain guard preserved. `compute_dead_knowledge_recommendations()` delegates to `detect_dead_knowledge_candidates()` which also applies the same domain filter.

### Information Disclosure (OWASP A02)
The `maintenance_recommendations` field in `context_status` already exposes operational metadata. The new signals add entry counts and rule names — no user PII, no secrets, no internal IDs exposed. Rule names and session counts are already visible via detection telemetry.

## Blast Radius

Worst case if `compute_friction_recommendations()` has a subtle bug: returns incorrect strings in `maintenance_recommendations`. No data loss, no ENTRIES corruption, no auth bypass. The old code's worst case was permanent ENTRIES pollution — this is strictly better.

`compute_dead_knowledge_recommendations()` returns `Vec::new()` on store error (inherited from `detect_dead_knowledge_candidates()`'s `Some(vec![])` error path) — safe default.

## Regression Risk

LOW. `RecurringFrictionRule` is removed from `default_extraction_rules()`, reducing the extraction pipeline from 4 to 3 rules. Existing entries previously written by this rule remain in ENTRIES (cleanup deferred to follow-up). No other extraction rules are affected. The `extraction_tick` return type change is internal to the background module — no public API changes.

## Dependency Safety

No new dependencies introduced.

## Secrets

No hardcoded credentials, tokens, or keys in the diff. No `.env` access patterns added.

## Findings

| ID | Severity | Description | Status |
|----|----------|-------------|--------|
| — | — | No findings | — |

## Verdict

**APPROVED — no blocking findings.**

Risk level: LOW. The change is a net security improvement: removes a side-effecting extraction rule that wrote to ENTRIES unconditionally, replacing it with pure read-only computation. No new attack surface introduced.
