# Agent Report — vnc-039-agent-2-spec

## Deliverable
- `product/features/vnc-039/specification/SPECIFICATION.md`

## Summary
Translated SCOPE.md (AC-01..AC-11, Resolved Decisions, Constraints) and SCOPE-RISK-ASSESSMENT.md (SR-01..SR-09) into a structured specification.
- 27 functional requirements (FR-01..FR-27) across: bridge MCP lifecycle proxying, stdio JSON-RPC framing, application/json + SSE response handling, cert-pin pinned-flush, `.mcp.json` remote-write, out-of-tree per-slug credential store, legacy unsupported path.
- 8 non-functional requirements (zero-dep-by-decision, NFR-06 no-token-to-logs, mode 0600, cleartext-at-rest accepted, fail-loud trust boundary, bounded build surface, cumulative tests).
- All 11 ACs turned into verifiable criteria with explicit validation tiers ([stub/local] #774-not-validated-live vs [no-cloud] Scope B).
- Domain model defines every requested term incl. slug vs projectHash distinction and the reconciled store schema.

## Key requirements / decisions
- **Schema-mismatch reconciliation (FR-23) spec'd as a hard requirement**, not a port: new store has ONE schema; hook client must read `observe_url` + `fingerprint` (not `url`), fixing the latent unpinned/UDS-fallback bug (SR-07).
- **Pinned-flush (FR-11/FR-12)** spec'd as token-after-pin-only + fail-loud + socket-destroyed-before-token-byte, with an AC-04 verification requiring a capturing test server to prove no `Authorization` header on wrong-pin (SR-02).
- **One agreed store key (FR-26)** spec'd as a requirement; exact key/path deferred to architecture (OQ-6).
- Scope A/B independence and #774 sequencing recorded as caveats on every Scope-A AC (SR-04/SR-05).

## Open questions for architect/human (both architecture-phase, not scope blockers)
- **OQ-4** entrypoint shape (subcommand vs `node <bridge>` in `.mcp.json`).
- **OQ-6** store path/layout AND the single key (slug vs projectHash). Flagged as highest-leverage pre-code decision: divergent keys = one consumer silently fails to resolve (SR-07/SR-08).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — #5081 (v:2 bundle ADR), #5088 (token-never-to-stdout), #5104 (bugfix-774 host-allowlist lesson), #4355 (clientInfo.name/session). All consistent with SCOPE; no conflicts.
- Stored: nothing — read-only tier; spec decisions are feature-specific.
