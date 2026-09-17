# nan-023 Implementation Brief — Per-Harness Idempotent Wiring, Non-Destructive Definition Install

> Feature: nan-023 · Issue: #990 · Capability: **C14 (multi-LLM harness parity)** · Goal: `personal-cloud` (#4946)
> Session 1 design complete. This brief compiles SCOPE, SPECIFICATION, ARCHITECTURE (ADR-001..006), RISK-TEST-STRATEGY, and ALIGNMENT-REPORT into an implementation-ready deliverable for Session 2.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-023/SCOPE.md |
| Scope Risk Assessment | product/features/nan-023/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/nan-023/specification/SPECIFICATION.md |
| Architecture | product/features/nan-023/architecture/ARCHITECTURE.md |
| ADR-001 Per-harness wiring layer | product/features/nan-023/architecture/ADR-001-per-harness-wiring-layer.md |
| ADR-002 Codex TOML surgical writer | product/features/nan-023/architecture/ADR-002-codex-toml-surgical-writer.md |
| ADR-003 Codex hooks JS-client + provider | product/features/nan-023/architecture/ADR-003-codex-hooks-js-client-provider.md |
| ADR-004 Definition install-if-absent | product/features/nan-023/architecture/ADR-004-definition-install-if-absent.md |
| ADR-005 Wire verb + harness intent | product/features/nan-023/architecture/ADR-005-wire-verb-harness-intent.md |
| ADR-006 C14 verification spine | product/features/nan-023/architecture/ADR-006-c14-verification-spine.md |
| Risk / Test Strategy | product/features/nan-023/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-023/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/nan-023/ACCEPTANCE-MAP.md |

## Goal

Give the Unimatrix installation package (JS client / dogfood mechanism) a non-destructive definition-install policy and a per-harness, idempotent **wiring** path so claude-code, opencode, and codex-cli each reach C14 multi-LLM harness parity in a **consumer** repo. Parity means the package writes each harness's MCP/retrieval and hook config non-destructively AND that, after wiring, a `context_*` retrieval call against the wired slug **returns** and wired hooks **fire** — never merely that config is present. gemini-cli is deferred.

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| Wire orchestrator (`lib/wire.js`) | pseudocode/wire-orchestrator.md | test-plan/wire-orchestrator.md |
| Skills installer (`lib/init.js` `installSkills`) | pseudocode/skills-installer.md | test-plan/skills-installer.md |
| opencode retrieval writer (`lib/opencode-install.js` `writeOpencodeMcp`) | pseudocode/opencode-retrieval.md | test-plan/opencode-retrieval.md |
| Codex installer (`lib/codex-install.js`) | pseudocode/codex-install.md | test-plan/codex-install.md |
| Surgical TOML helper (`upsertTomlTable`/`readTomlTable`) | pseudocode/toml-surgical.md | test-plan/toml-surgical.md |
| hook-client `--provider` argv hint (`lib/hook-client/index.js`, `normalize.js`) | pseudocode/hook-client-provider.md | test-plan/hook-client-provider.md |
| CLI routing (`bin/unimatrix.js`) | pseudocode/cli-routing.md | test-plan/cli-routing.md |
| C14 verifier (`test/`) | pseudocode/c14-verifier.md | test-plan/c14-verifier.md |

