# Test Plan Overview — nan-023 (Per-Harness Idempotent Wiring + Non-Destructive Definition Install)

> Feature: nan-023 · Issue #990 · Capability C14 (multi-LLM harness parity) · Goal `personal-cloud` (#4946)
> Rooted in RISK-TEST-STRATEGY R-01..R-16 and ACCEPTANCE-MAP AC-01..AC-16. This is a **JS package** feature (`packages/unimatrix/**`).

## 1. Test Strategy

Three tiers, all in `packages/unimatrix/test/` under the existing `node --test` runner (assertions via
`node:assert`; temp `.git` project fixtures via `fs.mkdtempSync`; test infra is **cumulative** — extend
`init.test.js` / `opencode-install.test.js` / `merge-settings.test.js`, do not scaffold in isolation):

| Tier | What | Where |
|------|------|-------|
| **Unit / component** | Pure functions + per-`(harness×surface)` writers, driven against on-disk fixtures; byte-for-byte and WireLeg-contract assertions | `toml-surgical`, `skills-installer`, `opencode-retrieval`, `codex-install`, `hook-client-provider`, `wire-orchestrator` plans |
| **Feature / CLI** | The real `init`/`wire` verbs end-to-end (the behavioral-outcome lens — a writer unit test does NOT discharge a command-level outcome) | `cli-routing` plan |
| **C14 behavioral (load-bearing)** | The verifier **executes** the `WireLeg` manifest: retrieval-**returns** + hook-**fires**, local+cloud, with mutation/negative controls; pre-tag real-server exercise | `c14-verifier` plan |

**Non-tautology discipline (SR-09/R-02):** the C14 verifier executes `manifest[].command` / connects
via `manifest[].entry` — never string-matches or reconstructs paths. Every behavioral AC (AC-05, AC-09,
AC-10) carries a **mutation control** that fails when the wired artifact is broken but its manifest
string is intact. This is the primary defense against the #917/#918/#930 "green test, holed capability"
family: the floor tests drive the assembled path (real MCP target, real hook-client spawn, synthetic
stdin), not a hand-constructed proxy.

## 2. Gate-0 — Codex Trust-Feasibility Determination (RUN BEFORE DELIVERY)

**DETERMINATION: seeding a *trusted, codex-firing* `.codex/` home in cloud CI is NOT FEASIBLE today.**
Evidence (repo-confirmed):

- **codex-cli is absent from the test environment** — no codex dependency in `packages/unimatrix/package.json` (zero-dep by design), no install step in `.github/workflows/ci.yml` (JS job runs only `npm run test:hook-client`, explicit "client has zero runtime deps"), and no Dockerfile installs codex. Repo-wide search for `@openai/codex` / "install codex" → no hits.
- **The `.codex/` trust-granting mechanism is undocumented in-repo** — SCOPE/ADR-006 §3/ARCH Q2 treat trust only as an abstract precondition and explicitly defer confirmation to the tester. The one concrete codex artifact (`/.codex/hooks.json`, vnc-013 reference) carries no trust field and there is no `.codex/config.toml` anywhere. A seed approach (global `~/.codex/config.toml` `[projects."<abs>"] trust_level="trusted"` under an overridden `HOME`/`CODEX_HOME` — the harness already overrides HOME) is an **external assumption, not repo-confirmed**.

**Critical distinction that keeps most codex ACs HARD (ADR-006 §1, ADR-003 §1, ACCEPTANCE-MAP AC-09 method):**
the C14 hook-fire discharge is to **execute the exact wired manifest command** (`node <clientPath>
<EVENT> --provider codex-cli`) with a synthetic event on stdin, asserting ingress at the JS hook client
— this **bypasses codex entirely**. The harness already spawns the client this exact way
(`test/hook-client/index-decoration.test.js`). Likewise the codex **cloud return** entry `command="node",
args=[bridge, hash]` mirrors the installed, tested token-free claude-code bridge (`init.js:257`) and is
codex-independent.

