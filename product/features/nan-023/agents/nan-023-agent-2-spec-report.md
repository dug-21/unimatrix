# Agent Report — nan-023-agent-2-spec (uni-specification)

## Deliverable
`product/features/nan-023/specification/SPECIFICATION.md`

## Summary
Authored SPECIFICATION.md from SCOPE.md and SCOPE-RISK-ASSESSMENT.md. 19 functional requirements (FR-01..FR-19), 10 non-functional requirements (NFR-01..NFR-10), all 16 acceptance criteria (AC-01..AC-16) with per-AC verification methods, domain model / ubiquitous language table, 8 user workflows, 13 constraints, dependencies, and explicit NOT-in-scope.

## Key decisions / interpretations
- **Behavioral ACs guarded against ceremonial wiring (SR-09):** AC-05 (retrieval returns), AC-08/09 (codex hooks target JS client + actually fire), AC-10 (retrieval returns) each carry a verification method that asserts FROM the wired command path and explicitly rejects config-presence/proxy discharge.
- Scoped strictly to the installation package's output into consumer repos; no "fix local config" behavior specified.
- Definition install held to skills-only; `--force` documented as definitions-only with an explicit "zero wiring changes" assertion (AC-02, SR-04).
- Codex hooks specified to target `packages/unimatrix/lib/hook-client/index.js` (`node <clientPath> <EVENT>`), mirroring the existing Claude hook command form (confirmed in init.js:502–518, :457), with mandatory `--provider codex-cli`.
- SR-01..SR-12 folded into NFRs/constraints (TOML format-preservation, trust precondition surfacing, warn-and-skip visibility, dry-run fidelity, containment, backward-compat golden-file).

## AC coverage
All AC-01..AC-16 present and traced to FRs with verification methods. gemini-cli excluded per SCOPE deferral.

## Open questions surfaced to architect
1. TOML writer implementation (library vs in-house, format-preserving).
2. Exact Codex hook lifecycle event set (Claude-mirroring).
3. Cloud vs local feasibility pinning for AC-09/AC-10 "where feasible"; pre-tag real-server exercise (SR-03).
4. Trust-precondition surfacing mechanism for untrusted `.codex/` (NFR-09, SR-02).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — #5743 (vnc-049 ADR-006), #5737 (harness onboarding touchpoints), #5755 (opencode shim trap), #4925 (nan-016 install-dir). No net-new generalizable pattern stored (read-only tier).
