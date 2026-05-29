# Agent Report: vnc-022-agent-3-risk (Architecture-Risk)

## Artifact Produced

`product/features/vnc-022/RISK-TEST-STRATEGY.md`

## Risk Summary

- **High priority**: 5 risks (R-01, R-02, R-03, R-06, R-10)
- **Medium priority**: 5 risks (R-04, R-05, R-08, R-11, R-13)
- **Low priority**: 4 risks (R-07, R-09, R-12, R-14)
- **Total scenarios**: 38

## Top Risks for Delivery Attention

1. **R-02 (HIGH)**: dispatch_request capability refactor regresses UDS path — 9 capability check sites must all be converted. AC-18 (UDS regression) is the primary gate. Grep audit for stale `uds_has_capability` calls is essential.

2. **R-06 (HIGH)**: ResolvedIdentity missing SessionWrite — if StaticTokenValidator doesn't add SessionWrite, every session-mutating operation silently fails with 400 Error. The symptom looks like "endpoint works but nothing happens." Integration tests must verify actual pipeline side-effects (session registered, observation persisted), not just HTTP status codes.

3. **R-03 (HIGH)**: Session ID prefix application — the "http-" prefix must be applied BEFORE sanitize_session_id runs. Per lesson #3902, unsanitized session_id propagates into AuditContext. The prefix must use only allowed characters (alphanumeric + hyphen + underscore). ADR-003 caught the colon issue ("http:" revised to "http-") but implementation must match.

4. **R-01 (HIGH)**: ObserveContext field drift — two `Arc<Store>` fields (store and entry_store) have identical types. A positional swap compiles but silently corrupts the pipeline. ADR-001's struct approach mitigates positional risk but the construction in main.rs must map the right server field to the right struct field.

5. **R-10 (HIGH)**: warn+continue failure paths — per lesson #4473, these are the tests most likely to be omitted because the feature works whether or not failure-path tests exist. The tester must explicitly verify each warn+continue arm has an Err-path test.

## Open Questions

- R-05: Does the /observe handler reuse the exact same `Limited` body collection code from McpAdapter, or is it a parallel implementation? Parallel implementations risk divergent size limits.
- R-13: Audit trail for /observe — is AuditSource::Http a new variant or reuse of an existing pattern? The architecture mentions "new variant or reuse existing pattern" — this needs resolution before implementation.

## Knowledge Stewardship

- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" -- found #4473 (warn+continue masks missing failure-path tests), #4177 (tautological assertions), #2758 (gate 3c grep for test names). Applied #4473 directly to R-10.
- Queried: /uni-knowledge-search for "session registry session_id collision concurrent" -- found #4354 (session ID mismatch trap with concurrent clients), #3902 (sanitize_session_id audit). Applied both to R-03 and R-08.
- Queried: /uni-knowledge-search for "dispatch_request tower handler" -- found #763 (observation intercept pattern), #4691 (dispatch_request transport-agnostic). Applied to integration risk analysis.
- Queried: /uni-knowledge-search for "risk pattern" -- found #3426 (regression risk underestimation). No directly applicable pattern for vnc-022.
- Stored: nothing novel to store -- all risks identified are feature-specific, not cross-feature patterns. The warn+continue testing gap (#4473) is already stored.
