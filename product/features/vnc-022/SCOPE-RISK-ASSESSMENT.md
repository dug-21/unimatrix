# Scope Risk Assessment: vnc-022

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `dispatch_request` has 10 Arc-wrapped params — adding a capabilities parameter creates a wide integration seam where HTTP and UDS callers must stay in sync on parameter evolution | High | Med | Architect should define a context struct rather than growing the parameter list; test both call sites against the same contract |
| SR-02 | PreCompact Day 1 degradation (briefing-only, no transcript) may silently produce lower-quality results for remote users with no visible indicator of degradation | Med | High | Spec should define explicit response field or header signaling degraded mode so clients can surface it |
| SR-03 | Client-generated `session_id` is trusted without server-side validation beyond format — a malicious or buggy client could hijack or pollute another session's state in `SessionRegistry` | High | Low | Architect should assess whether session_id needs to be scoped to bearer token identity, not just format-validated |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Cutting `hook-remote` CLI means clients must implement HTTP POST + JSON serialization + stdout writing independently — contract drift across 3 client patterns (Claude Code http hooks, Codex curl, Gemini curl) | Med | Med | Spec must define the wire contract precisely with examples; consider a contract test fixture clients can validate against |
| SR-05 | "Nice-to-have" event tier (3 events) deferred — boundary is implicit; if any critical/important event implicitly depends on a nice-to-have event for correctness, the pipeline has a silent gap | Med | Low | Architecture should explicitly map event dependencies to confirm no critical path crosses the tier boundary |
| SR-06 | No offline buffering for remote means transient network failures silently drop observation events — fire-and-forget events (9 of 13) have no retry or acknowledgment | Med | Med | Document the data-loss window clearly; architect should confirm fire-and-forget semantics are acceptable for all 9 events |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | `PathRouter` currently cannot reach `UnimatrixServer` service handles — the server is wrapped inside rmcp's `StreamableHttpService` and not directly accessible. Threading service handles through requires careful layering without breaking rmcp encapsulation | High | Med | Architect must design the handle-passing pattern before pseudocode; this is the primary structural change |
| SR-08 | `ResolvedIdentity` from vnc-021 carries `[Read, Write, Search]` capabilities — observation processing may require `SessionWrite` or other capabilities not yet in the set, risking silent auth failures | Med | Med | Verify the full capability set needed by each of the 13 event handlers against what `ResolvedIdentity` provides |
| SR-09 | Existing UDS hook path regression — making `dispatch_request` pub(crate) and adding a capability parameter changes every UDS call site; a missed update silently breaks local hooks (ref #4473 warn+continue masking) | High | Low | Integration tests must cover UDS path unchanged behavior (AC-18); architect should plan the refactor as a single atomic change |

## Assumptions

- **Session identity sufficiency** (Proposed Approach §4): Assumes client-generated session IDs are globally unique enough that token+session_id scoping is unnecessary. If two users share a token (e.g., team deployment), sessions could collide.
- **Capability parity** (Background §Auth model): Assumes `[Read, Write, Search]` plus `SessionWrite` is sufficient for all observation event handlers. If any handler checks capabilities not in this set, it fails silently.
- **Wire format stability** (Constraints §6): Assumes `HookRequest`/`HookResponse` serde format is stable. Any change to the wire types (e.g., for #670 transcript_excerpt) affects both UDS and HTTP paths simultaneously.

## Design Recommendations

- **SR-07 is the critical path**: The architect must solve how `/observe` handler accesses service handles without breaking rmcp's service wrapping. This is the one structural problem that blocks everything else.
- **SR-01 + SR-09 together**: The `dispatch_request` refactor touches both transports. Design the capability parameter and context struct together, test atomically.
- **SR-03 + token scoping**: Even if deferred, the architect should document whether session_id is scoped per-token or globally, so the security model is explicit.
