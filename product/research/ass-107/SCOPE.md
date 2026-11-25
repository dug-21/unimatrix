# ASS-107: MCP 2026-07-28 / rmcp 3.x adoption revisit

## Question
Is the MCP `2026-07-28` spec and the rmcp 3.x line now ready for Unimatrix to adopt? Does adopting it simplify personal-cloud attach (vnc-050 / #1010, #1014)? Can we adopt it without modifying existing clients?

## Why it matters
The MCP wire is the primary surface for both `personal-cloud` and `platform`. ass-105 (#971) deferred adoption on 2026-07-28 because the new spec was implemented only in rmcp 3.0.0-beta. It set a revisit for about 2026-08-28, and that revisit is overdue. vnc-050 is now changing the HTTP bridge and auth path, so we should know whether to design toward the stateless wire before building more on the stateful one. We pin `rmcp =1.7.0` (two spec revisions behind), and any move off it is a real migration.

## Prior art (build on, do not redo)
- `product/research/ass-105/ass-105-findings.md`: spec delta, security impact (auth boundary unaffected), rmcp version table, upgrade verify-list.
- Unimatrix pattern #4699: rmcp coupling is concentrated in about 3 files, and the proc-macro surfaces have stayed stable.
- vnc-050 SCOPE (`product/features/vnc-050/SCOPE.md`): bundle-based headless attach and scoped tokens.

## Bounded questions (priority order)
- **A. Readiness.** Is rmcp 3.x stable, with release dates? Did `2026-07-28` finalize as the RC described, and what changed since the RC? What is rmcp 3.x's declared MSRV compared with our workspace `rust-version`? What do the conformance-suite results show?
- **B. Backward compatibility (the gating question).** Can one server serve `2025-11-25` stateful clients (today's bridge, hook client, Claude/Codex/OpenCode) and `2026-07-28` stateless clients at the same time, with **zero client changes**? Verify the following:
  1. Does `peer_info()` / clientInfo stay populated under stateless serving (it is our attribution anchor)?
  2. What replaces the `Mcp-Session-Id`-keyed `client_type_map` (vnc-014) for stateless callers?
  3. Does the `ResolvedIdentity` that `auth.rs` inserts into request extensions still reach `build_context` in both modes?
  4. Which harness clients have already moved to the new spec, and which are still on the old one?
- **C. Simplification for vnc-050.** Concretely, what does the stateless wire remove from the bridge and server, such as session handling, `Mcp-Session-Id` passthrough, and sticky routing behind Tailscale or Cloudflare? Should vnc-050's bridge be written stateless-ready, or kept agnostic to the spec version? Does any SEP change the setup surface (bundle, tokens, CA trust)? Expected answer: mostly not, but confirm.
- **D. #1014 impact.** Do the spec's authorization changes (OAuth 2.1 / resource metadata) affect the claude.ai connector path? Hand the findings to that spike; do not answer #1014 itself.
- **E. Migration cost.** Is the upgrade path 1.7 → 2.x → 3.x one step or staged? What breaking hits land at our call sites? Estimate the effort in sessions.

## Method
- **Breadth:** code+ecosystem
- **Approach:** investigation (no PoC required)
- **Confidence required:** directional (all questions)

## Output
`product/research/ass-107/ass-107-findings.md` containing:
- A GO/DEFER/NO-GO recommendation on adoption, with a revisit trigger if DEFER.
- A yes/no on backward compatibility, with the evidence for each verify-item in B.
- Direct guidance to vnc-050 design for C.
- A migration sketch for E.

## Constraints
- Research only. No edits to `crates/` and no branch upgrades. A throwaway local proof of concept is allowed to verify the B items.
- Label pre-release material as such. Never treat a beta as stable.
- Security boundary claims (bearer, TLS pin, capability-after-identity) must be re-checked, not inherited from ass-105.

## Tracking
GH #1019. Related: #971 (ass-105), #1010 (vnc-050), #1014 (ass-108).