### Cross-Cutting Artifacts (produced in Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Component Map and Cross-Cutting Artifact paths above are confirmed against the files produced in Stage 3a (pseudocode/ and test-plan/ both carry one file per Component Map component plus OVERVIEW.md). Gate-0 (codex trust-feasibility) outcome recorded in test-plan/OVERVIEW.md: cloud trust-seeding NOT feasible in CI → codex-cloud self-firing (Q4 evidence column) is a documented conditional; local firing, command-level firing, and retrieval-return stay HARD; C14 codex leg claimed proven(local)/partial(cloud), never silent-skipped.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Wiring architecture | Per-harness wiring layer: `lib/wire.js` orchestrator + one non-clobbering writer per (harness × surface); each writer returns a structured `WireLeg`; the aggregated manifest is the single source of truth for verifier and dry-run | SCOPE Proposed Approach | architecture/ADR-001-per-harness-wiring-layer.md |
| Codex TOML writer | Minimal in-house **surgical block writer** (manages only the owned `[mcp_servers.unimatrix]` table), not a round-trip library; byte-for-byte foreign preservation by construction; zero new dependency | SCOPE Open Q "TOML writer" | architecture/ADR-002-codex-toml-surgical-writer.md |
| Codex cloud MCP transport | **Mirror the token-free stdio bridge.** Cloud `.codex/config.toml` `[mcp_servers.unimatrix]` uses `command="node", args=[bridgePath, projectHash]` — a verbatim mirror of the cloud claude-code entry (`init.js:257`); the bridge resolves the credential from the project hash, so no secret enters the TOML. **Do NOT use `url=`** (a bearer token in TOML violates Principle 8, "no secrets in config"). AC-10 codex-cloud return path targets this shape. | **DECIDED (Q1)** — ARCHITECTURE Open Q1; R-03 | architecture/ADR-006-c14-verification-spine.md |
| Codex hook target + attribution | Hooks target the **JS hook client** (`lib/hook-client/index.js`), not the binary; new `--provider <name>` argv **hint** parsed in `index.js`; `--provider codex-cli` on every command | SCOPE Decided §4 | architecture/ADR-003-codex-hooks-js-client-provider.md |
| Codex `--provider` wiring gap | **The provider arm exists in both normalizers — confirmed.** `normalize.js:23` and `hook.rs:35` both list `codex-cli` in `KNOWN_PROVIDERS` with hint-path-vs-inference logic (`hook.rs:186` validates the hint against the allowlist). The ONLY gap: `index.js` does not yet parse `--provider` argv, so it can't carry `codex-cli` into the existing hint path (silently infers claude-code). Delivery task: **wire `parseHookArgs` into the existing hint path in `index.js`, guarded by parity corpus #4751 so `normalize.js` and `hook.rs` stay in step.** No normalizer arm to add. | **DECIDED (Q5)** — R-05, #5737 | architecture/ADR-003-codex-hooks-js-client-provider.md |
| Codex emitted event set | **7-event set adopted (Q4):** SessionStart, UserPromptSubmit, PreToolUse (`^context_cycle$\|^mcp__unimatrix__context_cycle$`), PostToolUse (`*`), PreCompact, SubagentStart (`*`), Stop. Excludes PostToolUseFailure, SubagentStop. Mirroring Claude's event names does NOT prove Codex fires all seven — **AC-09 must RECORD per-event firing evidence** (which of the 7 reach the JS hook client); any that don't → wired-inactive, documented gap, not assumed-firing. | **DECIDED (Q4)** — SPEC Open Q2; R-15 | architecture/ADR-003-codex-hooks-js-client-provider.md |
| CI trust for `.codex/` (Gate-0) | **Trust-feasibility is a Gate-0 precondition the tester confirms BEFORE delivery starts** (not mid-delivery — avoids the SR-03 tag-tax). The test fixture may seed its own trusted codex home (test infra, not consumer wiring) — likely feasible. Feasible → AC-09/AC-10 codex-cloud stays a HARD AC; not feasible → codex-cloud becomes an explicitly DOCUMENTED conditional, codex-LOCAL stays hard, C14 is claimed proven(local)/partial(cloud), never silent-skipped. Enforces ADR-006 §3's surfaced-precondition as policy. | **DECIDED (Q2)** — ADR-006 §3; R-06; SR-02 | architecture/ADR-006-c14-verification-spine.md |
| Definition install policy | install-if-absent default; `--force` overwrites Unimatrix-owned **skills** only; foreign files never touched; `--force` never re-asserts wiring | SCOPE Decided §3/§8, Goal 4 | architecture/ADR-004-definition-install-if-absent.md |
| Command surface + intent | Distinct `unimatrix wire` verb; `--harness <name>` routing + new-entry opt-in signal; #960 intent gate (`skipped-intent` when a new user-owned-config entry lacks explicit opt-in) | SCOPE Decided §1/§2 | architecture/ADR-005-wire-verb-harness-intent.md |
| C14 verification spine | Verifier **executes** the `WireLeg` manifest's exact command/entry (retrieval-returns + hook-fires), local AND cloud where feasible; trust surfaced as precondition; pre-tag real-server exercise | SCOPE Constraints "C14 return path" | architecture/ADR-006-c14-verification-spine.md |