**Consequence (per the brief's decision rule — never silent-skipped):**

| Assertion | Status | Rationale |
|-----------|--------|-----------|
| codex-**LOCAL** hook firing (AC-09 local) | **HARD** | Manifest-command execution; no codex, no trust needed |
| codex-**LOCAL** retrieval return (AC-10 local) | **HARD** | `command="<binaryPath>"` against local server; fixture seeds local trust |
| codex-**CLOUD** retrieval return (AC-10 cloud) | **HARD** | `command="node", args=[bridge, hash]` bridge is installed/tested, codex-independent |
| codex-**CLOUD** hook firing via wired command (AC-09 cloud, command-level) | **HARD** | Drive the wired command against a cloud-backed client — codex-independent |
| codex **self-firing** the 7 events in a trusted `.codex/` (Q4 "codex-raises-it") | **DOCUMENTED CONDITIONAL** | Requires codex installed + confirmed trust — CI lacks both. Recorded per-event as `wired-inactive (untestable-in-CI: codex not installed / trust unconfirmed)`, NOT skipped |

**C14 claim for the codex leg: proven(local) / partial(cloud)** — the partial(cloud) scope is narrowed
to the single "codex itself raises the event" evidence column; the wired-command fire and both
retrieval-return arms are proven in both deployments.

**Escalation note (not a nan-023 blocker):** to upgrade codex self-firing from documented-conditional
to hard, a follow-up must (1) add a codex-cli install step to the test env and (2) confirm the
trust-seed mechanism. The pre-tag real-server exercise (§6) exercises the codex-independent
manifest-command path locally + cloud, so layered failures still surface off-tag.

**Dependency flag:** `lib/hook-client/index.js` does not yet parse `--provider` (infers `claude-code`).
AC-08/AC-09 `provider=codex-cli` stamping depends on landing ADR-003's `parseHookArgs` in Stage 3b —
already the plan (Q5).

## 3. Risk → AC → Component Mapping

| Risk | Priority | AC(s) | Test component |
|------|----------|-------|----------------|
| R-01 Ceremonial wiring | Critical | AC-09, AC-10, AC-05 | c14-verifier (return+fire from manifest) |
| R-02 Tautological verifier | Critical | AC-09, AC-10 | c14-verifier (mutation controls) |
| R-03 Cloud never-green-on-tag | Critical | AC-09/10 cloud | c14-verifier (pre-tag real-server exercise, feasibility matrix) |
| R-04 TOML foreign preservation | High | AC-06, AC-04 | toml-surgical, codex-install |
| R-05 Provider mislabel / split-brain | High | AC-07, AC-09 | hook-client-provider (static+parity), c14-verifier (behavioral) |
| R-06 Trust unmeetable / inert | High | AC-09, AC-10 | Gate-0 (§2), codex-install, c14-verifier (untrusted surface) |
| R-07 claude backward-compat | High | AC-03, AC-10 claude | c14-verifier (golden files) |
| R-08 vnc-049 opencode sentinel | High | AC-05 | opencode-retrieval, c14-verifier (return) |
| R-09 hook-client size gate | High | (cross-cut) | hook-client-provider (dual-limit, never raise) |
| R-10 Command injection | Med | (cross-cut AC-06/07) | toml-surgical, codex-install, hook-client-provider |
| R-11 warn-and-skip masks unwired | Med | AC-15, AC-13 | wire-orchestrator, all writers (skip-reason distinctness) |
| R-12 Intent-gate bypass | Med | AC-11, AC-16 | cli-routing, wire-orchestrator, opencode-retrieval |
| R-13 `--force` blast radius | Med | AC-02 | skills-installer (skills side), cli-routing (zero-wiring side) |
| R-14 dry-run / containment | Med | AC-12, AC-14 | cli-routing, wire-orchestrator, each writer |
| R-15 Event-set mismatch | Med | AC-09 (Q4) | codex-install (7-event set), c14-verifier (per-event evidence) |
| R-16 install-if-absent first-run | Med | AC-01 | skills-installer |

Every high-priority risk has ≥1 concrete test expectation in its component plan. No AC is discharged
by a config-presence proxy.

## 4. Per-AC Cloud-vs-Local Feasibility Matrix Plan (Q3)

The C14 verifier RECORDS each behavioral cell (AC-05/AC-08/AC-09/AC-10 × harness × local/cloud) into
RISK-COVERAGE-REPORT as a stated fact: **required / conditional-on-trust / infeasible(reason) /
n/a(reason)** — matching ACCEPTANCE-MAP's matrix. Per Gate-0 (§2), the only non-`required` cells are the
codex **self-firing** Q4 evidence columns (documented-conditional in cloud). No cloud assertion is
silently dropped.

## 5. Per-Event Codex Firing Evidence Plan (Q4 — 7-event table)

`c14-verifier` parametrizes over the 7 emitted events (SessionStart, UserPromptSubmit, PreToolUse,
PostToolUse, PreCompact, SubagentStart, Stop). For each: drive the wired command with a synthetic event,
RECORD `fires` (ingress observed) vs `wired-inactive`. The "wired command fires the JS-client path"
result is hard local+cloud; the separate "codex itself raises this event" column is
documented-conditional per Gate-0. Any `wired-inactive` is a **named gap** in the report, never a silent
drop (R-15).

## 6. Integration Harness Plan

**Two distinct integration concerns — do not conflate:**

**(a) infra-001 Rust MCP harness** — nan-023 touches **zero** Rust server code (it wires config into
consumer repos). Therefore no server-tool suite is *triggered by the feature*. The infra-001 role here
is a **health baseline for the C14 return path**: the verifier's `context_*` retrieval must return
against a real server, so:
- **`pytest -m smoke` — MANDATORY minimum gate** (confirms the MCP server the return-path connects to is healthy: store/get roundtrip, search, briefing returns).
- **`tools`, `lifecycle`** — run as regression baseline for the retrieval slug the C14 return path exercises (store/retrieval behavior per the suite-selection table). Not expected to change; a failure here is pre-existing → triage per USAGE-PROTOCOL (file GH Issue + `xfail`, do NOT fix in this PR).
- Full-workspace LINK smoke (`check-workspace-link-smoke.sh`) and `cargo test --workspace` are **not applicable** — no Rust change. (State this explicitly in the report rather than skipping silently.)

**(b) New JS integration tests (the C14 verifier)** — the feature's real integration surface, in
`packages/unimatrix/test/` under `node --test`, driving:
- Real/stub MCP servers via `test/helpers/real-server.js` / `mcp-stub-server.js` / `stub-server.js` for the **retrieval-return** assertions (claude/opencode/codex, local + cloud bridge).
- The JS hook client spawned with synthetic stdin (`spawn(process.execPath, [ENTRY, EVENT, "--provider", "codex-cli"], {env})`) for the **hook-fire** assertions.
- The hook-client size gate (`test/check-hook-client-size.js`) and parity corpus runner (`npm run test:hook-client:layer2`, corpus #4751) for R-09 / R-05.
- Backward-compat goldens captured into `test/fixtures/` **before** the per-harness refactor (SR-07).

New integration/C14-verifier tests to add (Stage 3b/3c): all tests enumerated in `c14-verifier.md`
(return + fire per leg, mutation controls, 7-event evidence, golden files, feasibility recording) plus
the codex writer + wire orchestrator suites. These validate MCP-visible behavior no writer unit test
can reach.

## 7. Pre-Tag Real-Server Exercise (SR-03 / R-03)

`test_pretag_real_server_exercise` runs codex hook-fire + retrieval-return + opencode/claude return
against a **real** local+cloud server (`real-server.js`) **before** the release chain — not a
release-only gate. This is the primary R-03 mitigation: it surfaces the layered cloud/codex failures
during delivery instead of consuming one tag round each (#5267, nan-019/nan-020). Release-only
local+cloud gates remain but are not the first signal.

## 8. Cross-Component Test Dependencies

- `wire-orchestrator` proves the **WireLeg manifest contract** that `c14-verifier` consumes — verifier tests assume every dispatch path (incl. all `skipped-*`) yields a well-formed leg.
- `toml-surgical` (pure functions) underpins `codex-install`'s file-level TOML assertions.
- `hook-client-provider` proves the **static** `--provider codex-cli` on every command; `c14-verifier` proves the **behavioral** ingested `provider` value. Split across both by design (static vs executed).
- `skills-installer` proves `--force` skills-only; `cli-routing` proves the zero-wiring-change half — AC-02 is only fully discharged across both.
- Goldens (SR-07) must be captured **before** the per-harness refactor lands, or the byte-identical assertion is meaningless.

## 9. Open Questions

1. **Trust-seed mechanism confirmation** — the global `~/.codex/config.toml` `[projects."<abs>"]
   trust_level="trusted"` + overridden `HOME`/`CODEX_HOME` approach is an assumption; if codex is later
   installed in CI, validate it before upgrading Q4 self-firing to hard. (Non-blocking; documented per §2.)
2. **`parseHookArgs` landing order** — AC-08/AC-09 `provider=codex-cli` stamping requires ADR-003's
   `index.js` change; confirm it lands in Stage 3b before the c14-verifier fire assertions run.
