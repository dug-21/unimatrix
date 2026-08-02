# ASS-105 — New MCP Specification Release: Spec Delta, Security-Model Impact, rmcp Upgrade Path, Adoption Assessment

## Question
A new official MCP specification is scheduled to publish ~2026-07-28 (≈2 days). rmcp has churned
rapidly for weeks, presumably tracking it. What is in the new spec, and what does adopting it do to
Unimatrix — **security model first**, then the rmcp/Rust upgrade path, then everything else it touches?

## Why it matters
The MCP wire is Unimatrix's primary surface: both the OSS deploy contract (`personal-cloud`) and the
L3 solution contract the platform goal presents to extenders. A spec version bump can move auth,
transport, and schema requirements underneath us. We pin `rmcp =1.7.0` / `rmcp-macros =1.7.0`
deliberately (ADR-001); an upgrade is a decision, not a default. We need the impact map before the
spec lands, not after.

## Approach & bounded questions (in priority order)

### A. Understand the new spec (do first)
- What version/date, what changed vs. the version rmcp 1.7.0 implements? Produce a concrete delta:
  added, changed, deprecated, removed.
- Which changes are mandatory for compliance vs. optional features?

### B. Security-model impact (the priority flagged by the human)
- Does it change the auth story — OAuth 2.1 / resource-server metadata, bearer handling,
  protocol-version headers, session binding? Map against our current model: sole bearer token, TLS
  fingerprint pinning, `BearerValidator` trait, capability-check-after-identity (Principle 3),
  per-slug routing.
- Does it affect the anticipated L2 identity seam (relying-party verifier, ass-100/101)?
- Any new attack surface (elicitation, sampling, resource fetch, async tasks) that touches the
  integrity / poison-resistance posture?

### C. rmcp / Rust upgrade path
- What rmcp version implements the new spec, and is it released or pre-release on ~2026-07-28?
  Delta from 1.7.0: our pinned features, `#[tool_router]` / `#[tool_handler]` macros, schemars
  schema-gen, Streamable HTTP transport.
- Rust edition / toolchain / MSRV bumps pulled in. Breaking API changes at our call sites.
  Lockstep `rmcp` + `rmcp-macros` re-pin (#902 class).

### D. Other impacts (researcher extends beyond the above)
- Wire-contract stability for existing clients — Claude Code, Codex CLI, Gemini CLI — and multi-LLM
  parity (C14). Backward-compat / dual-version negotiation window during transition.
- New protocol capabilities as *opportunities* mapped to Unimatrix tools (structured tool output,
  elicitation, resource links, etc.) — flag, don't design.
- Client-SDK semver co-evolution (platform L3 seam).
- **Timing recommendation:** adopt-now vs. wait-for-stable-rmcp, with rationale.

## Output
A findings doc (`ass-105-findings.md`): the spec delta, a security-model impact assessment with a
go/no-go-now posture, an rmcp upgrade-cost estimate, and a ranked list of other impacts — enough for
a uni-zero decision on whether/when to open a delivery or design cycle.

## Constraints / prior art
- ADR-001 (rmcp exact-pin rationale), ass-002 (SDK landscape), ass-050 (bearer security model),
  ass-100/101 (edge identity + root-of-trust).
- Needs external/web research for the spec itself + codebase analysis for impact — likely a single
  spike spanning both, not a campaign.
- Current pin: `rmcp = "=1.7.0"`, `rmcp-macros = "=1.7.0"` (`crates/unimatrix-server/Cargo.toml`).
