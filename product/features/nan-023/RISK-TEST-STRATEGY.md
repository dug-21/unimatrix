# Risk-Based Test Strategy: nan-023

> Feature: nan-023 (Issue #990) · Capability: C14 (multi-LLM harness parity) · Goal: `personal-cloud` (#4946)
> Inputs: SCOPE.md, architecture/ARCHITECTURE.md, ADR-001..006, specification/SPECIFICATION.md, SCOPE-RISK-ASSESSMENT.md (SR-01..SR-12)
> Historical evidence: Unimatrix #5267, #4473, #4177, #4311, #5737, #5748, #5743, #5377, #5372, #4780, #5378, #5192, #5768.

This strategy identifies risks specific to the designed system: `lib/wire.js` orchestrator + `WireLeg` manifest, per-harness writers, the in-house surgical TOML writer (ADR-002), the `--provider` argv hint (ADR-003), and the manifest-driven C14 verifier (ADR-006). The dominant risk is ceremonial wiring (SR-09): the entire design's value rests on the verifier executing the *exact written command from the manifest* rather than checking config presence.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Ceremonial wiring — config/hook is written (manifest shows `created`) but a `context_*` call never returns and/or the wired hook never fires; a real consumer gets an inert leg (SR-09) | High | High | **Critical** |
| R-02 | Manifest verifier is tautological — asserts on `WireLeg.command`/`entry` strings or reconstructs paths instead of *executing* the written command; passes authoring, defeats ADR-006's non-tautology guarantee (#4177) | High | Med | **Critical** |
| R-03 | Cloud return/fire gate is release-only and never-green-on-a-tag; codex cloud MCP transport (Open Q1) unresolved blocks AC-10 cloud path (SR-03, #5267) | High | High | **Critical** |
| R-04 | Surgical TOML writer (`upsertTomlTable`) drops/reorders foreign `[mcp_servers.*]` tables, comments, or key ordering on read-modify-write (SR-01) | High | Med | High |
| R-05 | Codex event mislabeled `claude-code` — `--provider codex-cli` omitted on a command, OR `normalize.js` lacks a codex arm its Rust `hook.rs` twin has (split-brain), OR test asserts `source_domain=codex` which ingress forces to `claude-code` (SR-10, #5737, #5748) | High | Med | High |
| R-06 | Codex trust-gating makes the leg green-in-CI / inert-for-consumer; CI harness cannot mark `.codex/` trusted, making AC-09/10 unmeetable (SR-02, Open Q2/Q4) | High | Med | High |
| R-07 | Backward-compat regression — claude-code `.mcp.json` / `.claude/settings.json` output changes when writers are pulled behind `wire.js` (SR-07) | High | Med | High |
| R-08 | vnc-049 sentinel regression — `writeOpencodeMcp` disturbs the Ollama `provider` block, existing `mcp.unimatrix`, or foreign keys (SR-06, #5743) | High | Med | High |
| R-09 | hook-client size gate trips — `--provider` argv parse pushes `hook-client/index.js` past the gate; ~10KB raw headroom (200KB backstop) but the PRIMARY limit is 110KB stripped, and the meta-assertion in `size-gate.test.js` must move in lockstep (#5377, #5372, #4780, #5378) | Med | Med | High |
| R-10 | Command-string injection into written config — `binaryPath`/`clientPath` interpolated into `.codex/config.toml` `command=` and hook `command` strings; a path with shell/TOML metacharacters corrupts or escapes the emitted command | Med | Low | Med |
| R-11 | warn-and-skip masks a never-wired leg — a swallowed warning reads as success; `skipped-*` action not surfaced/asserted (SR-08, #4473) | Med | Med | Med |
| R-12 | Intent-gate bypass — a *new* `mcp.unimatrix`/`[mcp_servers.unimatrix]` written into user-owned config without `--harness`/opt-in; silent install (#960, SR-11) | Med | Med | Med |
| R-13 | `--force` blast radius — `--force` re-asserts or overwrites wiring, or touches foreign skill files (SR-04, AC-02) | Med | Med | Med |
| R-14 | dry-run divergence / containment bypass — dry-run computes a different action set than the real path or a writer escapes the resolved root (SR-12, AC-12/14) | Med | Low | Med |
| R-15 | Codex hook event-set mismatch — the emitted lifecycle events (Open Q2) don't match events codex actually fires; hooks are wired but no configured event ever triggers → ceremonial by omission | Med | Med | Med |
| R-16 | install-if-absent first-run break — existence check mis-detects a fresh repo as "already installed" and skips skill install, or clobbers a modified skill (AC-01, NFR-06) | Med | Low | Med |

## Risk-to-Scenario Mapping

### R-01: Ceremonial wiring (dominant — SR-09)
**Severity**: High · **Likelihood**: High · **Impact**: Feature ships green while every codex consumer (and any harness) has retrieval that never returns and hooks that never fire — the vnc-047/#944 class the closed loop cannot self-catch.

**Test Scenarios**:
1. For each detected harness, after `wire`, issue a `context_*` retrieval **against the wired slug from the wired command path** and assert a non-empty RETURN (AC-10). Presence of the config entry does NOT discharge this.
2. In a trusted `.codex/` env, trigger the wired codex hook event and assert it ARRIVES at the JS hook client ingress (AC-09) — observed ingress, not config parse.
3. Negative control: delete/blank the wired entry, re-run the verifier, assert it FAILS. If it still passes, the assertion is a presence proxy — reject.

**Coverage Requirement**: Every C14 leg (claude-code, codex-cli, opencode-retrieval) has a return assertion executed from the `WireLeg` manifest; codex + claude have a fire assertion; opencode fire is plugin-level. No leg's acceptance rests on config presence.

### R-02: Tautological / path-reconstructing verifier (SR-09 enabler, #4177)
**Severity**: High · **Likelihood**: Med · **Impact**: ADR-006's manifest spine is defeated — the verifier "passes" by inspecting the manifest it was handed rather than executing it, so path-divergence survives.

**Test Scenarios**:
1. Verifier consumes `WireLeg.command`/`path` and **executes** that exact string; assert the child process/hook ingress reacts — not that the string contains `hook-client`.
2. Mutation test: point a `WireLeg.command` at a non-existent client path; verifier must FAIL, proving it runs the command rather than string-matching it.
3. Assert the verifier reconstructs no paths of its own (code review + a fixture where reconstructed path ≠ manifest path; only the manifest path is exercised).

**Coverage Requirement**: At least one mutation/negative case per behavioral AC (AC-09, AC-10) that fails when the wired artifact is broken but its manifest string is intact.

### R-03: Cloud never-green-on-tag + codex cloud transport unresolved (SR-03, #5267)
**Severity**: High · **Likelihood**: High · **Impact**: AC-09/AC-10 cloud arms first execute only on a release tag and fail in sequence (one tag round each, per nan-019/nan-020 #5267); Open Q1 (codex `url=` vs `command=node <bridge>`) is unresolved, so the codex cloud return path may have no defined transport.

**Test Scenarios**:
1. **Pre-tag real-server exercise** (ADR-006): run codex hook-fire + retrieval-return AND opencode/claude return against a real local+cloud server *before* the release chain — surface layered failures off-tag.
2. Cloud codex retrieval scenario pinned to the resolved transport (Open Q1): assert the emitted TOML entry (`command`+`args` bridge shape, per architect recommendation) produces a returning `context_*` call in cloud.
3. Per-AC feasibility matrix test: record which of AC-09/AC-10 run green in cloud vs local-only, so "where feasible" is a stated, asserted fact not a silent gap (Open Q3).

**Coverage Requirement**: A pre-tag gate exercises every local+cloud behavioral assertion; cloud codex path is not asserted until Open Q1 transport is decided (tracked as a gap below).

### R-04: TOML surgical writer foreign-key/comment/order preservation (SR-01)
**Severity**: High · **Likelihood**: Med · **Impact**: The first non-JSON writer; a naive upsert reorders or drops foreign `[mcp_servers.*]` tables, inline comments, or key order — silently mangling a consumer's codex config.

**Test Scenarios**:
1. Fixture `.codex/config.toml` with foreign tables, interleaved comments, and non-alphabetical key order; run wire; byte-diff every foreign region = empty; `[mcp_servers.unimatrix]` present (AC-06).
2. Idempotence: run twice, byte-identical output (AC-04, NFR-01).
3. Round-trip on a table adjacent to `[mcp_servers.unimatrix]` (comment immediately above/below the owned table) — a common off-by-one for block writers.
4. Update-in-place: existing `[mcp_servers.unimatrix]` with a stale value is upserted without disturbing surrounding bytes.

**Coverage Requirement**: Foreign-byte preservation is a first-class assertion (NFR-08), not best-effort; covers comments, ordering, and adjacency.

### R-05: Provider mislabel / normalize.js split-brain (SR-10, #5737, #5748)
**Severity**: High · **Likelihood**: Med · **Impact**: Codex events silently attributed to `claude-code`; C14 parity claim is corrupt data. Two failure paths: (a) `--provider codex-cli` missing on a written command; (b) `normalize.js` and Rust `hook.rs` diverge on the codex arm.

**Test Scenarios**:
1. String-assert `--provider codex-cli` present on EVERY written codex hook command; a missing flag is a fail-loud defect, not a warning (AC-07, NFR-07).
2. Behavioral: fire a wired codex hook, assert the ingested event carries `provider="codex-cli"` (not `claude-code`) — assert on **`provider`**, NOT `source_domain` (ingress forces `source_domain="claude-code"` per #5748/architecture; asserting source_domain will false-fail).
3. Parity-corpus check: if a codex provider arm is added to `normalize.js`, assert the Rust `hook.rs` twin agrees (split-brain guard, #5737). If nan-023 does not touch the normalizer, assert existing `KNOWN_PROVIDERS` codex arm already round-trips the hint.

**Coverage Requirement**: Provider attribution asserted both statically (flag on command) and behaviorally (ingested `provider` value); source_domain explicitly excluded from the assertion.

### R-06: Trust-gating unmeetable / inert-for-consumer (SR-02, Open Q2/Q4)
**Severity**: High · **Likelihood**: Med · **Impact**: AC-09/10 require a trusted `.codex/` layer; if CI cannot mark it trusted, firing is untestable and the parity claim is conditional and unverified.

**Test Scenarios**:
1. Confirm the local+cloud CI harness can mark a fixture `.codex/` trusted; if not, this is an escalation (see gaps), not a silent skip.
2. Untrusted-env scenario: assert the leg surfaces the trust precondition (warn/fail-loud) and never presents an inert no-op as success (NFR-09).
3. Assert the trust-precondition surface (Open Q4) is emitted to the consumer (concrete warn text / exit behavior).

**Coverage Requirement**: Trust is a surfaced precondition with an asserted warning path; the firing assertion runs in a confirmed-trusted fixture or the AC is escalated as unmeetable.

### R-07: claude-code backward-compat regression (SR-07)
**Severity**: High · **Likelihood**: Med
**Test Scenarios**:
1. Golden-file `.mcp.json` + `.claude/settings.json` from a fixture BEFORE the per-harness refactor; assert byte-identical after `writeMcpJson`/`mergeSettings` are called through `wire.js` (NFR-06).
2. Fresh-repo `init` still produces the full claude-code common path.
**Coverage Requirement**: Golden files captured pre-refactor; common-path diff = empty.

### R-08: vnc-049 opencode sentinel regression (SR-06, #5743)
**Severity**: High · **Likelihood**: Med
**Test Scenarios**:
1. opencode fixture with `mcp.unimatrix`, Ollama `provider` block, foreign keys; run wire; byte-diff sentinel regions = empty (AC-05).
2. After wiring, a `context_*` retrieval against the opencode slug RETURNS (retrieval-still-returns regression).
3. Fresh opencode (no `mcp.unimatrix`): additive write creates it; indent preserved via `detectIndent`.
**Coverage Requirement**: Byte-for-byte sentinel + return regression, mirroring #5743.

### R-09: hook-client size gate (#5377, #5372, #4780, #5378)
**Severity**: Med · **Likelihood**: Med
**Test Scenarios**:
1. After adding `--provider` argv parse, run `test/check-hook-client-size.js`: assert stripped ≤110KB (PRIMARY) and raw ≤200KB (BACKSTOP). Budget against the **stripped** total, not just the ~10KB raw headroom the architecture flags.
2. If cap constants change, assert the `size-gate.test.js` meta-assertion moved in lockstep (#5378). Never minify or raise the gate (#4780).
**Coverage Requirement**: Both limits asserted; addition is lean code + trimmed comment prose only.

### R-10: Command-string injection into written config
**Severity**: Med · **Likelihood**: Low · **Impact**: `binaryPath`/`clientPath` (from `resolveBinary`, `.git`-walk root) are interpolated into `command=` TOML values and hook `command` strings; a path containing spaces, quotes, or TOML/shell metacharacters corrupts the emitted config or changes the executed command.
**Test Scenarios**:
1. Fixture with a project root path containing a space and a quote; assert the emitted `.codex/config.toml` `command`/`args` and `hooks.json` command are correctly quoted/escaped and parse back to the intended argv.
2. Assert paths are emitted as TOML string values (or `args[]` array form per Open Q1 recommendation) that survive re-read, not naive concatenation.
**Coverage Requirement**: Path-with-metacharacters fixture per written surface (TOML `command`, hooks `command`).

### R-11: warn-and-skip masks never-wired leg (SR-08, #4473)
**Severity**: Med · **Likelihood**: Med
**Test Scenarios**:
1. Malformed config per harness → `WireLeg.action="skipped-malformed"` is surfaced (visible in output), input byte-preserved, no throw, no partial write (AC-15).
2. Assert `skipped-malformed` and `skipped-undetected` are DISTINCT, reported outcomes — a malformed skip is never presented as success (#4473).
**Coverage Requirement**: Each skip reason asserted as a first-class visible manifest outcome.

### R-12: Intent-gate bypass (#960, SR-11)
**Severity**: Med · **Likelihood**: Med
**Test Scenarios**:
1. No `--harness`: assert NO new `mcp.unimatrix`/`[mcp_servers.unimatrix]` written into a config lacking it → `action="skipped-intent"` with help line (AC-11).
2. `--harness <x>`: assert the new entry IS written.
3. Additive merge into an existing surface proceeds without opt-in.
4. Ambiguous/unknown invocation prints help, never defaults to install (AC-16).
**Coverage Requirement**: New-entry gate asserted on both arms; additive-merge exemption asserted; ambiguity → help.

### R-13: `--force` blast radius (SR-04, AC-02)
**Severity**: Med · **Likelihood**: Med
**Test Scenarios**:
1. Modified skill + foreign file + pre-existing wiring; run `--force`; assert skill == shipped, foreign byte-identical, and every wiring surface byte-identical to pre-run (zero wiring changes).
**Coverage Requirement**: `--force` proven definitions-only; wiring diff = empty.

### R-14: dry-run divergence / containment bypass (SR-12, AC-12/14)
**Severity**: Med · **Likelihood**: Low
**Test Scenarios**:
1. `--dry-run` on each verb: `[dry-run]`-prefixed output, zero filesystem changes, and action set equals the real-path action set (NFR-10).
2. Sandbox run asserts zero writes to `~/.codex/`, `~/.claude/`, `~/.gemini/`, `~/.config/` (AC-12).
3. A writer handed a target escaping root → `skipped`, never followed (`isWithinProject`).
**Coverage Requirement**: dry-run action-set equality + global-path write-absence + containment guard.

### R-15: Codex hook event-set mismatch (Open Q2)
**Severity**: Med · **Likelihood**: Med · **Impact**: Hooks wired for events codex never fires → ceremonial by omission (a subclass of R-01 the firing test only catches if it triggers a *real* codex event).
**Test Scenarios**:
1. The AC-09 fire test triggers an actual codex lifecycle event that is in the emitted set; assert ingress. If no emitted event corresponds to a real codex trigger, escalate.
**Coverage Requirement**: The emitted event set (Open Q2) is validated against events codex actually raises, via the fire test.

### R-16: install-if-absent first-run break (AC-01, NFR-06)
**Severity**: Med · **Likelihood**: Low
**Test Scenarios**:
1. Fresh repo `init`: all Unimatrix skills installed.
2. Re-run with a modified skill on disk: skill survives byte-for-byte (AC-01).
**Coverage Requirement**: First-run install + re-run non-clobber both asserted.

## Behavioral-Outcome Coverage (from the scope behavioral lens)

Every entry point in SCOPE-RISK-ASSESSMENT's behavioral lens gets a REQUIRED scenario that drives the user's actual command and asserts the observed outcome — never a seam beneath it.

| Outcome (from scope lens) | Entry point | Required scenario (drives the real command, asserts the outcome) |
|---|---|---|
| Skills installed; claude-code `.mcp.json`+`settings.json` present | `unimatrix init` (fresh) | Run `init` on a bare fixture; assert skills on disk + both claude configs present and byte-match golden (R-07, R-16) |
| Edited skill survives byte-for-byte | `unimatrix init` (re-run) | Modify an installed skill; run `init`; byte-diff = empty (AC-01, R-16) |
| Skills → shipped; foreign untouched; wiring NOT re-asserted | `unimatrix init --force` | Run `--force`; skill==shipped, foreign byte-identical, wiring diff = empty (AC-02, R-13) |
| MCP+hooks+retrieval ensured for detected harnesses; zero definition writes | `unimatrix wire` | Multi-harness fixture; run `wire`; wiring surfaces change, skill/protocol/agent trees byte-identical (AC-03) |
| Second run byte-identical | `unimatrix wire` (twice) | Byte-diff each surface run1↔run2 = empty (AC-04, R-04) |
| `mcp.unimatrix` ensured; sentinel preserved; retrieval still returns | `unimatrix wire` (opencode) | Sentinel byte-diff empty AND `context_*` returns against opencode slug (AC-05/10, R-08) |
| `[mcp_servers.unimatrix]` in TOML; codex hooks → JS client w/ `--provider codex-cli`; `context_*` returns AND hook fires | `unimatrix wire --harness codex-cli` | Drive the command; assert TOML foreign-preserved (R-04), hook command targets `hook-client` not binary + carries flag (R-05), then EXECUTE manifest: retrieval returns + hook fires in trusted env, local+cloud where feasible (R-01/02/03/06) |
| Intended actions printed `[dry-run]`; nothing written | `unimatrix wire --dry-run` | Run; `[dry-run]` output, zero writes, action set == real path (AC-14, R-14) |

A test that proves the outcome one layer BENEATH the command (a writer unit test, or a `WireLeg.command` string assertion) does NOT discharge these rows — the scenario must drive `init`/`wire` and, for behavioral rows, execute the manifest.

## Integration Risks

- **wire.js → per-harness writers → WireLeg aggregation**: a writer that returns a malformed/absent `WireLeg` breaks the verifier's input contract; assert every dispatch path yields a well-formed `WireLeg` (including all `skipped-*` actions).
- **wire.js → verifier (ADR-006)**: the manifest is the sole contract that defeats path-divergence; if the verifier reads anything other than the manifest to locate what to execute, R-02 materializes.
- **hook-client `--provider` argv → normalize.js hint → Rust `hook.rs` attribution**: split-brain across the JS/Rust normalizer twins (#5737); `source_domain` forced to `claude-code` at ingress (#5748) — the boundary where R-05 lives.
- **init.js ↔ wire.js**: `init` calls `installSkills` then `wire`; assert `wire` performs no definition/DB work when reached via `init` OR standalone (AC-03), and that init's loud claude-path throw is preserved while wire legs never throw (AC-15).
- **resolveBinary/`.git`-walk root → command interpolation**: paths flow into written command strings (R-10) and containment guard (R-14); assumed identical local vs cloud (SR-12) — assert root resolution parity.

## Edge Cases

- Foreign TOML comment immediately adjacent to `[mcp_servers.unimatrix]`; non-alphabetical TOML key order; CRLF vs LF line endings in a foreign config.
- Existing `mcp.unimatrix`/`[mcp_servers.unimatrix]` with a stale value (upsert, not append).
- Detected marker present but config file absent/empty; config present but marker dir missing.
- Project root path containing spaces/quotes/unicode (R-10).
- Concurrent-ish re-run (idempotence) and `--dry-run` immediately followed by real run producing the same action set.
- Untrusted `.codex/` layer (R-06); cloud with no local binary (Open Q1).

## Security Risks

Untrusted input this feature accepts: **existing consumer config files** (`.mcp.json`, `opencode.json`, `.codex/config.toml`, `.claude/settings.json`, hooks) parsed and rewritten, plus **filesystem paths** (`binaryPath`, `clientPath`, project root) interpolated into executable command strings.

- **Command injection into written config (R-10)** — the highest-value security surface: hook `command` and TOML `command=`/`args` are emitted from paths; a malicious/odd path could inject arguments or break out of a TOML string. Blast radius: the emitted command runs on every hook event in the consumer repo. Mitigation asserted via metacharacter fixtures + `args[]`-array emission.
- **Path traversal / write-outside-root** — every writer must be `isWithinProject`-guarded; assert zero writes outside the resolved root and to the named global paths (AC-12, R-14). Blast radius if unguarded: writing to `~/.codex/`, `~/.claude/` — an explicit non-goal.
- **Malformed-input DoS / corruption** — malformed JSON/TOML must warn-and-skip, never throw or partial-write (AC-15, R-11); a partial write corrupts a consumer config. Blast radius: bricked harness config.
- **Foreign-content tamper** — the surgical writers must never alter foreign entries (R-04, R-08); silent mutation of a foreign MCP server or the Ollama provider block is a trust/integrity breach.

No secrets are written by design (cloud `url` transport may carry a token — assert no token is logged or written to a definition file, and that `--dry-run` output does not print it).

## Failure Modes

- **Malformed config** → leg `skipped-malformed`, input preserved, warning surfaced, no throw (AC-15). Wire legs never throw; init's claude `.mcp.json`/`settings.json` path retains its existing loud throw (backward compat, SR-07).
- **Undetected harness** → `skipped-undetected`, no write, exit success (AC-13) — distinct from malformed skip (R-11).
- **Intent not granted** → `skipped-intent` + help line, no write (AC-11).
- **Path escapes root** → `skipped`, never followed (AC-12).
- **Untrusted `.codex/`** → surfaced precondition warning / fail-loud, never a silent inert pass (NFR-09, R-06).
- **Cloud transport undefined (Open Q1)** → codex cloud return path must fail loud / be explicitly marked not-yet-feasible, not silently pass on config presence (R-03).

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (TOML foreign-key preservation) | R-04 | ADR-002 in-house surgical writer; foreign-byte diff = empty is first-class acceptance (NFR-08) |
| SR-02 (trust-gated green-in-test/inert-for-user) | R-06 | Trust confirmed in CI fixture or escalated; precondition surfaced + fail-loud (NFR-09, Open Q2/Q4) |
| SR-03 (cloud release-only gate tax) | R-03 | Pre-tag real-server exercise (ADR-006); Open Q1 blocks codex cloud arm until transport decided |
| SR-04 (`--force` blast radius) | R-13 | `--force` definitions-only; wiring diff = empty assertion (AC-02) |
| SR-05 (skills-only scope read as full parity) | R-16 (partial) | Skills-only boundary asserted in help/output; protocols/agents untouched (FR-09) |
| SR-06 (vnc-049 sentinel regression) | R-08 | Byte-for-byte sentinel + retrieval-still-returns regression (#5743, AC-05) |
| SR-07 (claude-code backward compat) | R-07 | Golden-file pre-refactor; common-path diff = empty (NFR-06) |
| SR-08 (warn-and-skip masks unwired leg) | R-11 | `skipped-*` distinct, visible, asserted manifest outcomes (#4473, AC-15) |
| SR-09 (ceremonial wiring) | R-01, R-02 | Verifier EXECUTES manifest command; return+fire asserted; negative/mutation controls (ADR-006, #4177) |
| SR-10 (provider mislabel) | R-05 | `--provider codex-cli` on every command (static) + ingested `provider` value (behavioral); split-brain guard (#5737) |
| SR-11 (intent-gate bypass) | R-12 | New-entry write gated on `--harness`/opt-in; both arms asserted (AC-11, #960) |
| SR-12 (dry-run divergence / containment) | R-14 | dry-run action-set equality + containment guard + global-path write-absence (AC-12/14) |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | ~11 (return/fire from manifest per leg, mutation/negative controls, pre-tag real-server exercise, cloud feasibility matrix) |
| High | 6 (R-04..R-09) | ~16 (TOML preservation+adjacency, provider static+behavioral+split-brain, trust env+precondition, golden files, opencode sentinel+return, size-gate dual-limit) |
| Medium | 7 (R-10..R-16) | ~15 (injection fixtures, skip-reason distinctness, intent both arms, `--force` zero-wiring, dry-run/containment, event-set validation, first-run+non-clobber) |
| **Total** | **16** | **~42** |

## Knowledge Stewardship
- Queried: context_search for install/wiring lessons, ceremonial-wiring patterns, release-only gate failures, provider split-brain, size-gate, opencode sentinel — findings: #5267 (never-green-on-tag → R-03), #4473 (warn+continue masking → R-11), #4177 (tautology caught at gate → R-02), #4311/#5737/#5748 (provider attribution/split-brain/source_domain-at-ingress → R-05), #5743 (vnc-049 non-clobber sentinel → R-08), #5377/#5372/#4780/#5378 (hook-client dual size gate → R-09), #5192 (verify-by-name false-green → R-03 pre-tag), #5768 (ADR-006 C14 spine).
- Stored: nothing novel to store — the dominant patterns (ceremonial-wiring/execute-the-manifest, never-green-on-tag pre-tag gate, warn-continue masking, hook-client dual size gate, provider split-brain) already exist as cross-feature entries; nan-023's risks are specific instantiations captured in this document.