## Delivery Sequence

All 5 design-phase open questions are DECIDED (see Resolved Decisions). Delivery proceeds in this order:

- **Gate-0 (BEFORE delivery starts) — Codex trust-feasibility check (Q2).** The tester confirms whether cloud CI can seed a trusted `.codex/` home for a fixture (test infra, not consumer wiring). This runs up front to avoid the SR-03 tag-tax of discovering trust infeasibility mid-delivery. Decision rule (record the outcome in RISK-COVERAGE-REPORT):
  - **Feasible in cloud CI** → AC-09/AC-10 codex-cloud stays a HARD AC.
  - **Not feasible** → codex-cloud firing becomes an explicitly DOCUMENTED conditional; codex-LOCAL firing stays hard; C14 is claimed proven(local)/partial(cloud) — **never silent-skipped**. Enforces ADR-006 §3's surfaced-precondition as policy.
- **Stage 3a** — pseudocode + test plan (populate Component Map + Cross-Cutting Artifacts). Test strategy must include the per-AC feasibility matrix (see ACCEPTANCE-MAP) and per-event codex firing evidence plan (Q4).
- **Stage 3b** — implement. The reframed **index.js task (Q5)**: wire `parseHookArgs` into the existing hint path in `index.js` (the provider arm already exists in both `normalize.js:23` and `hook.rs:35`); guard with parity corpus #4751 so the JS and Rust twins stay in step. Size-gated — never raise the hook-client gate.
- **Stage 3c** — C14 verifier executes the `WireLeg` manifest (retrieval-returns + hook-fires) local AND cloud per the Gate-0 outcome and the feasibility matrix; records per-event firing evidence for AC-09; pre-tag real-server exercise.

## Files to Create / Modify

