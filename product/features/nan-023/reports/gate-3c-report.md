# Gate 3c Report: nan-023

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-09-17
> Feature: nan-023 — Per-Harness Idempotent Wiring, Non-Destructive Definition Install (Issue #990) · Capability C14
> Validated at: committed HEAD on `feature/nan-023` (c9d6767e), diff vs `main`
> Result: **PASS** (2 WARNs — pre-existing footprint gate GH#994; CI-invocation gap for nan-023 top-level suites / Node-24 bare-runner caveat. Neither blocking.)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Behavioral-outcome proof (scope lens) | PASS | Every entry-point outcome maps to a test that drives `init`/`wire` and EXECUTES the WireLeg manifest. Not discharged by config presence. |
| 2. Risk mitigation proof (R-01..R-16) | PASS | RISK-COVERAGE-REPORT maps each risk → AC → executed test → PASS. No high/critical risk lacks a concrete executed test. |
| 3. Test coverage completeness | PASS | All Phase-2 risk→scenario mappings exercised; integration smoke run; per-event codex firing recorded; feasibility matrix stated. |
| 4. Specification compliance | PASS | AC-01..16 verified; FR/NFR discharged; behavioral ACs (AC-05/08/09/10) asserted from wired command path. |
| 5. Architecture compliance | PASS | ADR-001..006 honored; manifest-executing C14 spine confirmed non-tautological by re-run; zero Rust drift. |
| 6. Knowledge stewardship | PASS | Tester report carries `## Knowledge Stewardship` with `Queried:` + `Stored:` (#5778). |
| WARN A — footprint gate | WARN | `test_remote_install_under_290kb` pre-existing RED on main (312005>290000); GH#994 filed; skip marker references it. Human gate-raise-vs-trim. |
| WARN B — CI invocation / runner caveat | WARN | CI serial runner covers only `test/hook-client/`; nan-023 top-level suites (incl. C14 spine) run at local Gate 3c only. Bare `node --test` unreliable on Node 24. CI follow-up warranted. |
| NOTE — codex self-firing | Documented conditional (honored) | Cloud codex SELF-raising the 7 events = `wired-inactive (untestable-in-CI)` per Gate-0; command-level firing + both retrieval arms are HARD/PASS. Not a coverage hole. |

## Independent Verification Performed

Not a report re-read — the load-bearing claims were re-executed and cross-checked:

- **C14 verifier re-run** (`c14-{claude,codex,opencode,negative}.test.js`, serial): **21 pass / 0 fail / 0 skip**. Read `c14-verifier.js` — it spawns the MCP target from `leg.entry` VERBATIM, runs a real JSON-RPC handshake, and observes ingress at a real UDS socket (local) / HTTP stub (cloud). It never string-matches a command or reconstructs a path. The negative controls in `c14-negative.test.js` break the wired target (`entry.command="/nonexistent/…"`, blanked target, broken client path) while asserting the manifest string is still present, then assert the verifier FAILS. **Non-tautology confirmed by construction and by passing mutation controls** (SR-09/R-02).
- **Component suites re-run** (wire, codex-install, codex-toml-surgical, cli-routing, opencode-install, init-integration, serial): **135 pass / 0 fail / 0 skip**.
- **GH#994**: exists, OPEN, documents exact footprint numbers + precedent #775. `remote-client.test.js:493` skip reason string references GH#994.
- **Diff scope**: entire branch diff is under `packages/unimatrix/**` + `product/features/nan-023/**`. **Zero Rust/crate/Cargo changes** — validates the "smoke is a confirmatory health baseline, not feature-triggered" disposition.
- **No integration tests deleted or commented** (no `D` entries for test files; no Python/`suites/` changes). The 3 `copySkills` tests were **updated** to ADR-004 install-if-absent wording (`Installed skill file:` / `Kept skill file (exists)` / `[dry-run] Would install skill file:`) — coverage preserved/expanded, not removed.

## Detailed Findings

### 1. Behavioral-outcome proof (scope behavioral lens) — PASS
Every entry point in SCOPE-RISK-ASSESSMENT's User-Facing Entry Points table maps to a test that drives the user's actual command (`init`/`wire`) and, for behavioral rows, executes the manifest:
- `init` fresh / re-run edited skill / `--force` → init + init-integration suites (install-if-absent, keep-existing, force-overwrite arms).
- `wire` / `wire` twice → wire.test.js (multi-harness, idempotence, dry-run action-set equality).
- `wire` opencode → c14-opencode: sentinel byte-preserved AND `context_*` RETURNS (local + token-free cloud bridge).
- `wire --harness codex-cli` → c14-codex builds the manifest via the real `wire()` orchestrator, then EXECUTES it: TOML foreign-preserved, hook command targets `hook-client/index.js` (not the binary) with `--provider codex-cli`, retrieval returns, hook fires. cli-routing.test.js separately proves argv→`wire()` dispatch, closing the CLI seam.
No behavioral row is discharged by a seam-beneath or presence proxy. The C14 mutation controls are the affirmative proof (a presence check would pass where these fail).

### 2. Risk mitigation proof — PASS
R-01..R-16 each carry ≥1 executed PASS test in RISK-COVERAGE-REPORT's Risk→AC→Test table. Dominant risks:
- **R-01/R-02 (ceremonial / tautological wiring)**: C14 return+fire executed from the manifest per leg; 4 retrieval mutation controls + hook mutation control + tautology guard all fire. Verified by re-run.
- **R-03 (cloud never-green-on-tag)**: pre-tag real-server exercise ran the wired retrieval legs against the real `target/release/unimatrix` binary (local) — claude+codex+opencode all returned non-empty; cloud codex proven via token-free stdio bridge (no url/token leak). Cloud full-server arm recorded as environment-infeasible off-tag, not silently dropped.
- **R-04/R-08 (TOML + opencode sentinel preservation)**: byte-diff foreign-region suites PASS.
- **R-05 (provider mislabel/split-brain)**: static `--provider codex-cli` on every command + behavioral `provider="codex-cli"` (asserts on `provider`, not `source_domain`); Q5 confirmed no normalizer arm to add.
- **R-09 (size gate)**: stripped 105243/110000, raw 191416/200000 — gate not raised.

### 3. Test coverage completeness — PASS
- Integration **smoke (`pytest -m smoke`) = 37 pass / 0 fail** (MANDATORY minimum gate) — tester-reported; consistent with zero-Rust delta (server behavior definitionally identical to main). tools+lifecycle run as confirmatory non-gating baseline (stated, not silently skipped); `cargo test --workspace` + LINK smoke marked **N/A (zero Rust)** explicitly.
- RISK-COVERAGE-REPORT includes integration test counts AND the AC-09 per-event codex firing table (7 events × local/cloud) AND the per-AC cloud-vs-local feasibility matrix.
- Gate-0 disposition honored: cloud codex self-firing = documented conditional (`wired-inactive`, untestable-in-CI, per-event), never silent-skip; codex LOCAL firing, command-level firing (both deployments, all 7 events), and both retrieval-return arms are HARD and PASS.

### 4. Specification compliance — PASS
AC-01..AC-16 all verified with stated methods. Behavioral ACs (AC-05/08/09/10) asserted from the wired command path via the WireLeg manifest — the SR-09 ceremonial-wiring guard is satisfied (confirmed by executing the verifier and its mutation controls). No AC discharged by a tautology or config-presence proxy.

### 5. Architecture compliance — PASS
ADR-001 (manifest-aggregating orchestrator), ADR-002 (in-house surgical TOML), ADR-003 (codex hooks → JS client, `--provider` hint), ADR-004 (skills-only install-if-absent + `--force`), ADR-005 (`wire` verb + intent gate), ADR-006 (manifest-executing C14 spine, pre-tag exercise) all reflected in code and tests. Zero Rust drift. Component boundaries intact (wire.js orchestrator, codex-install.js, no harness logic in init).

### 6. Knowledge stewardship — PASS
`nan-023-agent-4-tester-report.md` carries `## Knowledge Stewardship` with `Queried:` (context_briefing: #5327, #4781, #4473/#4177, #5768, #5777) and `Stored:` (#5778 — bare `node --test` recursive-discovery + shared-skills race pattern). Compliant.

## WARNs (non-blocking, tracked)

### WARN A — pre-existing footprint gate (GH#994)
`test_remote_install_under_290kb` is RED on `main` (baseline 312005 > 290000). nan-023 adds ~+47903 of legitimate per-harness wiring (branch 359908). Filed **GH#994** (OPEN; exact numbers + precedent #775 for the 250→290KB raise). Test marked `{ skip: "Pre-existing: GH#994 …" }`, reason string references the issue. This is a **human gate-raise-vs-trim decision surfaced, not a masked feature bug** — carried to the merge gate. Not a Gate 3c blocker.

### WARN B — CI-invocation gap for nan-023 top-level suites (+ Node-24 bare-runner caveat)
The CI job (`.github/workflows/ci.yml` → `npm run test:hook-client`) globs only `test/hook-client/*.test.js`. The new nan-023 suites live at `test/` top level — **wire, codex-install, codex-toml-surgical, cli-routing, opencode-install, init-integration, and the load-bearing C14 verifier spine (`c14-*`) are not executed by any CI job.** They pass at local Gate 3c (verified here by re-run), but there is no automated CI regression guard for the feature's core logic.

Compounding: the tester's documented Node-24 caveat — a bare recursive `node --test` (the `npm test` script) produces 11 false failures + 1 cancel (a stdin fixture recursively discovered as a test; parallel files racing on the shared real `skills/` dir → ENOENT). CI does NOT use bare `node --test`; it uses the serial `test:hook-client` runner, so the CI job that DOES run is green. But that means neither existing CI invocation is a viable home for the top-level suites: `test:hook-client` doesn't select them, and bare `node --test` is unreliable.

**Assessment:** CI uses the serial runner and is green for what it runs — but a **CI-invocation follow-up is warranted**. Recommended: add a CI job (or extend the runner) that executes the nan-023 top-level suites via an explicit serial `*.test.js` list (the portable idiom `run-hook-client.js` already uses), and/or land the tester's non-blocking fixture fixes (`require.main === module` guard on `stdio-mcp-fixture.js`; isolate skills-mutating tests to temp dirs) so a bare `node --test` becomes CI-usable. Non-blocking for this gate — behavior is proven at the protocol-mandated Gate 3c execution point, and the CI-scope limitation is largely pre-existing (CI only ever ran the hook-client subdir).

## Documented Conditional (honored, not a gap)
Cloud codex SELF-raising the 7 lifecycle events in a real trusted `.codex/` is recorded per-event as `wired-inactive (untestable-in-CI: codex not installed / trust unconfirmed)`. Per Gate-0 this is an enumerated, asserted-present named gap (`test_codex_self_firing_documented_conditional`), never silent-skipped. Command-level firing (wired command reaches the JS hook client) for all 7 events, and both retrieval-return arms, are HARD and PASS in local + cloud. C14 codex-leg claim honestly narrowed to **proven(local) / partial(cloud)**, where partial = only the "codex itself raises it" column. This is a stated boundary of what CI can prove, not a nan-023 wiring coverage hole.

## Rework Required
None.

## Scope Concerns
None. Scope, technology, and architecture support all requirements; the two WARNs are governance/CI-hygiene follow-ups, not scope failures.
