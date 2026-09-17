# Gate 3a Report: nan-023

> Gate: 3a (Component Design Review)
> Date: 2026-09-16
> Result: PASS
> Feature: nan-023 — Per-Harness Idempotent Wiring, Non-Destructive Definition Install (JS package, `packages/unimatrix/**`)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS (2 WARN) | 8 components map 1:1 to Component Map + ADR-001..006; signatures match Integration Surface. WARN: `url?` vs Q1 token-free `transport` reconciliation; `writeCodexHooks -> WireLeg` vs `WireLeg[]` widening — both flagged with recommended resolutions. |
| 2. Specification coverage | PASS | Every FR-01..19 has pseudocode; NFR-01..10 addressed; no scope additions (gemini not detected; wire-only, no new observation capability). |
| 3. Risk coverage | PASS | R-01..R-16 each map to ≥1 test scenario; Critical R-01/02/03 carry mutation controls + pre-tag real-server exercise; priorities reflected. |
| 4. Interface consistency | PASS (1 WARN) | Shared `WireLeg` + `Transport` consistent across all files. WARN: multi-event hooks WireLeg shape recommended (internally consistent, flagged for 3b). |
| 5. Knowledge stewardship | PASS | Both 3a agent reports carry `## Knowledge Stewardship`; pseudocode (read-only) has `Queried:`; tester has `Queried:` + `Stored: #5769`. |

