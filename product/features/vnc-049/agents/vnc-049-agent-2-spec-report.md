# Agent Report — vnc-049-agent-2-spec

## Deliverable
`product/features/vnc-049/specification/SPECIFICATION.md`

## Coverage
All 7 acceptance criteria (AC-01..07) present and mapped to functional requirements
(FR-01..12) with verification methods. All 12 SR-XX risks reflected in constraints/ACs.

## Key decisions
- **provider vs source_domain vs model-id** codified as distinct terms in Ubiquitous
  Language; `--provider opencode` sets provider only, source_domain needs an explicit
  resolution path, backend-model needs a new carrier.
- **AC-03** written to assert the STORED record resolves `source_domain=opencode` (+ negative
  assertion vs claude-code default), not that the flag was passed (SR-07).
- **AC-06** written as a behavioral end-path assertion (local-model event lands + queryable
  distinct from cloud through the real assembled path), not carrier-field-populated (SR-01/10).
- **AC-04** verified on the stored subagent record aligned to owning feature/cycle; validated
  `session.agent` not folded into spoofable tool args (SR-09, protects AC-07 seam).
- **AC-05** requires byte-for-byte preservation + retrieval regression assertion + idempotence
  (SR-08).
- **C18 ledger guardrail** stated authoritative: `partial` until AC-06 demonstrated,
  independent of E2 sizing.

## Open questions surfaced for architect
- OQ-1 E2 sizing (in-cycle vs fast-follow); OQ-4 source_domain resolution mechanism to
  reconcile (#4306 listener-provider path vs #5737 DomainPack default); OQ-5 model-carrier
  shape (payload vs typed field, parity-corpus blast radius); OQ-3 session.idle→Stop
  semantics PoC; OQ-2 targeted carve; OQ-6 subagent MCP connection; OQ-7 PreCompact stability.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- #5737 (harness three-touchpoints pattern),
  #4306 (vnc-013 provider-field ADR). Read-only tier; no storage.
