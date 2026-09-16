# Agent Report — nan-023-agent-1-pseudocode (Stage 3a pseudocode)

## Deliverables (all under product/features/nan-023/pseudocode/)

| File | Component |
|------|-----------|
| OVERVIEW.md | Component interaction, WireLeg shared type, Transport descriptor, data flow, wave ordering, integration surfaces, modularity budget |
| wire-orchestrator.md | `lib/wire.js` — `wire`, `detectHarnesses`, intent gate, WireLeg aggregation |
| skills-installer.md | `lib/init.js` `copySkills`→`installSkills` (install-if-absent + `--force`) |
| opencode-retrieval.md | `lib/opencode-install.js` `writeOpencodeMcp` (additive `mcp.unimatrix`) |
| codex-install.md | `lib/codex-install.js` `maybeWireCodex`/`writeCodexMcpToml`/`writeCodexHooks` |
| toml-surgical.md | `lib/codex-install.js` internal `readTomlTable`/`upsertTomlTable` |
| hook-client-provider.md | `lib/hook-client/index.js` `parseHookArgs` + `merge-settings.js` `buildHookClientCommand` 3rd arg |
| cli-routing.md | `bin/unimatrix.js` `wire` verb, `--harness`, `--force`, `--dry-run` |
| c14-verifier.md | `test/` manifest-executing verifier (non-tautology) |

Component list matches the IMPLEMENTATION-BRIEF Component Map exactly (8 components; OVERVIEW is the cross-cutting 9th artifact).

## Dependency ordering (wave plan for Stage 3b)

- **Wave A (leaves):** toml-surgical (pure string), hook-client-provider (size-gated — do first & guard), skills-installer (self-contained in init.js).
- **Wave B (writers):** opencode-retrieval; codex-install (needs A1 toml-surgical + A2 `buildHookClientCommand` 3rd arg).
- **Wave C (orchestration):** wire-orchestrator (needs B1/B2 + reused claude writers); cli-routing (needs C1 + A3).
- **Wave D:** c14-verifier (consumes the WireLeg manifest from C1; fires the exact command from A2).
- **Before Wave C:** capture claude-code golden files (SR-07) BEFORE routing `writeMcpJson`/`mergeSettings` through `wire.js`.
- **Before Wave A:** Gate-0 (Q2) codex trust feasibility confirmed by tester — decides AC-09/AC-10 codex-cloud HARD vs documented-conditional.

## Open questions / gaps flagged (for Stage 3b / architect)

1. **Transport signature reconciliation (primary).** Integration-Surface signatures carry `url?` for `writeOpencodeMcp`/`writeCodexMcpToml`, but Q1 (DECIDED) forbids emitting `url=` for cloud codex and mandates the token-free bridge (`command="node", args=[bridge, hash]`), which needs `bridgePath`+`projectHash`. Pseudocode threads a `Transport` descriptor (`stdio-binary` | `stdio-bridge`) from the orchestrator in place of a bare `url`. Recommend the implementer adopt `{ transport }` opts. Behavior/bytes unchanged — the emitted bytes are the Q1-decided bytes either way. Flagged in OVERVIEW, opencode-retrieval, codex-install.
2. **Multi-event hooks WireLeg shape.** claude-code and codex hooks are multi-event, so a single `WireLeg.command` cannot carry all events. Recommend emitting ONE hooks `WireLeg` per event (each with its exact `command`) so the manifest stays the sole execution source (non-tautology, R-02). This widens `writeCodexHooks -> WireLeg[]` (declared as single `WireLeg`). Flagged in wire-orchestrator + codex-install.
3. **Intent-gate site (single-site rule).** The codex/opencode NEW-entry gate can live in the orchestrator OR self-gated in the writer. Pseudocode shows opencode gated in the orchestrator and codex self-gated for cohesion — implementer must pick ONE site per surface and keep it single (asserted once). Flagged in wire-orchestrator + codex-install.
4. **claude-code routing through wire() (SR-07 caution).** `init` currently writes claude config inline (Steps 3–4). Routing through `wire()` must reuse the writers unchanged and produce byte-identical output; do NOT write claude config twice. Recommend `wire()` owns claude-code dispatch; `init` delegates. Golden-file before the refactor. Flagged in cli-routing.
5. **Standalone `wire` transport resolution.** `resolveWireTransport` must decide local-vs-cloud for a `wire` invocation in an already-inited repo. Recommend mirroring `init`'s local `resolveBinary()` path with cloud fallback; document so dry-run and real paths agree (NFR-10). Flagged in cli-routing.
6. **opencode `mcp` server schema.** Confirm opencode's exact `mcp.unimatrix` shape (`type`/`command` array) against the shipped reference before emitting, so retrieval RETURNS (AC-10) rather than being present-but-unspawnable. Flagged in opencode-retrieval.

None of these block Stage 3b; all are reconciliations with a recommended resolution stated in-line.

## Constraint honoring (spot-check)

- C-01..C-13, Modularity, WireLeg contract: honored per-file. Hook-client size gate never raised (hook-client-provider.md is explicit; addition is lean, reuses `mapToCanonical`/`KNOWN_PROVIDERS`, no new module).
- Non-clobbering / dry-run / idempotence specified for every writer (opencode, codex TOML, codex hooks, skills).
- `--provider codex-cli` mandatory with a fail-loud throw if absent (codex-install). `--force` definitions-only, never forwarded to `wire` (skills-installer + cli-routing).
- Verifier EXECUTES the manifest with per-AC mutation/negative controls (non-tautology, R-01/R-02).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search (pattern, decision/nan-023) — surfaced ADR-001..006 (#5763–5768), #5737 (provider≠source_domain / split-brain), #5743 (vnc-049 non-clobber sentinel), #5372/#4780 (hook-client size gate), #1195 (prefix-match settings merge). Applied to intent gate, provider hint, opencode/codex writers, size-gate handling.
- Deviations from established patterns: none. Design reuses the `mergeSettings`/`opencode-install.js` detect→parse-safe→additive→format-preserving→warn-and-skip→containment principle and the existing `KNOWN_PROVIDERS` hint contract (no normalizer arm added, per Q5). Read-only tier — nothing stored.
