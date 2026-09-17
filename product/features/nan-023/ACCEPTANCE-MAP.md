# nan-023 Acceptance Criteria Map

> Feature: nan-023 · Issue: #990 · Capability: C14 (multi-LLM harness parity) · Goal: `personal-cloud` (#4946)
> Source: SCOPE.md AC-01..AC-16, SPECIFICATION §6, RISK-TEST-STRATEGY R-01..R-16.
> Behavioral ACs (AC-05 return, AC-08/09 codex hooks, AC-10 return) are asserted from the wired command path via the `WireLeg` manifest — never discharged by a config-presence proxy (SR-09 guard).

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | `unimatrix init` never overwrites an existing Unimatrix-owned skill by default; a modified existing skill survives a re-run byte-for-byte | test | Fixture repo with a modified installed skill; run `init`; byte-diff the skill file = empty (FR-07, R-16) | PENDING |
| AC-02 | `init --force` overwrites Unimatrix-owned skills with shipped versions; foreign files under same dirs untouched; `--force` re-asserts/overwrites NO wiring | test | Modified skill + foreign file + pre-existing wiring; run `--force`; skill == shipped, foreign byte-identical, every wiring surface byte-identical to pre-run (FR-02/08, SR-04, R-13) | PENDING |
| AC-03 | `unimatrix wire` ensures MCP + hooks + retrieval for every detected harness and writes/modifies zero definition files | test | Multi-harness fixture; run `wire`; assert wiring surfaces changed AND skill/protocol/agent trees byte-identical (FR-03) | PENDING |
| AC-04 | Wire path run twice → byte-identical config for every supported harness (idempotent) | test | Run `wire` twice; byte-diff each wiring surface between runs = empty (NFR-01, FR-03, R-04) | PENDING |
| AC-05 | opencode detected → `mcp.unimatrix` ensured; existing entry + Ollama `provider` block + foreign keys preserved byte-for-byte; retrieval still returns | test | opencode fixture; byte-diff sentinel regions = empty; then issue `context_*` against the wired slug and assert it RETURNS (FR-11, NFR-02, vnc-049 sentinel, SR-06, R-08) | PENDING |
| AC-06 | Codex detected → `[mcp_servers.unimatrix]` written/merged into `.codex/config.toml`; foreign `[mcp_servers.*]` and all other keys/comments/ordering preserved | test | TOML fixture with foreign tables/interleaved comments/non-alpha key order; run wire; assert owned table present AND foreign regions byte-preserved incl. adjacency (FR-12, NFR-08, SR-01, R-04) | PENDING |
| AC-07 | Codex detected → hooks in Claude-like, event-driven shape with `--provider codex-cli` on every command; foreign hook entries preserved | test | Parse written `.codex/hooks.json`; assert event-driven `type:"command"` shape, `--provider codex-cli` on every command (fail-loud if missing), foreign hooks byte-preserved (FR-13, NFR-07, R-05) | PENDING |
| AC-08 | Codex hook commands target the JS hook client (`hook-client/`), not the unimatrix binary — asserted against the written command string | test | String-assert each written command invokes `node <…/hook-client/index.js> …` and does NOT invoke the `unimatrix` binary (FR-13a, ADR-003) | PENDING |
| AC-09 | Package-wired Codex hooks actually FIRE — a wired hook event reaches the JS hook client (behavioral, not presence). Local HARD; cloud per Gate-0 trust outcome. **7 events RECORD per-event firing evidence (Q4).** | test | In a trusted `.codex/` env, execute the manifest `command` with a synthetic event; assert ingress at JS hook client stamped `provider=codex-cli` (not `source_domain`). Negative/mutation control must fail when artifact broken but string intact. For each of the 7 emitted events RECORD fires vs wired-inactive (see per-event table below) — mirroring Claude's names does NOT prove Codex fires them. Local HARD; cloud per Gate-0 (§ Codex trust) + feasibility matrix (FR-13/16, NFR-05/09, R-01/02/06/15) | PENDING |
| AC-10 | After wiring, a `context_*` retrieval against the wired slug RETURNS (not just config written). claude-code required; codex-cli local required / cloud per Gate-0 trust; opencode required via `mcp.unimatrix`. Cloud codex uses the token-free stdio bridge (Q1). | test | Per harness, connect the MCP target from `manifest[].entry` and issue `context_*`; assert non-empty RETURN. Config-presence check explicitly insufficient. Mutation control: broken entry must fail. Cloud codex entry = `command="node", args=[bridge, hash]` (never `url=`). Deployment coverage per feasibility matrix + Gate-0 (FR-16, NFR-05, SR-09, R-01/02/03) | PENDING |
| AC-11 | Writing a NEW MCP/retrieval entry into a user-owned config requires explicit `--harness`/opt-in; additive merge into an existing surface needs none | test | With no `--harness`, assert no new `mcp.unimatrix`/`[mcp_servers.unimatrix]` written into a config lacking it → `action="skipped-intent"` + help line; with `--harness <x>`, assert it IS written; additive merge proceeds without opt-in (FR-04/14, SR-11, R-12) | PENDING |
| AC-12 | No path writes any config outside the resolved project root (asserted for `~/.codex/`, `~/.claude/`, `~/.gemini/`, `~/.config/`) | test | Run init/wire in a sandbox; assert zero writes to the named global paths; a writer handed a target escaping root → `skipped`, never followed (FR-18, NFR-04, SR-12, R-14) | PENDING |
| AC-13 | Wiring gated on project-local markers; a harness with no marker → no writes, no error | test | Repo missing a harness marker; run wire; assert `skipped-undetected` leg, no writes for that leg, exit success (FR-15) | PENDING |
| AC-14 | Both definition install and wiring honor `--dry-run`: intended actions printed with `[dry-run]`, nothing written; action set matches real-path action set | test | Run each verb with `--dry-run`; assert `[dry-run]`-prefixed output AND zero filesystem changes AND action set == real-path action set (FR-05, NFR-10, R-14) | PENDING |
| AC-15 | Malformed config (invalid JSON/TOML) for any harness → preserved unchanged, leg skipped with a warning; init/wire never throw or partially write | test | Inject malformed config per harness; assert input byte-preserved, `skipped-malformed` warning surfaced (visible, not swallowed), no throw, no partial write; distinct from `skipped-undetected` (FR-19, NFR-03, FR-17, SR-08, R-11) | PENDING |
| AC-16 | Command surface + per-harness wiring consistent with #960 — no path silently installs an unintended surface; ambiguous invocations surface help | test | Assert no silent new-surface install; assert ambiguous/unknown invocation (unknown `--harness`, conflicting flags) prints help rather than defaulting to install (FR-06/14, ADR-005, R-12) | PENDING |

