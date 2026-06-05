# Agent Report: vnc-024-agent-3-risk (architecture-risk, update pass)

Updated `RISK-TEST-STRATEGY.md` to reflect two final human design-review decisions
(typed `transcript_delta` contract; OSS rejection of `RetainDays`). Decisions not relitigated.

## Risks changed

| Risk | Change | What moved |
|------|--------|-----------|
| R-01 | **risk-reduced** for delta payload | Added scenario 4: dual-sided (Rust↔TS) `TranscriptDeltaPayload {offset,bytes}` fixture parsing into the typed struct on both sides — closes the formerly-untyped (`serde_json::Value` → `any`) highest-drift field. Remaining R-01 surface = other tagged/flatten variants. |
| R-08 | **risk-reduced** | Delta `offset`/`bytes` was the prime contract-incompleteness vector; now an explicit typed 6th export verified by R-01 sc.4. R-08 description/scenarios re-pointed to the named binding; F2 no longer re-types it. Stays High (other omitted-field surface remains). |
| R-09 | **new coverage added** | OSS `validate()` must REJECT `RetainDays(N)` with the enterprise-only error AND accept `PurgeOnCycleClose` (AC-13/ADR-005). Replaced the old range-check scenario (no range check exists in OSS — rejection is the only path). Framed the footgun: operator believes retention is on; OSS lacks encrypt-at-rest (principle 8 / ass-069 in-memory-only). |
| R-10 | **mostly DISSOLVED** | Severity/likelihood/priority Med→Low. Only live OSS value is unit `"PurgeOnCycleClose"` (bare string, no ambiguity). Dropped coverage for prettifying the rejected tagged `RetainDays` form. Residual: confirm `"PurgeOnCycleClose"` parses + bare-`u32` rejected. |
| R-03 / R-04 | **unchanged** | Accept-and-drop, both transports + batch arm, remain the gate prerequisite. |

## Other sections updated
- **Security Risks**: rewrote the `transcript_retention` bullet (raw secret-bearing ephemeral state; OSS rejects `RetainDays`). Added a **guardrail bullet**: no reusable content secret-scanner exists; the accept-and-drop + in-memory-ephemeral + purge control IS the secrets guarantee; no test/design may assume a redactor licenses persisting raw transcript.
- **Failure Modes**: retention row now "reject loudly, enterprise-only error" (not range clamp); added accept-`PurgeOnCycleClose` row.
- **Edge Cases**: `RetainDays(0)` boundary → any `N` rejected as enterprise-only (no range check).
- **Coverage Summary**: Critical 12 scenarios; High count 6; Medium R-11/R-12; Low R-13/R-10.
- **Header preamble**: retention framing changed from "TOML shape undocumented" to "accepts a value OSS cannot safely honor".

## SR traceability — confirmed holds
- **SR-04** (contract completeness) → now R-08 **+ R-01**; row notes the typed dual-sided delta closes the formerly-`any` field.
- **SR-07** (secrets-to-disk) → R-03/R-04 unchanged; reinforced by the new no-scanner guardrail.
- **SR-02** (None-vs-omission) → R-02 unchanged.
- SR-01, SR-03, SR-05, SR-06, SR-08, SR-09 → unchanged, still accurate.
- No SR row orphaned; no new SR introduced (these were architecture-decision shifts within existing SR scope).

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` not re-run this pass — this is a targeted update to an existing
  strategy whose historical evidence (#4711, #3557, #1266, #4070, #4311, #3905) was already mined
  in the original authoring pass and remains valid.
- Stored: nothing novel to store — both changes are feature-specific contract/validation decisions.
  The one candidate cross-feature pattern ("an enterprise-seam enum variant the OSS build must
  *reject*, not accept-and-ignore, when it cannot honor the variant's safety contract") is visible
  in only this one feature so far; if a 2nd feature repeats it, store via `/uni-store-pattern` then.
