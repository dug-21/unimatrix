# Agent Report — vnc-037-agent-2-spec

## Task
Lock OQ-A (FAIL LOUD) into SPECIFICATION.md as an explicit, verifiable requirement. Additive only — no renumbering of existing FRs/ACs.

## Artifact
- `product/features/vnc-037/specification/SPECIFICATION.md` (updated in place)

## Changes (all additive)
- **FR-19** — fail-loud error contract: a post-primary-read edge/count/title failure FAILS the whole `context_get`, propagating via the **same error mapping as the primary-read failure path**; no degrade-with-note, no silent edge omission. Scoped to the default-on path (the opt-out path skips the queries and cannot reach it). Rationale locked in text (silent-omit indistinguishable from true zero-edge; one consistent failure contract, no new partial-success shape).
- **AC-14** — verifiable: (a) edge-query-failure-fails-loud (RED) for the ranked query, split COUNT, and title join → assert the get FAILS (mapped `ServerError`), not success-with-omitted-edges; (b) zero-edge success (FR-12) is structurally DISTINCT from a failed get, never conflated; (c) static no-unwrap on the edge path.
- **C-13** — fail-loud constraint + the "edges unavailable" distinct-marker requirement should a degrade path ever be reintroduced (a bare omission is never acceptable).
- **AC-10** — FR range bumped to FR-1..FR-19; test inventory gains the named **edge-query-failure-fails-loud** case (cross-asserts the zero-vs-failure distinction).
- **Open Questions** — OQ-A moved to a new "Resolved (human-directed)" subsection; no longer open.
- **Knowledge Stewardship** — recorded this agent's briefing query (MCP unavailable, non-blocking).

## IDs added
- **FR-19**, **AC-14**, **C-13** (plus AC-10 FR-range/inventory update). No existing FR/AC renumbered.

## Verification confirmation
- **Fail-loud contract is verifiable** — AC-14(a) names a RED failure-path test for each of the three edge queries, asserting a mapped error rather than a success-with-omitted-edges payload.
- **Zero-vs-failure distinction is verifiable** — AC-14(b) asserts a genuine zero-edge entry returns success (FR-12 empty state) that is structurally distinct from the AC-14(a) error result; the two are never conflated.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — no results (Unimatrix MCP unavailable in environment; non-blocking per spec protocol). Proceeded from in-document prior briefing findings + RISK-TEST-STRATEGY R-16 (fail-loud, mapped `ServerError`, run RED per #4876). Read-only tier — no storage.