## AC × Harness × Deployment Feasibility Matrix (Q3 — REQUIRED)

Every cell is a stated fact — **required**, **conditional-on-trust** (Gate-0 decides; hard if feasible, documented conditional if not — never silent-skipped), **infeasible(reason)**, or **n/a(reason)**. This finishes SCOPE's "where feasible" into an enumerated, auditable claim; no cloud assertion may be silently dropped. Covers the behavioral ACs across claude-code, opencode, codex-cli × local/cloud.

| AC | Harness | Local | Cloud |
|----|---------|-------|-------|
| **AC-05** (opencode retrieval returns) | claude-code | n/a (opencode-specific AC) | n/a |
| AC-05 | opencode | required | required (via token-free bridge) |
| AC-05 | codex-cli | n/a (opencode-specific AC) | n/a |
| **AC-08** (codex hook cmd targets JS client — string assertion) | claude-code | n/a (codex-specific AC) | n/a |
| AC-08 | opencode | n/a (codex-specific AC) | n/a |
| AC-08 | codex-cli | required | required (written command string is deployment-independent) |
| **AC-09** (codex hooks actually FIRE) | claude-code | n/a (codex-specific AC) | n/a |
| AC-09 | opencode | n/a (codex-specific AC) | n/a |
| AC-09 | codex-cli | required (fixture seeds trusted `.codex/`) | conditional-on-trust (Gate-0: hard if cloud CI can seed trusted `.codex/`; else documented conditional, C14 partial(cloud)) |
| **AC-10** (retrieval RETURNS after wiring) | claude-code | required | required (proven bridge path) |
| AC-10 | opencode | required | required (via `mcp.unimatrix` bridge) |
| AC-10 | codex-cli | required (local trust seedable) | conditional-on-trust (Gate-0; cloud entry = `command="node", args=[bridge, hash]`, never `url=` — Q1) |