Gate-0 codex trust determination: recorded per the decision rule. 5/5 checks pass, 3 WARN. No FAIL.

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS (2 WARN)
**Evidence**:
- Component boundaries: pseudocode/ carries exactly the 8 Component-Map components (wire-orchestrator, skills-installer, opencode-retrieval, codex-install, toml-surgical, hook-client-provider, cli-routing, c14-verifier) + OVERVIEW. Matches ARCHITECTURE §Component Breakdown and the brief Component Map.
- ADR fidelity: ADR-001 (per-harness writers + `WireLeg` manifest) — wire-orchestrator aggregates a manifest, writers return legs. ADR-002 (in-house surgical block writer, not a round-trip lib) — toml-surgical.md manages only `[mcp_servers.unimatrix]`, copies foreign bytes verbatim. ADR-003 (hooks target JS hook client + `--provider` hint) — codex-install writes `node <clientPath> <EVENT> --provider codex-cli`; hook-client-provider wires `parseHookArgs` into the existing hint path (Q5: no normalizer arm added). ADR-004 (install-if-absent + `--force` skills-only) — skills-installer iterates the shipped source tree, never the destination. ADR-005 (`wire` verb + `--harness` + #960 intent) — cli-routing + intentGate. ADR-006 (verifier EXECUTES the manifest) — c14-verifier core discipline.
- Signatures match ARCHITECTURE Integration Surface / brief Function Signatures for `wire`, `detectHarnesses`, `installSkills`, `maybeWireCodex`, `writeCodexMcpToml`, `writeCodexHooks`, `upsertTomlTable`, `readTomlTable`, `buildHookClientCommand(…,providerHint?)`, `parseHookArgs`.

**WARN 1 (transport signature)**: ARCHITECTURE Integration Surface + brief list `writeOpencodeMcp(dir,{binaryPath?,url?},dryRun)` and `writeCodexMcpToml(dir,{binaryPath?,url?},dryRun)` carrying `url?`, but ADR-006 Q1 (DECIDED) forbids emitting `url=` for cloud codex and mandates the token-free bridge (`command="node", args=[bridge,hash]`). The pseudocode caught this latent source-doc inconsistency and threads a `Transport` descriptor (`stdio-binary` | `stdio-bridge`) in place of a bare `url`. Emitted bytes are the Q1-decided bytes either way. Assessed as a signature reconciliation resolvable in Stage 3b, not a design gap — the pseudocode's resolution is correct.

**WARN 2 (multi-event hooks leg)**: brief/ARCHITECTURE declare `writeCodexHooks(...) -> WireLeg` (single), but claude and codex hooks are multi-event, so one `WireLeg.command` cannot carry all 7 commands. Pseudocode recommends widening to `WireLeg[]` (one leg per event) so the manifest stays the sole execution source (non-tautology, R-02). Internally consistent (c14-verifier assumes one leg per event). Flagged for 3b.

### Check 2 — Specification coverage
**Status**: PASS
**Evidence**: FR→pseudocode map complete: FR-01/06 → cli-routing; FR-02/07/08/09 → skills-installer; FR-03/04/14/15 → wire-orchestrator; FR-05 → all writers (dry-run branches) + cli-routing; FR-10 → reused `writeMcpJson` wrapped in wire; FR-11 → opencode-retrieval; FR-12 → codex-install + toml-surgical; FR-13 → writeCodexHooks (targets JS client + `--provider codex-cli`, fail-loud throw if flag absent); FR-16 → c14-verifier return path; FR-17 → `skipped-*` legs carry `reason` (SR-08); FR-18 → `isWithinProject` in every writer; FR-19 → warn-and-skip / `skipped-malformed` in every writer.
NFR-01..10 all addressed: idempotence (`unchanged` short-circuit + byte-equality), byte preservation (surgical TOML slice-and-rejoin; opencode single-key mutate), malformed-safe, containment, local+cloud transport, backward-compat golden (SR-07), attribution fail-loud, TOML format preservation, trust surfaced (`codexTrustNote`), dry-run fidelity (same manifest drives both paths).
No scope additions: gemini intentionally not detected; no new per-harness observation capability; no protocol/agent install; no global-config writes.

### Check 3 — Risk coverage
**Status**: PASS
**Evidence**: RISK-TEST-STRATEGY R-01..R-16 each carry test scenarios in a component plan (test-plan OVERVIEW §3 map + per-component plans):
- Critical: R-01 ceremonial wiring → c14-verifier return+fire from manifest; R-02 tautology → mutation controls (`test_verify_*_mutation_broken_*_fails` per behavioral AC); R-03 cloud never-green-on-tag → `test_pretag_real_server_exercise` + feasibility-matrix recording.
- High: R-04 → toml-surgical foreign-byte + adjacency + CRLF tests; R-05 → hook-client-provider static flag + parity-corpus #4751 + c14-verifier behavioral `provider` assertion (source_domain excluded); R-06 → Gate-0 + untrusted-surface tests; R-07 → golden files captured pre-refactor; R-08 → opencode sentinel byte-diff + return; R-09 → dual-limit size-gate test + meta-assertion lockstep.
- Medium: R-10 injection fixtures per surface; R-11 skip-reason distinctness; R-12 intent both arms; R-13 `--force` zero-wiring; R-14 dry-run/containment/global-path; R-15 per-event evidence; R-16 first-run + non-clobber.
Edge cases (adjacency, non-alpha key order, CRLF, metacharacter paths) and integration risks (wire→writers→manifest contract, JS/Rust normalizer twins, root resolution parity) are enumerated. Behavioral-outcome lens rows all mapped to command-driven scenarios.

### Check 4 — Interface consistency
**Status**: PASS (1 WARN)
**Evidence**: `WireLeg` defined once in OVERVIEW with invariants (command iff hooks; entry iff mcp/retrieval; reason iff skipped-*) and used identically across wire-orchestrator, codex-install, opencode-retrieval, c14-verifier. `Transport` descriptor resolved once by the orchestrator and threaded to MCP writers. Data flow coherent: `bin` → `init`/`wire` → `wire()` → dispatch → manifest → verifier consumes the same manifest. `EVENT_MATCHERS`/`PRETOOLUSE_CYCLE_MATCHER` reused so codex and claude event sets stay in step.
**WARN (carried from Check 1 WARN 2)**: the single-vs-array hooks-leg shape is the one interface point needing a 3b decision; all files already assume the one-leg-per-event resolution, so there is no contradiction between component files.
Also noted (not blocking): intent-gate single-site rule — pseudocode shows opencode gated in the orchestrator and codex self-gated; both files explicitly instruct "pick ONE site, assert once." Consistent guidance, resolvable in 3b.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS
**Evidence**:
- pseudocode agent (read-only tier): `## Knowledge Stewardship` present with `Queried:` (context_briefing + context_search surfacing ADR-001..006, #5737, #5743, #5372/#4780, #1195) and an explicit "Read-only tier — nothing stored" with reason. Compliant (Queried entries present; reason given).
- tester agent: `## Knowledge Stewardship` present with `Queried:` (context_briefing + context_search, gate-3b test-plan-fidelity lessons) and `Stored: entry #5769` via context_store. Compliant.

## Open-Question Triage (design-blocking vs Stage-3b detail)

The pseudocode agent raised 6 open questions; all resolvable in Stage 3b, none design-blocking. Concur:

| # | Question | Assessment |
|---|----------|-----------|
| 1 | Transport signature (`url?` → `transport`) | 3b detail. Emitted bytes are Q1-decided either way; reconciliation is signature-only. |
| 2 | Multi-event hooks WireLeg shape | 3b detail. Recommend `WireLeg[]`; all files internally consistent with it. |
| 3 | Intent-gate single-site rule | 3b detail. Both files instruct pick-one-site-assert-once; test plan verifies once. |
| 4 | claude routing through `wire()` | 3b detail with SR-07 guard. R-07 golden files captured before refactor de-risk it. |
| 5 | Standalone `wire` transport resolution | 3b detail. Recommended `resolveBinary()` + cloud fallback; document for dry-run parity. |
| 6 | opencode `mcp` server schema | 3b detail with a behavioral safety net. Highest actual-correctness risk (wrong schema → retrieval won't return), but the c14-verifier AC-10 return assertion executes the entry, so an unspawnable shape fails loudly rather than shipping green. Confirm against shipped reference in 3b. |

## Gate-0 / Load-Bearing Constraint Verification

- **Gate-0 codex trust determination recorded per the decision rule** — PASS. test-plan/OVERVIEW.md §2 records cloud trust-seeding NOT feasible in CI (repo-confirmed: codex-cli absent from package.json/CI/Dockerfiles; `.codex/` trust mechanism undocumented in-repo). Consequence table keeps codex-LOCAL firing, command-level cloud firing, and both retrieval-return arms HARD (the wired-command execution bypasses codex; the bridge is codex-independent); only codex self-firing the 7 events is a DOCUMENTED CONDITIONAL, recorded per-event as `wired-inactive (untestable-in-CI)` — never silent-skipped. C14 codex leg claimed proven(local)/partial(cloud). Matches the brief decision rule exactly.
- **R-09 hook-client size gate never raised** — PASS. hook-client-provider.md marks it a HARD CONSTRAINT; addition is lean (`parseHookArgs` + one `main()` branch, reuse `mapToCanonical`/`KNOWN_PROVIDERS`, no new module); dual-limit test (stripped ≤110KB PRIMARY, raw ≤200KB backstop) + meta-assertion lockstep; explicit "never minify, never raise (#5372/#4780)."
- **C-04 `--provider codex-cli` fail-loud** — PASS. writeCodexHooks throws if the built command lacks the flag (NFR-07); AC-07 test asserts the flag on every one of the 7 commands.
- **C-12 `--force` definitions-only, never forwarded to wire** — PASS. skills-installer scopes `--force` to skill writes; cli-routing `routeWire` rejects `--force` with a usage note and never threads it into `wire()`; AC-02 wiring-diff-empty asserted at the CLI boundary.
- **C-09/C-11 every writer non-clobbering / idempotent / dry-run / containment** — PASS. Verified per writer (opencode-retrieval, codex TOML, codex hooks, skills).
- **C14 verifier EXECUTES the manifest (non-tautology, SR-09/R-02) with mutation controls** — PASS. c14-verifier §Core discipline forbids string-match/path-reconstruction; `assertHookFires` spawns the exact `WireLeg.command`; `assertRetrievalReturns` connects `WireLeg.entry`; each behavioral AC (AC-09, AC-10) carries a mutation control that fails when the artifact is broken but the manifest string is intact.

## Rework Required

None. Result is PASS; the WARN/open-question items are Stage-3b implementation reconciliations with recommended in-file resolutions, not gate blockers.
