# Gate 3b Report: nan-023

> Gate: 3b (Code Review)
> Date: 2026-09-17
> Feature: nan-023 — Per-Harness Idempotent Wiring, Non-Destructive Definition Install (Issue #990)
> Reviewed at: committed HEAD on `feature/nan-023` (f34e4aaf), diff vs `main`
> Result: **PASS** (2 tracked WARNs — modularity estimate + pre-existing install footprint; neither blocking)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | Functions/data structures/algorithms match per-component pseudocode + brief signatures. |
| 2. Architecture compliance | PASS | ADR-001..006 honored; component boundaries intact; no harness logic in init/orchestrator. |
| 3. Interface implementation | PASS | WireLeg contract + all function signatures match Integration Surface exactly. |
| 4. Test-case alignment | PASS | Component + C14 suites map to test plans; 294 component + 21 C14 tests pass. |
| 5. Code quality | PASS | No stubs/TODO/placeholder; all files < 1,000-line cap; suites green. |
| 6. Security | PASS | Containment guards, TOML escaping (R-10), token-free cloud entries, fail-safe parse. |
| 7. Knowledge stewardship | N/A here | Agent-report stewardship blocks are enforced at wave-return (process #986), not Gate 3b. |
| Load-bearing constraints | PASS | Size gate not raised; fail-loud provider; JS-client hook target; C14 executes manifest. |
| Modularity (PL-10) | WARN (non-blocking) | codex-install.js 621 lines vs <450 *estimate* — under the 1,000 cap; prompt pre-acknowledged. |
| Install footprint (290KB) | WARN (known debt) | Pre-existing RED on main; routed to human merge gate + 3c GH Issue. Not introduced blocking. |

## Detailed Findings

### 1. Pseudocode Fidelity — PASS
Every implemented function matches its pseudocode and the brief's Function Signatures table:
- `wire()`, `detectHarnesses()`, `resolveTransport()` in `lib/wire.js` — detect → select → dispatch → aggregate manifest, exactly per `wire-orchestrator.md`. Intent gate is self-gated in the writers (single site), orchestrator only forwards `--harness` — matches pseudocode's "pick ONE site" directive.
- `maybeWireCodex / writeCodexMcpToml / writeCodexHooks / upsertTomlTable / readTomlTable / tomlString` in `lib/codex-install.js` — algorithm, constants, and the 7-event `CODEX_EVENTS` set match `codex-install.md` and `toml-surgical.md` byte-for-byte (matchers reused from `EVENT_MATCHERS`; PreToolUse = cycle matcher). Malformed is checked BEFORE the intent gate as specified.
- `installSkills()` install-if-absent + `--force` in `lib/init.js`; `writeOpencodeMcp()` additive in `lib/opencode-install.js`; `parseHookArgs()` + hint path in `lib/hook-client/index.js`; `buildHookClientCommand(clientPath, event, providerHint?)` in `lib/merge-settings.js` — all match.

Intentional, documented refinement: `writeCodexHooks` returns `WireLeg[]` (one leg per event) rather than the brief's singular `WireLeg`. This is mandated by ARCHITECTURE + `codex-install.md` ("ONE WireLeg per event so the manifest carries every exact command string", SR-09/R-02). Consistent with ADR-006 manifest fidelity — not a departure.

### 2. Architecture Compliance — PASS
- ADR-001 (per-harness writers return structured WireLeg; orchestrator aggregates manifest): implemented; manifest is the single source of truth for verifier, dry-run, and summary.
- ADR-002 (in-house surgical TOML, zero dep, byte-preserving): `splitKeepEndings/scanTables/ownedRegion/upsertTomlTable` copy every byte outside the owned table verbatim; multi-line-string-aware boundary scanner; no new dependency.
- ADR-003 (codex hooks → JS hook client, `--provider` hint): `buildHookClientCommand(...codex-cli)` targets `hook-client/index.js`, never the binary; `parseHookArgs` wires the existing hint path; claude 2-arg calls byte-identical.
- ADR-004 (skills-only install-if-absent + `--force`): `installSkills` iterates only the shipped source tree; foreign files never touched; scope-boundary action line emitted.
- ADR-005 (`wire` verb, `--harness` intent, wire skips skills+DB): `routeWire` calls `wire()` only; unknown `--harness` → help, exit 2, no write.
- ADR-006 (C14 spine executes manifest; trust surfaced): verifier spawns `entry`/`command` verbatim; trust note attached to every codex leg.
- init<->wire require cycle correctly broken by lazy `require("./init.js")` inside claude dispatch.

### 3. Interface Implementation — PASS
WireLeg shape (harness/surface/action/path/command?/entry?/reason?) is produced consistently by all writers. Signatures, argument shapes, and reused helpers (`isWithinProject`, `readJsonSafe`, `detectIndent`, `EVENT_MATCHERS`, `isUnimatrixHook`, `computeProjectHash`) match the Integration Surface. Error handling follows the two-boundary posture: claude reused writers throw (backward compat), opencode/codex writers warn-and-skip.

### 4. Test-Case Alignment — PASS
- Component suites (`wire`, `codex-install`, `codex-toml-surgical`, `cli-routing`, `opencode-install`, `init`, `merge-settings`, `hook-client/index`): **294 pass / 0 fail**.
- C14 suites (`c14-claude`, `c14-codex`, `c14-opencode`, `c14-negative`): **21 pass / 0 fail**.
- No `.only` / `.skip` / focused tests; 0 skipped, 0 todo.
- Golden byte-identical claude test present and passing (`wire.test.js` — SR-07/C-10).

### 5. Code Quality — PASS
- No `TODO`/`FIXME`/`placeholder`/`unimplemented!`/`todo!` in any changed production file.
- Line counts (all < 1,000 cap): wire 295, codex-install 621, opencode-install 608, init 760, hook-client/index 498, merge-settings 428, bin/unimatrix 279.
- Size gate `check-hook-client-size.js` file itself UNCHANGED vs main; passes: stripped 105,243 ≤ 110,000, raw 191,416 ≤ 200,000.

### 6. Security — PASS
- **Containment**: every new writer guards `isWithinProject` (AC-12); path-escape → skipped, never followed. `installSkills` throws on `..` in a shipped filename.
- **TOML injection (R-10)**: `buildTomlEmit` routes binaryPath/bridgePath/projectHash through `tomlString` (TOML-basic escaping of quotes/backslashes/control chars). No naive concatenation into `command =`.
- **No secrets in config (Q1/Principle 8)**: cloud codex + opencode entries emit `command="node", args=[bridge, hash]`; `url=`/token never written. `test_verify_codex_cloud_entry_carries_no_token_or_url` asserts no url/token/mcp-url in the serialized entry.
- **Fail-safe deserialization**: `readTomlTable`/`scanTables` return `{malformed}` on ambiguous boundaries; `readJsonSafe` distinguishes malformed; never panics/corrupts — file preserved, leg skipped-malformed.
- **Fail-loud provider (C-04/AC-07)**: `requireProviderFlag` enforces `--provider codex-cli` on every codex hook command; missing → throw (converted to skipped-malformed, never a silent write without the flag).
- `cargo audit` — N/A: nan-023 is JS-only; no Rust/dependency changes.

### Load-Bearing Constraints — PASS (all verified)
| Constraint | Status | Evidence |
|---|---|---|
| Size gate NOT raised (R-09) | PASS | Gate file unchanged; passes with ~4.7KB stripped / ~8.6KB raw headroom. |
| `--provider codex-cli` fail-loud (C-04/AC-07) | PASS | `requireProviderFlag`; every command built with CODEX_PROVIDER. |
| Codex hooks target JS client not binary (C-03/AC-08) | PASS | `buildHookClientCommand` → `node <clientPath> ...`. |
| `--force` definitions-only, never to wire (C-12) | PASS | init threads force to `installSkills` only; `routeWire` `--force` is a noted no-op. |
| Non-clobber/idempotent/dry-run/containment (C-09/C-11) | PASS | Idempotence + dry-run tests green; unchanged short-circuits. |
| C14 verifier EXECUTES manifest, non-tautology (SR-09/R-02) | PASS | Spawns entry/command verbatim; mutation controls in `c14-negative` fail on broken artifact with intact manifest string, and pass. |
| TOML escaped via tomlString (R-10) | PASS | See Security. |
| Cloud codex never emits url= (Q1) | PASS | See Security. |
| Backward-compat claude golden byte-identical (SR-07/C-10) | PASS | Golden test passes; claude writers reused unchanged. |

## WARNs (non-blocking, tracked)

1. **Modularity estimate — codex-install.js 621 lines** vs the brief's `<450` *estimate*. Under the governing PL-10 cap of 1,000 code lines (and the spawn prompt pre-acknowledges "~621 — under cap"). NEW module, not an over-cap grow. Non-blocking WARN; no rework required.
2. **Install footprint `test_remote_install_under_290kb`** is RED (312,005 > 290,000 on main; branch total ~359,908). Pre-existing tech debt, not introduced by nan-023's ~47KB of legitimate wiring code. Per spawn instructions this is a governance/gate-raise decision (precedent #775: 250→290KB) routed to the human merge gate and to Stage 3c for a GH Issue + triage. Assessed as tracked known-debt, NOT a Gate 3b blocker. No nan-023 file is gratuitously heavy or carries removable weight.

## Rework Required
None.

## Knowledge Stewardship
- Stored: nothing novel to store — Gate 3b findings are feature-specific and belong in this glass-box report; no cross-feature recurring failure pattern surfaced (all checks passed on first review).