### AC-09 Per-Event Codex Firing Evidence (Q4)

Mirroring Claude's event names does NOT prove Codex fires all seven. AC-09 must RECORD, per event, whether it reaches the JS hook client (`fires`) or is present-but-silent (`wired-inactive` — a documented gap, not assumed-firing). Fill during Stage 3c; any `wired-inactive` is a named gap, never a silent drop.

| Codex Event | Matcher | Local firing | Cloud firing (per Gate-0) |
|-------------|---------|--------------|---------------------------|
| SessionStart | — | RECORD | RECORD |
| UserPromptSubmit | — | RECORD | RECORD |
| PreToolUse | `^context_cycle$\|^mcp__unimatrix__context_cycle$` | RECORD | RECORD |
| PostToolUse | `*` | RECORD | RECORD |
| PreCompact | — | RECORD | RECORD |
| SubagentStart | `*` | RECORD | RECORD |
| Stop | — | RECORD | RECORD |

## Cross-Cutting Verification Notes

- **Backward-compat golden (SR-07/R-07):** capture byte-for-byte golden of claude-code `.mcp.json` + `.claude/settings.json` from a fixture BEFORE the per-harness refactor; assert unchanged after `writeMcpJson`/`mergeSettings` route through `wire.js`. Guards AC-03/AC-10 claude-code leg.
- **Non-tautology (SR-09/R-02):** the C14 verifier must EXECUTE `manifest[].command`/connect via `manifest[].entry`, never string-match or reconstruct paths. At least one mutation/negative control per behavioral AC (AC-09, AC-10) that fails when the wired artifact is broken but its manifest string is intact.
- **hook-client size gate (R-09):** after the `--provider` argv addition, `test/check-hook-client-size.js` asserts stripped ≤ 110 KB (PRIMARY) and raw ≤ 200 KB (BACKSTOP); size-gate meta-assertion moves in lockstep if constants change. Never raise the gate.
- **Command injection (R-10):** fixture with a project root path containing spaces/quotes; assert emitted TOML `command`/`args` and hooks `command` are correctly quoted/escaped and re-parse to intended argv.
- **Pre-tag real-server exercise (SR-03/R-03):** run codex hook-fire + retrieval-return + opencode/claude return against a real local+cloud server BEFORE the release chain, to surface layered failures off-tag.

## Design-Phase Open Questions — RESOLVED (human-approved; baked into ACs above)

| Open Question | Affected AC arms | Resolution |
|---------------|------------------|------------|
| Codex cloud MCP transport shape | AC-10 codex cloud arm | **Q1:** mirror the token-free stdio bridge — `command="node", args=[bridge, hash]`; never `url=` (Principle 8). Asserted in the AC-10 codex-cloud cell. |
| CI can mark `.codex/` trusted | AC-09 / AC-10 codex firing | **Q2:** Gate-0 precondition confirmed by tester BEFORE delivery. Feasible → hard; not feasible → documented conditional (local hard, cloud partial), never silent-skip (R-06, SR-02). |
| Per-AC cloud-vs-local feasibility matrix | AC-05 / AC-08 / AC-09 / AC-10 "where feasible" | **Q3:** matrix above states every cell as required / conditional-on-trust / infeasible / n/a — no silent drop. |
| Exact codex lifecycle event set | AC-09 fire coverage | **Q4:** 7-event set adopted; per-event firing evidence RECORDED (table above); non-firing → wired-inactive documented gap (R-15). |
| codex-cli arm in both normalizers | AC-07 / AC-09 attribution | **Q5:** confirmed present in `normalize.js:23` + `hook.rs:35`; no arm to add. Only gap is `parseHookArgs` in `index.js`; keep twins in lockstep via parity corpus #4751 (R-05, #5737). |
