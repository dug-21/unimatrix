# Agent Report: vnc-038-agent-3-risk (revision pass)

MODE: architecture-risk. Mirrored the human-approved scope revision (fold #735 in; confirm local-unaffected; tighten ADR-006; add ADR-008) into `RISK-TEST-STRATEGY.md`.

## Inputs read (UPDATED artifacts)
- SCOPE.md (AC-11/12/13, NFR-06, RD-1/AC-09/AC-10 local-unaffected, RD-5 local direct-binding).
- ARCHITECTURE/ADR-006 (REVISED — local keeps DIRECT path-hash binding, NOT a resolver key) + ADR-008 (NEW — token only via `v:2` bundle).
- SPECIFICATION.md (FR-15/16/17, AC-11/12/13, C-13).

## Changes applied to RISK-TEST-STRATEGY.md

### New risks
- **R-13 (Critical, High×Med) — Local STDIO/UDS routed through the unified resolver.** The concrete GATE-2 / C-13 guard. Scenarios: assert local STDIO (`main.rs:1158`) + UDS (`main.rs:859`) open `~/.unimatrix/{hash}/unimatrix.db` directly with no slug; assert they never call `parse_project_key` / construct the resolver / reference `ProjectKey::Default` / use a bundle; assert local is NOT a resolver-map key; cross-check ADR-004 deletions are HTTP-only.
- **R-14 (High, Med×Med) — First-boot bearer token leaks to stdout/logs.** ADR-008/AC-11/NFR-06. Scenarios: first-boot token-surface test asserts no token substring in stdout/captured `tracing` logs; sole-channel assertion (bundle carries token, no "also print" path); local non-regression (redaction deployment-context-gated, not unconditional removal — reconciled with ADR-006/R-13).
- **R-15 (Low) — #735 mechanical cleanups.** AC-12 (`router.rs` ≤500 lines) + AC-13 (`public_url.rs` stale `dead_code`/comment removed). Tracked, not over-weighted.

### Changed risks
- **R-07 re-scoped** from "breaks local UDS / MCP seam" to "breaks the MCP seam" — the local-UDS regression split out to R-13 because local now bypasses the resolver entirely (it is no longer "path-hash-as-key under the resolver"). Scenarios now cover only the HTTP seam, the #2398 call-site audit, and N=1-via-same-resolver.
- **R-11 superseded** (was Med/High → Low/Low). #735 folded in; the router/boot collision risk is dissolved by construction, not mitigated by coordination. Retained as a no-op for SR-06 traceability.

### Traceability
- **SR-06 (#735 collision) → resolved-by-fold-in** (no longer a sequencing risk); the carry-items now map to R-14 + R-15.
- **SR-04 (hard-cutover blast radius) → R-07, R-10, R-13.** The local-UDS blast-radius concern now points to the R-13 local-direct-binding guard (ADR-006 tightening), not "reconcile local under the resolver."
- Added rows mapping #735 CI-1→R-14 and CI-2/CI-3→R-15 (no original SR).

### Supporting sections updated
- Integration Risks: local is NOT a resolver seam (two independent boot-time wiring paths); token is single-channel by construction.
- Security Risks: added first-boot token-emission surface (blast radius = every project on the deployment).
- Failure Modes + Edge Cases: added local-direct-binding boot and token-surface rows; flagged the local exemption from the cloud-only loud-first-boot rule.
- Coverage Summary: Critical 6→8 (added R-13), High 4→5 (added R-14), Low 0→2 (R-11 superseded + R-15).

### Kept intact
- R-01..R-06, R-08..R-10, R-12 unchanged. N=2 proof for the observe funnel (R-02/R-09, C-11) intact.

## Open questions
- **OQ (delivery, R-14∩R-13):** Is `http/token.rs:101` reachable on the local STDIO/UDS path? ADR-008 requires confirming the print site is HTTP-first-boot-scoped; if shared, the redaction must be deployment-context-gated, not unconditionally removed, or local regresses. This needs a code-cited confirmation at delivery (the test is specced, but the gating mechanism depends on the answer).
- **OQ (residual, SR-04 assumption):** RD-1 assumes zero existing cloud/container HTTP deployments hold a default served store; if any do, AC-09's hard cut loses data despite "no users." Local is confirmed safe (GATE-2); the residual is cloud-only and remains a human-owned assumption, not a code risk.

## Knowledge Stewardship
- Queried: re-read UPDATED ADR-006 (#5085 tightened), NEW ADR-008, updated SPECIFICATION (FR-15/16/17, C-13, AC-11/12/13) -- applied to R-13/R-14/R-15, R-07 re-scope, R-11 supersede, SR-04/SR-06 re-mapping. Prior queries (#4974/#4956/#2398) carried forward.
- Stored: nothing novel to store -- the governing patterns (ceremonial-funnel N=2, parity-corpus atomicity, call-site audit, redact-secrets-from-logs) already exist; the "tightened ADR re-points a guard from under-the-resolver to bypass-the-resolver" reconciliation is single-feature, not yet a cross-feature (2+) pattern.