| File | Change | Summary |
|------|--------|---------|
| `packages/unimatrix/lib/wire.js` | **new** (<300 lines) | Orchestrator: `detectHarnesses`, intent gate (#960), dispatch per-harness writers, aggregate `WireLeg[]` manifest. |
| `packages/unimatrix/lib/codex-install.js` | **new** (<450 lines) | `maybeWireCodex`, `writeCodexMcpToml`, `writeCodexHooks`, internal `upsertTomlTable`/`readTomlTable`. |
| `packages/unimatrix/lib/init.js` | modify (thin, ~+60) | `copySkills` → `installSkills` (install-if-absent + `--force`); call wire layer; no harness logic inlined. Stays < 800 lines. |
| `packages/unimatrix/lib/opencode-install.js` | modify (~+70) | Add `writeOpencodeMcp` — additive `mcp.unimatrix`, preserves Ollama `provider` + foreign keys byte-for-byte. |
| `packages/unimatrix/lib/hook-client/index.js` | modify (lean) | `parseHookArgs(argv) -> {event, providerHint}`; hint path stamps provider. **Size-gated — never raise the gate.** |
| `packages/unimatrix/lib/merge-settings.js` | modify (~+3) | `buildHookClientCommand` gains optional 3rd `providerHint` arg appending `" --provider <hint>"`. |
| `packages/unimatrix/bin/unimatrix.js` | modify | Route `wire` verb, `--harness <name>`, `--dry-run`; wire skips skills + DB; `--force` gates `installSkills` only, never forwarded to `wire`. |
| `packages/unimatrix/test/` | **new** | C14 verifier (executes manifest); golden files (claude-code common path); opencode sentinel; TOML/provider/size-gate/intent/dry-run tests. |

## Data Structures

`WireLeg` — the manifest element; the contract that defeats path-divergence (SR-09):

```
WireLeg = {
  harness:  "claude-code" | "opencode" | "codex-cli",
  surface:  "mcp" | "retrieval" | "hooks" | "plugin",
  action:   "created" | "updated" | "unchanged" | "skipped-undetected"
          | "skipped-malformed" | "skipped-intent",
  path:     string,             // absolute file written
  command?: string,             // hooks: EXACT command string written
  entry?:   object,             // mcp/retrieval: exact entry written
  reason?:  string              // for any "skipped-*"
}
```

Codex config shapes written:

```toml
# .codex/config.toml (local) — surgical upsert of only this owned table
[mcp_servers.unimatrix]
command = "<absolute binaryPath>"

# cloud variant (Q1 DECIDED) — verbatim mirror of the cloud claude entry (init.js:257);
# the bridge resolves the credential from the project hash, so no secret is in the file.
# [mcp_servers.unimatrix]
# command = "node"
# args = ["<absolute mcp-bridge.js>", "<projectHash>"]
# NEVER url = "<mcpUrl>" — a bearer token in TOML violates Principle 8 (no secrets in config).
```

```json
// .codex/hooks.json — same matcher-group shape as .claude/settings.json
{ "hooks": { "PreToolUse": [ { "matcher": "^context_cycle$|^mcp__unimatrix__context_cycle$",
  "hooks": [ { "type": "command",
    "command": "node <abs>/lib/hook-client/index.js PreToolUse --provider codex-cli" } ] } ] } }
```

## Function Signatures

### New (this feature)

| Signature | File |
|-----------|------|
| `wire(projectRoot, {harness?, clientPath, binaryPath?, mcp?:{url,token}, dryRun}) -> {actions:string[], manifest:WireLeg[]}` | `lib/wire.js` |
| `detectHarnesses(dir) -> {"claude-code":bool,"opencode":bool,"codex-cli":bool}` | `lib/wire.js` |
| `installSkills(projectRoot, {force:boolean, dryRun:boolean}) -> string[]` | `lib/init.js` |
| `writeOpencodeMcp(dir, {binaryPath?, url?}, dryRun) -> WireLeg` | `lib/opencode-install.js` |
| `maybeWireCodex(dir, {clientPath, binaryPath?, url?, dryRun}) -> WireLeg[]` | `lib/codex-install.js` |
| `writeCodexMcpToml(dir, {binaryPath?, url?}, dryRun) -> WireLeg` | `lib/codex-install.js` |
| `writeCodexHooks(dir, {clientPath, dryRun}) -> WireLeg` | `lib/codex-install.js` |
| `upsertTomlTable(raw:string, tablePath:string, body:string[]) -> {text:string, changed:boolean}` | `lib/codex-install.js` (internal) |
| `readTomlTable(raw:string, tablePath:string) -> {present:boolean, body:string}` | `lib/codex-install.js` (internal) |
| `buildHookClientCommand(clientPath, event, providerHint?) -> string` | `lib/merge-settings.js` (3rd arg added) |
| `parseHookArgs(argv:string[]) -> {event:string, providerHint:(string\|null)}` | `lib/hook-client/index.js` |

### Existing (reuse — do not reinvent)

| Signature | Source |
|-----------|--------|
| `detectProjectRoot(startDir) -> string` | `lib/init.js` |
| `writeMcpJson(projectRoot, binaryPath, dryRun) -> string[]` | `lib/init.js` |
| `mergeSettings(filePath, commandSource, {dryRun}) -> {actions, content}` | `lib/merge-settings.js` |
| `isUnimatrixHook(hookEntry) -> boolean` / `UNIMATRIX_PATTERNS` (pattern 5) | `lib/merge-settings.js` |
| `maybeProvisionOpenCode(dir, {dryRun}) -> string[]` | `lib/opencode-install.js` |
| `isWithinProject(dir, target) -> boolean` / `readJsonSafe` / `detectIndent` | `lib/opencode-install.js` |
| `resolveBinary() -> string` | `lib/resolve-binary.js` |
| `KNOWN_PROVIDERS = ["claude-code","gemini-cli","codex-cli","opencode"]` | `lib/hook-client/normalize.js` |

## Constraints

- **C-01 Project-scoped only** — root via existing `.git` walk (`detectProjectRoot`); never write global/user config.
- **C-02 New TOML reader/writer** — first non-JSON writer; must be format-preserving (foreign keys/comments/ordering); in-house surgical writer (ADR-002).
- **C-03 Codex hooks target the JS hook client** — `node <clientPath> <EVENT>`, never the `unimatrix` binary; asserted against the written command string.
- **C-04 `--provider codex-cli` mandatory** on every Codex hook command (vnc-013 ADR-006); missing = fail-loud defect.
- **C-05 vnc-049 retrieval sentinel** — `mcp.unimatrix` + Ollama `provider` block in `opencode.json` survive byte-for-byte; retrieval-still-returns regression.
- **C-06 C14 return path is load-bearing** — a leg is not "wired" unless a `context_*` retrieval returns after wiring; config presence is insufficient.
- **C-07 Local AND cloud deployment** — install path (Codex hook wiring, retrieval return, stdio-vs-HTTP transport) must work under both; ACs assert both where feasible.
- **C-08 Codex trust-gating** — `.codex/config.toml`/hooks load only for a trusted `.codex/` layer; trust surfaced as precondition, never a silent no-op.
- **C-09 Non-clobbering / fail-safe** — every wire leg warns-and-skips on malformed input, never throws; mirrors `opencode-install.js`. init's loud claude-path throw preserved (backward compat).
- **C-10 Backward compatibility** — claude-code `.mcp.json` + `.claude/settings.json` common-path output byte-identical after per-harness refactor; golden-file before refactor.
- **C-11 Idempotence + dry-run + containment** apply to every new writer.
- **C-12 `--force` blast radius** — definitions-only (skills); wiring always-additive; `--force` writes zero wiring changes; never forwarded to `wire`.
- **C-13 Definition scope = skills only** — protocols/agents excluded; boundary stated in help/output.
- **Modularity** — hook-client size gate is the single tight resource (110 KB stripped PRIMARY / 200 KB raw backstop; ~10 KB raw headroom). `--provider` parse must be lean; **never raise the gate** (#5372, lesson #4780). No file exceeds 1,000 lines.

## Dependencies

- **JS installer package** — `lib/init.js` (`copySkills` :130, hook wiring :502–518), `bin/unimatrix.js`, `merge-settings.js`, `opencode-install.js`, `resolve-binary.js`.
- **JS hook client** — `lib/hook-client/index.js` (hook target; only `parseHookArgs` is missing, Q5); `normalize.js:23` mirrors Rust `hook.rs:35` — both already carry the `codex-cli` `KNOWN_PROVIDERS` arm (confirmed, Q5); keep the twins in lockstep via parity corpus #4751.
- **TOML** — no new dependency; in-house surgical writer (ADR-002).
- **vnc-049 / ADR-006 (Unimatrix #5743)** — non-clobbering OpenCode branch + C17 retrieval regression sentinel.
- **#960** — explicit-intent install model.
- **#16732 (upstream, FIXED)** — Codex MCP-tool-call hooks fire; assumed fixed. If regressed, AC-09 unmeetable (escalate).
- **vnc-013 ADR-006** — `--provider codex-cli` attribution requirement.
- **Deployment** — local (Rust binary / UDS) and cloud (JS bridge / HTTP) Unimatrix server.

## NOT in Scope

- gemini-cli parity — deferred; hand-authored reference config stands.
- Definition install beyond skills — protocols (`.claude/protocols/`) and agents (`.claude/agents/`) NOT brought under install-if-absent/`--force`.
- Hash-tracked / 3-way-merge definition updates — rejected.
- Writing to any global/user-level config (`~/.codex/`, `~/.claude/`, `~/.config/`, `~/.gemini/`).
- Changing the dogfood definition source-of-truth flow (definitions flow into the package via `/uni-release`).
- New observation/retrieval capability per harness (C18/vnc-049) — this feature only *wires* existing surfaces.
- Fixing this repo's own local harness config — scope is the package's output into consumer repos.
- Deeper per-provider `source_domain` resolution — ingress forces `DEFAULT_HOOK_SOURCE_DOMAIN="claude-code"` (#5737); AC-09 firing asserts `provider=codex-cli`, not `source_domain`.
- `--force` re-asserting wiring.

## Alignment Status

**Vision PASS** (5 PASS, 1 WARN, 0 VARIANCE, 0 FAIL). No variance requires human approval. The feature directly advances `personal-cloud`; the three parity legs match the goal's multi-LLM set exactly; gemini deferral mirrors the goal's own deprioritization; JS-client stance honors architectural principles 5/6/8.

**WARN RESOLVED — the cloud/codex parity questions the WARN flagged are now DECIDED.** The WARN concerned two design-phase unknowns behind AC-09/AC-10 cloud arms: (a) codex cloud MCP transport, and (b) whether CI can mark a fixture `.codex/` trusted. Both are decided: (a) transport **mirrors the token-free stdio bridge** — `command="node", args=[bridge, hash]`, never `url=` (Q1); (b) trust-feasibility is a **Gate-0 precondition** the tester confirms before delivery, with an explicit decision rule that keeps codex-cloud hard-if-feasible / documented-conditional-if-not — never silent-skipped (Q2). The WARN's own recommendation is adopted: the **per-AC cloud-vs-local feasibility matrix is now REQUIRED** in ACCEPTANCE-MAP (Q3), carries per-event codex firing evidence (Q4), and the pre-tag real-server exercise (ADR-006) stands. This closes the ceremonial-wiring / never-green-on-tag exposure (R-01/R-03).

**Non-blocking follow-ups (uni-zero's, NOT delivery blockers — do not gate delivery on these):**
- C14 `done_when` #5729 wording — a capability-accounting refinement owned by uni-zero.
- ALIGNMENT "curve" vs corpus "threshold" terminology — a capability-accounting reconciliation owned by uni-zero.

## Design-Phase Open Questions — ALL RESOLVED (human-approved)

The 5 questions previously carried into delivery are DECIDED and baked into the Resolved Decisions table, Delivery Sequence, and ACCEPTANCE-MAP. Recorded here for provenance:

1. **Codex cloud MCP transport shape — DECIDED (Q1): mirror the token-free stdio bridge.** `command="node", args=[bridgePath, projectHash]` (verbatim mirror of the cloud claude entry, `init.js:257`); the bridge resolves the credential from the hash, so no secret is in the file. **Never `url=`** (bearer token in TOML violates Principle 8). AC-10 codex-cloud return path targets this shape. (ARCHITECTURE Open Q1; R-03.)
2. **CI trust for `.codex/` — DECIDED (Q2): Gate-0 precondition, escalate never silent-skip.** Tester confirms trust-feasibility BEFORE delivery starts (avoids SR-03 tag-tax); the fixture may seed its own trusted codex home. Feasible → codex-cloud stays hard; not feasible → documented conditional (local stays hard, C14 proven(local)/partial(cloud)). Enforces ADR-006 §3. (ARCHITECTURE Open Q2; R-06; SR-02.)
3. **Per-AC feasibility matrix — DECIDED (Q3): REQUIRED.** ACCEPTANCE-MAP now carries an explicit AC × harness × deployment (local/cloud) matrix; every cell is a stated fact (required / conditional-on-trust / infeasible+reason). "Where feasible" is now an enumerated, auditable claim — no silent drop of any cloud assertion. (Vision WARN recommendation; SPEC Open Q3.)
4. **Codex lifecycle event set — DECIDED (Q4): 7-event set adopted, with per-event firing evidence.** Emit the 7 events; mirroring Claude's names does NOT prove Codex fires all seven — AC-09 RECORDS which reach the JS hook client; non-firing events → wired-inactive, documented gap, folded into the Q3 matrix per-event. (SPEC Open Q2; R-15.)
5. **codex-cli provider arm — DECIDED (Q5): confirmed present in both normalizers; the work is `index.js`.** `normalize.js:23` and `hook.rs:35` both list `codex-cli` in `KNOWN_PROVIDERS` with hint-path-vs-inference logic (`hook.rs:186` validates the hint). No normalizer arm to add. Delivery task reframed: wire `parseHookArgs` into the existing hint path in `index.js`, guarded by parity corpus #4751. (R-05; #5737.)
