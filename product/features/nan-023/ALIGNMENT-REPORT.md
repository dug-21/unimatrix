# Alignment Report: nan-023

> Reviewed: 2026-09-16
> Artifacts reviewed:
>   - product/features/nan-023/architecture/ARCHITECTURE.md
>   - product/features/nan-023/specification/SPECIFICATION.md
>   - product/features/nan-023/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Goal source: Unimatrix #5731 (`personal-cloud`); capability #5729 (C14)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances `personal-cloud`; three parity legs (claude-code, opencode, codex-cli) match the goal's stated multi-LLM set exactly; JS-client stance honors principles 5/6/8 |
| Milestone Fit | PASS | Targets C14 (North-star curve); correctly scopes out C17 (already proven), 3-way-merge, observation depth — no over-reach |
| Scope Gaps | PASS | All 6 SCOPE goals and AC-01..AC-16 traced across all three source docs |
| Scope Additions | PASS | New mechanisms (WireLeg manifest, `--provider` argv parse, surgical TOML) are implementations of stated ACs, not new scope; normalize.js/source_domain depth explicitly bounded out |
| Architecture Consistency | PASS | Tight cross-referencing of ADR/SR/AC IDs; gemini deferral consistent across all three docs (satisfies pattern #3742) |
| Risk Completeness | WARN | 16-risk register is complete and mapped to all 12 SRs, but AC-09/AC-10 cloud+codex closure depends on two unresolved design-phase open questions; SCOPE's "where feasible" hedge could let cloud codex parity slip silently |

**Counts:** PASS 5, WARN 1, VARIANCE 0, FAIL 0.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE AC-01..AC-16 is traced in SPECIFICATION §6 and mapped to a risk (R-01..R-16); each architecture component maps to a stated goal/AC |
| Addition | `WireLeg` manifest + manifest-driven C14 verifier (ARCHITECTURE) | Not in SCOPE by name, but is the mechanism that makes AC-09/AC-10 non-tautological (SR-09). Implementation detail serving a stated AC — not scope expansion |
| Addition | `--provider <name>` argv parsing in `hook-client/index.js` (ARCHITECTURE ADR-003) | Mechanism to satisfy SCOPE's mandatory `--provider codex-cli` constraint (C-04/NFR-07). In-scope as implementation |
| Addition | `.codex/hooks.json` filename + surgical TOML writer (ADR-002) | Concretizes SCOPE's "Codex hooks, Claude-like shape" and "new TOML writer" constraint. Architecture-level specificity, not new scope |
| Simplification | Definition install = install-if-absent + `--force`, skills only | Rationale (SCOPE Non-Goals, ADR-004): definitions are git-tracked/recoverable; beats hash-tracking/3-way-merge on simplicity. Documented and consistent across docs |
| Simplification | gemini-cli deferred | Rationale: matches the goal's own gemini deprioritization (Google pivot to Antigravity, #5731). Consistent deferral across all three docs |

## Variances Requiring Approval

None. No VARIANCE or FAIL. One WARN below is for human awareness, not approval.

### WARN (awareness only) — cloud/codex parity closure depends on unresolved open questions

1. **What**: AC-09 (codex hooks fire) and AC-10 (retrieval returns) carry cloud arms that depend on two unresolved design-phase open questions: (a) codex MCP transport in cloud — `url=` vs `command=node <bridge>` (ARCHITECTURE Open Q1; RISK R-03, rated Critical); (b) whether the local+cloud CI harness can mark a fixture `.codex/` layer trusted (ARCHITECTURE Open Q2; RISK R-06; SR-02).
2. **Why it matters**: C14 parity is the goal's load-bearing outcome ("Multi-LLM connect identically... same over HTTPS as over local", #5731). SCOPE's "assert both where feasible" language (C-07, AC-09/AC-10) is a legitimate hedge, but if unresolved it can let cloud codex parity ship as claimed-but-inert — the exact ceremonial-wiring / never-green-on-tag failure the risk strategy itself flags (R-01/R-03, evidence #5267).
3. **Recommendation**: Accept as-is for design-phase — the docs honestly flag both as open questions escalated to the design leader/human, and the risk strategy prescribes a pre-tag real-server exercise (ADR-006) plus a per-AC cloud-vs-local feasibility matrix. Human awareness point: require the feasibility matrix to state explicitly which AC arms run green in cloud vs local-only before the parity claim is made, so "where feasible" resolves to a stated fact, not a silent gap.

## Detailed Findings

### Vision Alignment
The feature advances `personal-cloud` (#5731), whose intent commits to "Multi-LLM (Claude Code, Codex CLI, and OpenCode) connect identically via HTTPS." The three parity legs in SCOPE (claude-code, opencode, codex-cli) match that set exactly, and gemini-cli's deferral mirrors the goal's own note that "Google is pivoting away from the Gemini CLI harness toward Antigravity." Capability C14 (#5729) done_when — "the client bridge is wired into each harness's MCP config... and each drives context_* against one slug" — is precisely what the feature's return-path ACs (AC-10) assert. Architectural principles hold: work is confined to the JS/TS edge client (principle 6, "the client is an adapter"); warn-and-skip per leg is graceful degradation (principle 5); RISK Security Risks assert no token is logged or written to any definition file (principle 8). No shortcut contradicts the vision.

### Milestone Fit
The feature targets the current North-star curve item (C14) and explicitly declines adjacent work: C17 (server-side config seeding) is cross-linked as already-proven, not re-built; 3-way-merge/hash-tracking is rejected; per-harness observation depth (C18/vnc-049) is out of scope; gemini is deferred. No future-milestone capability is pulled forward. Milestone discipline is clean.

### Architecture Review
ARCHITECTURE.md defines an orchestrator (`lib/wire.js`) + one non-clobbering writer per (harness × surface), reusing the `mergeSettings`/`opencode-install.js` principle. The load-bearing stance — a leg is "wired" only when the `WireLeg` manifest's exact command yields a returning `context_*` call / firing hook — is the correct answer to the ceremonial-wiring risk (SR-09) and is consistent with the goal's fidelity commitment. Modularity is tracked (hook-client size gate flagged as the single tight resource, ~10 KB raw headroom, with an explicit "never raise the gate" note per #4780). Two open questions (codex cloud transport, CI trust-gating) are surfaced rather than buried — see WARN. Consistent with the per-harness config-surface matrix pattern (#5761/#5762).

### Specification Review
SPECIFICATION.md traces all 16 SCOPE ACs (§6) with a per-AC verification method, and explicitly guards the behavioral ACs (AC-05/08/09/10) against config-presence proxies. Domain models bound gemini as "known but out of scope" and pin definitions to skills-only. §9 restates all SCOPE non-goals; §8 correctly notes normalize.js/source_domain depth is out of scope and flagged for downstream, preventing scope creep into Rust attribution. No AC is discharged by tautology. Alignment with SCOPE is complete.

### Risk Strategy Review
RISK-TEST-STRATEGY.md carries a 16-risk register with full SR-01..SR-12 → R-01..R-16 traceability, correct severity weighting (R-01 ceremonial wiring and R-02 tautological verifier rated Critical alongside R-03 cloud never-green-on-tag), and a behavioral-outcome coverage table that drives the real `init`/`wire` command rather than a seam beneath it. Security coverage (command injection via interpolated paths, containment, malformed-input corruption, foreign-content tamper, token non-logging) is thorough. The only completeness caveat is that the closure of AC-09/AC-10 cloud arms rests on open questions the strategy itself flags (R-03/R-06) — captured as the WARN, not a coverage gap.
