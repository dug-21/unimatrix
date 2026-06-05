# ASS-065: rmcp 0.16→1.4 Migration — API Surface and Transport Impact

**Tracking**: #666
**CVE**: CVE-2026-42559 (DNS rebinding on Streamable HTTP transport, high severity)

---

## Goal

Answerable questions this spike must resolve:

1. **What rmcp APIs, types, traits, and features does `unimatrix-server` use across all source files?** — Complete inventory of our integration surface.
2. **What breaking changes occurred in rmcp between 0.16.0 and 1.4.0?** — Exhaustive changelog/migration guide review covering every minor/major release.
3. **Which of our used APIs are affected by breaking changes, and how?** — Cross-reference of Q1 inventory against Q2 changelog.
4. **What is the impact on vnc-021's transport layer (HTTPS listener, bearer token auth, session management)?** — Specific assessment of transport-related changes.
5. **What is the realistic migration effort: patch-level fix, feature-level rework, or architectural rework?** — Effort estimate with rationale.
6. **Can the DNS rebinding fix (CVE-2026-42559) be cherry-picked or backported to 0.16.x without the full migration?** — Feasibility and risk of a targeted patch vs. full upgrade.

## Breadth

`code+ecosystem` — requires both internal codebase analysis (28+ files using rmcp) and external ecosystem investigation (rmcp changelog, GitHub releases, commit history, CVE details).

## Approach

`investigation` + `evaluation` — investigate what changed, evaluate migration paths.

## Confidence Required

`validated` — migration recommendations must be grounded in specific API mapping, not directional guesses. The cross-reference of "what we use" vs. "what changed" must be exhaustive.

## Target Outputs

FINDINGS.md containing:
- Complete API usage inventory (types, traits, features, by file)
- Breaking change catalog (0.16→1.0→1.4)
- Impact matrix: each used API × each breaking change → affected/unaffected/renamed/removed
- Transport layer impact assessment specific to vnc-021
- Migration path recommendation (full upgrade vs. targeted backport vs. hybrid)
- Effort estimate with risk factors
- Go/no-go recommendation for immediate migration vs. deferred

## Constraints

### Hard
- `unimatrix-server` is the only crate with rmcp dependency — migration is scoped to one crate
- We use features: `server`, `client`, `transport-io`, `macros`, `transport-streamable-http-server`, `transport-streamable-http-server-session`
- Version is pinned with `=0.16.0` — not a range
- vnc-021 HTTPS transport + bearer token auth was built against 0.16 API

### Hypothesis
- A full upgrade to 1.4.0 is preferable to a backport, if migration effort is manageable (challengeable if backport is significantly simpler and equally secure)
- The DNS rebinding CVE is the primary urgency driver (challengeable if other vulnerabilities exist in the 0.16→1.4 range)

## Dependencies

- **Requires**: #663 closed (chunked TE body limit — ✅ merged)
- **Unblocks**: actual migration implementation (future delivery session)

## Prior Art

- vnc-021 delivery artifacts: `product/features/vnc-021/` — the original transport layer implementation
- Dependabot alerts #18, #19 — CVE-2026-42559 details
- rmcp crate on crates.io / GitHub — changelog and release notes
