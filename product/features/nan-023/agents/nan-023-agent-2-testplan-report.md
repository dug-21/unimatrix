# Agent Report — nan-023 Stage 3a Test Plan Design (uni-tester)

Agent ID: nan-023-agent-2-testplan · Phase: Test Plan Design (Stage 3a)

## Deliverables (all under `product/features/nan-023/test-plan/`)

| File | Component |
|------|-----------|
| `OVERVIEW.md` | Strategy, Gate-0 outcome, risk→AC map, feasibility+firing plans, integration harness plan, pre-tag exercise |
| `wire-orchestrator.md` | `lib/wire.js` — detection, manifest contract, intent-gate dispatch |
| `skills-installer.md` | `lib/init.js installSkills` — install-if-absent, `--force` skills-only |
| `opencode-retrieval.md` | `writeOpencodeMcp` — vnc-049 sentinel preservation |
| `codex-install.md` | `lib/codex-install.js` — TOML MCP + hooks writers |
| `toml-surgical.md` | `upsertTomlTable`/`readTomlTable` — foreign-byte preservation |
| `hook-client-provider.md` | `parseHookArgs` + `buildHookClientCommand` + size gate |
| `cli-routing.md` | `bin/unimatrix.js` — verbs, intent gate, dry-run, containment |
| `c14-verifier.md` | The load-bearing manifest-executing verifier |

Component plans map 1:1 to the IMPLEMENTATION-BRIEF Component Map.

## Gate-0 Trust-Feasibility Determination

**Seeding a trusted, codex-firing `.codex/` home in cloud CI is NOT FEASIBLE today.** Evidence
(repo-confirmed): codex-cli is absent from `package.json` (zero-dep), `.github/workflows/ci.yml` (no
install step), and all Dockerfiles; the `.codex/` trust mechanism is undocumented in-repo (ADR-006 §3 /
ARCH Q2 deferred it to the tester).

**Consequence** — the critical distinction keeps most codex ACs HARD: the C14 hook-fire discharge
executes the exact wired manifest command (`node <clientPath> <EVENT> --provider codex-cli`) with a
synthetic stdin event, **bypassing codex**; the cloud return entry `command="node", args=[bridge, hash]`
mirrors the installed, tested token-free bridge.

- codex-LOCAL firing (AC-09 local): **HARD**
- codex-LOCAL + CLOUD retrieval return (AC-10): **HARD** (bridge is codex-independent)
- codex-CLOUD wired-command firing (AC-09 cloud, command-level): **HARD**
- codex **self-firing** the 7 events in a trusted `.codex/` (Q4 evidence column): **DOCUMENTED CONDITIONAL** — recorded per-event as `wired-inactive (untestable-in-CI)`, never silent-skipped.
- **C14 codex leg claim: proven(local) / partial(cloud)** — partial scope narrowed to the single "codex itself raises the event" column.

Escalation (non-blocking): to upgrade Q4 self-firing to hard, a follow-up must add a codex-cli install
step + confirm the trust-seed mechanism (likely global `~/.codex/config.toml` `[projects."<abs>"]
trust_level="trusted"` under overridden HOME/CODEX_HOME). Pre-tag real-server exercise covers the
codex-independent path off-tag meanwhile.

Dependency flag: `index.js` does not yet parse `--provider`; AC-08/AC-09 `provider=codex-cli` stamping
depends on landing ADR-003's `parseHookArgs` in Stage 3b (already planned, Q5).

## Risk → AC Coverage

All R-01..R-16 mapped to component plans (see OVERVIEW §3). Critical (R-01/02/03) → c14-verifier
(execute-manifest return+fire, mutation controls, pre-tag exercise). High (R-04..R-09) → toml-surgical,
codex-install, hook-client-provider, opencode-retrieval, c14-verifier goldens. Non-tautology enforced:
every behavioral AC (AC-05/09/10) carries a mutation control that fails on a broken artifact with an
intact manifest string.

## Integration Suite Plan

- **infra-001 (Rust MCP harness):** nan-023 touches zero Rust server code. `pytest -m smoke` = MANDATORY
  minimum gate (health baseline for the C14 return path). `tools`/`lifecycle` = regression baseline for
  the retrieval slug; failures there are pre-existing → GH Issue + `xfail`, not fixed in this PR.
  `cargo test --workspace` and workspace-link-smoke are N/A (no Rust change) — stated explicitly.
- **New JS integration (the real surface):** the C14 verifier in `packages/unimatrix/test/` (`node
  --test`) driving real/stub MCP servers (`real-server.js`/`mcp-stub-server.js`) and the spawned hook
  client with synthetic stdin; size gate + parity corpus #4751; backward-compat goldens captured
  pre-refactor.

## Open Questions

1. Trust-seed mechanism is an unconfirmed external assumption; validate if/when codex is added to CI.
2. Confirm `parseHookArgs` lands in Stage 3b before c14-verifier fire assertions run.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search (decision/nan-023) — found ADR entries #5763/#5767/#5768, per-harness config-surface pattern #5761/#5762, and gate-3b test-plan-fidelity lessons (#3548, #4202, #4328, #2758). Applied the "test plan names a specific string but implementation isn't driven" lessons by requiring every behavioral AC to execute the manifest, not string-match.
- Stored: entry #5769 "Testing pattern — decouple execute-the-wired-command from external-harness-fires-the-event (Gate-0 trust-feasibility)" via context_store (pattern/testing). Novel refinement of the ceremonial-wiring discipline specific to trust/tool-feasibility scoping.
