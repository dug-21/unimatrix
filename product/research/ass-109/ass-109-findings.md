# FINDINGS: One Node client for local and cloud — production install vs. this repo's dogfood

**Spike**: ass-109 (#1020)
**Date**: 2026-11-25
**Approach**: investigation (+ throwaway clean-repo npm install for Q3)
**Confidence**: evidence-backed for Q0–Q4; directional for Q5–Q6

Working-tree state at time of spike (uncommitted, reported alongside HEAD where it differs): `.codex/hooks.json`, `opencode.json`, `.claude/settings.json` (model line only), `.opencode/agent/`→`.opencode/agents/`, new untracked `.opencode/plugins/unimatrix.js`.

---

## Findings

### Q: 0. Regression timeline
**Answer**: Hypothesis **proven** for Claude hooks and MCP: the OpenCode/Codex adoption commit regressed dogfood. Codex hooks were never on the Node client in this repo.

| Date | Commit | Effect |
|---|---|---|
| 2026-03-12 → 03-20 | `96b896b1`, `f9dbd0e3` | `.mcp.json` → `/usr/local/cargo/bin/unimatrix`; hooks → Rust `unimatrix hook <EVENT>` |
| 2026-04-18 | `04433547` (vnc-013) | `.codex/hooks.json` created, Rust `unimatrix hook … --provider codex-cli` |
| 2026-06-10/11 | `0274a7da`, `3823926e`, `75877b9c`, `f84b5938` (nan-016, vnc-031) | `dogfood-install.sh` + `dogfood-switchover.sh` |
| **2026-06-12** | **`f99baa74`** | **Last known-good.** Claude hooks promoted to `node ~/.unimatrix/dogfood-client/lib/hook-client/index.js <EVENT>`; F6 (#682) soak clock starts. Design: nan-016 ADR-001/002/003 (#5825, #4925, #4926), vnc-031 ADR-003 (#4941), vnc-027 OQ4 (F4a switchover). MCP = cargo binary. |
| 2026-09-11 | `a9161558` | Codex hooks reshaped (matchers), still Rust. |
| **2026-09-15 16:23** | **`4df72389` (#982)** "add OpenCode/Codex harness config + switch to npm unimatrix binary" | **Break.** All 8 Claude hooks → `LD_LIBRARY_PATH=… /usr/local/share/nvm/…/@dug-21/unimatrix-linux-x64/bin/unimatrix hook <EVENT>` (Rust, npm-global release binary). `.mcp.json` and new `opencode.json` → npm-global release binary. |
| 2026-09-15 17:43 | `039ef91c` | Claude hooks restored to Node via install+promote. `.mcp.json` / `opencode.json` left on npm-global release binary. F6 soak clock implicitly reset (rollback-state window). |
| 2026-09-16 | `4e5a7614` (vnc-049), `ac163873` (nan-023) | Schema v32; per-harness wiring. |
| 2026-11-23 | `a86e49b4` (#1005) | `.mcp.json` → cargo binary after v31 release daemon refused v32 DB (`SchemaTooNew`). `opencode.json` not fixed at HEAD (fixed only in working tree). |

**Evidence**: `git log`/`git show` on the listed files and commits; commit message of `039ef91c` states #984/#982 "left .claude/settings.json in dogfood ROLLBACK state".
**Recommendation**: Treat `4df72389` as the regression point. The root cause is that harness config was hand-edited to point at an installed release binary instead of being produced by the dogfood tooling; fix by making dogfood tooling own every harness surface (see gaps T1–T3).

### Q: 1. Decision history
**Answer**:
- **#682 (open)** "Rust hook.rs client retirement gate (F6 — committed in principle)": retire the Rust **hook** client path; Rust binary stays as MCP server/daemon; retirement is a config change in `init.js`; gates = parity suite, latency, queue/PreCompact parity, ≥4-week runtime soak on installed TS client **plus** packaging fidelity of `npx … init`. **Current.**
- **ass-068 FINDINGS Q2** (`product/research/ass-068/FINDINGS.md:90`): one pure-TS client, two transports (UDS, HTTP). Current (implemented by vnc-026/027).
- **vnc-027 SCOPE** problem 1: "two hook clients, two languages … contradicts the single-edge-language vision (JS/TS only) and blocks F6"; adds TS UDS transport; OQ4 recommends dogfood switchover. Current.
- **vnc-026 ADR-001 (#4751)**: parity corpus with Rust hook as oracle. Current — note it keeps hook.rs alive as test oracle until F6.
- **nan-016 ADR-001 (#5825)** copy-install via npm pack, never `npm link`; **ADR-002 (#4925)** fixed dir `~/.unimatrix/dogfood-client/`, not npm global prefix; **ADR-003 (#4926)** switchover via shipped `mergeSettings`. Current. #982 (`4df72389`) violated ADR-002's intent by pointing at the npm global prefix.
- **vnc-031 ADR-003 (#4941)**: retired script-level prune; supersedes the nan-016 pattern #4936 (stale `*` matcher not pruned). #4936 is effectively superseded.
- **Pattern #4923**: TS client derives state/socket from project-root hash, so copy-install shares state with the Rust binary.
- **Goal**: `product/goals/personal-cloud/CAPABILITIES.md` C14 (multi-LLM parity, 🔴) and remote≡local parity done_when (line 80).
- No ADR found that commits the **MCP** definition to a Node entry locally; the local MCP entry is the Rust binary in bridge mode (`main.rs:698`), which #682 explicitly keeps.

**Evidence**: Unimatrix `context_search` (entries above); `gh issue view 682`; feature SCOPE files.
**Recommendation**: Use #682 body as the controlling commitment. Scope the follow-up as "F6 prerequisite: production local init emits Node hooks for every harness," because #682's packaging-fidelity gate cannot pass while init emits Rust hooks (Q2).

### Q: 2. Rust client usage
**Answer**: **Disproven** that Rust client usage is little or none. Rust `hook` is still the client in production local installs for Claude and OpenCode, and in this repo's Codex (HEAD).

| Surface | Harness | Client invoked | Evidence |
|---|---|---|---|
| `lib/wire.js:154-157` `dispatchClaude` local | Claude (production) | **Rust** `LD_LIBRARY_PATH=… <bin> hook <EVENT>` (legacy string arm, `merge-settings.js:120-128`) | Scratch install: all 8 Claude hooks were Rust. nan-023 C-10 required "byte-identical" output, freezing this. Pre-nan-023 `init.js:617` did the same. |
| `lib/init.js:554` | Claude (cloud/bundle) | Node | code |
| `lib/codex-install.js:536` | Codex (production) | Node `--provider codex-cli` | scratch install |
| `opencode-plugin/lib/emit.js:5,43` (published `@dug-21/unimatrix-opencode-plugin`) | OpenCode (production) | **Rust** via `unimatrix hook … --provider opencode` on PATH; `bin/unimatrix.js` routes only `init`/`wire`/`mcp-bridge` to JS (lines 171/214/225), everything else execs the Rust binary | code |
| `.claude/settings.json` HEAD + WT | Claude (repo) | Node dogfood client | file |
| `.codex/hooks.json` **HEAD** | Codex (repo) | **Rust** `unimatrix hook … --provider codex-cli` (PATH → `/usr/local/cargo/bin/unimatrix`) | file |
| `.codex/hooks.json` **WT** | Codex (repo) | Node dogfood client | diff |
| `.opencode/plugins/unimatrix.js` **WT only** (untracked) | OpenCode (repo) | Node dogfood client via `Bun.spawn` | file |
| `.gemini/settings.json` | Gemini (reference) | Rust | file |
| `scripts/dogfood-switchover.sh rollback` | Claude (repo) | Rust `target/release/unimatrix hook` (by design) | script |
| `README.md:252,254,280` | docs | Documents `unimatrix hook` for Codex/OpenCode/Gemini (line 252 also says Codex targets JS client — inconsistent) | file |
| MCP entries (`.mcp.json`, `opencode.json`, init local) | all | Rust binary bridge mode — **in scope as server, not hook client**, per #682 | — |
| CI | — | No workflow invokes `unimatrix hook` (grep `.github/workflows` empty) | grep |

**Recommendation**: Flip `dispatchClaude` local to the Node command source and the OpenCode plugin's emit to `node <clientPath>`; drop C-10 byte-identity for local Claude hooks; update README.

### Q: 3. Production local install
**Answer**: Experiment: `npm pack packages/unimatrix` (0.11.3) → `npm install <tgz>` in a scratch git repo (`HOME` and npm cache redirected to scratchpad; pre-seeded `.codex/` and `opencode.json` `{}`) → `npx unimatrix init`. Registry supplied `@dug-21/unimatrix-linux-x64@0.11.3`.

| Written | Location | Value | Portable |
|---|---|---|---|
| `.mcp.json` | project | absolute `<repo>/node_modules/@dug-21/unimatrix-linux-x64/bin/unimatrix` + absolute `LD_LIBRARY_PATH` | No |
| `.claude/settings.json` hooks | project | 8 × absolute Rust `hook` commands | No; and Rust client |
| `.codex/hooks.json` | project | absolute `node <repo>/node_modules/@dug-21/unimatrix/lib/hook-client/index.js <EVENT> --provider codex-cli` | No |
| `.codex/config.toml` MCP | — | skipped-intent (needs `wire --harness codex-cli`) | — |
| `opencode.json` | project (host-owned) | added `"plugin": ["@dug-21/unimatrix-opencode-plugin"]`; retrieval MCP skipped-intent | Yes, but edits host-owned file |
| `.opencode/package.json`, `.opencode/plugins/unimatrix.js` | project | depends on published plugin `^0.11.3` (Rust emit) | Yes |
| `.claude/skills/*` | project | copied | Yes |
| `~/.unimatrix/<hash>/` (db, config.toml, vector), `~/.cache/unimatrix/models` | user | DB pre-created | n/a |
| `.gitignore`, `settings.local.json` | — | untouched | — |

Result: not portable (absolute machine paths in files hosts normally commit); does not touch `.gitignore` (respects host policy); does mutate host-owned `opencode.json`. No per-harness local overlay is used.
**Evidence**: scratch repo under session scratchpad; `git status` after init; file contents above.
**Recommendation**: Write machine-specific values (absolute binary/client paths, `LD_LIBRARY_PATH`) only to per-harness local surfaces (Claude `settings.local.json` + local-scope MCP; OpenCode/Codex user-level config), and keep project files portable (`node`/`npx` relative or command-name forms) — gap P2.

### Q: 4. This repo's dogfood setup
**Answer**: Current procedure (from scripts and commits): (1) `cargo build`/install → `/usr/local/cargo/bin/unimatrix` (manual; currently dated 2026-09-17); (2) `scripts/dogfood-install.sh` → frozen copy of the in-repo client at `~/.unimatrix/dogfood-client/` (client only, platform binary asserted absent); (3) `scripts/dogfood-switchover.sh promote` → rewrites **only** `.claude/settings.json` hooks; (4) everything else hand-edited: `.mcp.json` (cargo), `opencode.json` (npm-global at HEAD, cargo in WT), `.codex/hooks.json` (Rust at HEAD, Node in WT), `.opencode/plugins/unimatrix.js` (WT only, hand-written Bun shim).

Differences vs production and reason:
| Difference | Pre-release-only? |
|---|---|
| Server binary = repo-built cargo binary, not `node_modules` platform package | **Yes** — schema migrations ship before release (`a86e49b4`) |
| Client = copy of in-repo package at fixed dir, not `node_modules` | **Yes** — unreleased client changes; fixed dir per #4925 survives rebuild except `~/.unimatrix` wipe (`039ef91c`) |
| Claude hooks Node; production init emits Rust | **No** — production defect (P1) |
| OpenCode via hand-written local shim, not published plugin | Partly — published plugin emits Rust (P3) and can't target dogfood client |
| Codex/OpenCode/MCP not managed by any script | **No** — tooling gap (T1) |
| Absolute `/home/vscode/...` paths in tracked files | No — same non-portability as production (P2/T2) |

**Evidence**: `scripts/dogfood-*.sh`; git diff HEAD; commits in Q0.
**Recommendation**: Keep only the two pre-release substitutions (server binary, client tree); route all harness wiring through the same `wire` code path as production.

### Q: 5. Pre-release upgrade path (directional)
**Answer**: Yes, feasible, with three blockers. `npm install <local tgz>` works (shown in Q3), but: (a) the root package's `optionalDependencies` resolves the **released** platform binary from the registry → schema v31 binary against v32 DB → `SchemaTooNew` (the exact `a86e49b4` failure). Needs a locally packed platform package (`npm pack` of a `unimatrix-linux-x64` dir containing the cargo-built binary + onnxruntime) installed alongside, or an `UNIMATRIX_BINARY`-style override in `resolve-binary.js`. (b) Rollback is one-way once the repo binary migrates the DB; going back to a release requires a DB backup — not a config flip. (c) Hook config: until P1 is fixed, production `init` would regress this repo's Claude hooks to Rust; and installing into this repo's `node_modules` writes absolute in-repo paths that change on clean/reinstall. Swap scripts become unnecessary once (a)+(P1)+(P2) land; `dogfood-install.sh` remains useful only as "pack the client" step.
**Evidence**: Q3 experiment; `package.json` optionalDependencies; `a86e49b4` message.
**Recommendation**: Adopt "pack both artifacts locally + run production `unimatrix wire`" as the target dogfood procedure; add a binary-path override and pre-migration DB backup step.

### Q: 6. Per-harness local surfaces (directional, from harness knowledge; not re-verified against current upstream docs)
- **Claude Code**: project `.claude/settings.json` (shared), `.claude/settings.local.json` (personal, Claude auto-gitignores it), user `~/.claude/settings.json`; hooks from all scopes are combined (all run), scalar settings take precedence local > project > user. MCP: project `.mcp.json`, local/user scope in `~/.claude.json` (`claude mcp add --scope local`). Local-scope MCP and `settings.local.json` are the overlay for absolute paths.
- **Codex CLI**: user `~/.codex/config.toml` + `~/.codex/hooks.json`; project `.codex/config.toml` / `.codex/hooks.json` honored only for trusted projects (README:252 notes trust dependency). No dedicated `*.local` overlay file; user-level config is the local overlay.
- **OpenCode**: global `~/.config/opencode/opencode.json` and `~/.config/opencode/plugins/`, project `opencode.json` and `.opencode/plugins/`, `OPENCODE_CONFIG` env path; configs deep-merge (project over global). User-level config/plugins are the local overlay.
**Recommendation**: Machine-specific values go to: Claude → `settings.local.json` + local-scope MCP; Codex → `~/.codex/*`; OpenCode → `~/.config/opencode/*` or `OPENCODE_CONFIG`. Verify Codex hook-file merge semantics before building on it.

---

## Install procedures (described separately)

**Production local install (current, as shipped)**: `npm install @dug-21/unimatrix` (pulls platform binary) → `npx unimatrix init` (skills, DB pre-create, Claude MCP + hooks, Codex hooks, OpenCode plugin) → `npx unimatrix wire --harness codex-cli|opencode` to opt in MCP for those harnesses. Target after fixes: same commands, but Node hooks for all harnesses and machine paths in local overlays.

**This repo (current)**: `cargo build --release` and install to `/usr/local/cargo/bin/unimatrix` → `scripts/dogfood-install.sh` → `scripts/dogfood-switchover.sh promote` (Claude hooks only) → hand-maintain `.mcp.json`, `opencode.json`, `.codex/hooks.json`, `.opencode/plugins/unimatrix.js`. Target: pack client + locally packed platform package (or binary override) → production `unimatrix wire` with overlays; no switchover script.

---

## Gap list

| ID | Tag | Gap | Size |
|---|---|---|---|
| P1 | `production` | Local `init`/`wire` emits Rust `hook` commands for Claude (`wire.js:154-157`, legacy string arm); blocks #682 packaging gate | bugfix (small; drop C-10 byte-identity, update goldens) |
| P2 | `production` | Absolute machine paths written into project files (`.mcp.json`, `.claude/settings.json`, `.codex/hooks.json`); no use of per-harness local overlays | feature (medium; per-harness overlay writer) |
| P3 | `production` | Published OpenCode plugin emits `unimatrix hook` → Rust (`opencode-plugin/lib/emit.js`); `bin/unimatrix.js` has no `hook`→JS route | bugfix (small–medium) |
| P4 | `production` | `init` edits host-owned `opencode.json` (adds `plugin`) | bugfix (small; move to `.opencode/` or user config) |
| P5 | `production` | README:252/254/280 document Rust `unimatrix hook` for Codex/OpenCode/Gemini; `.gemini/settings.json` reference config is Rust | docs (small) |
| P6 | `production` | No binary-path override; local platform binary always from registry version pin | feature (small; enables T4) |
| T1 | `this-repo-only` | Dogfood tooling manages only Claude hooks; `.mcp.json`, `opencode.json`, `.codex/hooks.json`, OpenCode shim are hand-edited (root cause of `4df72389`) | feature (medium; or subsumed by T4) |
| T2 | `this-repo-only` | HEAD `.codex/hooks.json` = Rust; HEAD `opencode.json` MCP = stale npm-global release binary (v31 vs v32 DB); fixes exist only uncommitted | bugfix (trivial; commit WT after review) |
| T3 | `this-repo-only` | OpenCode dogfood shim `.opencode/plugins/unimatrix.js` untracked, hand-written, hardcodes `/home/vscode` | bugfix (small) |
| T4 | `this-repo-only` | No procedure to install locally packed client + repo-built binary through the production path; no pre-migration DB backup / rollback story | feature (medium; depends on P1, P2, P6) |
| T5 | `this-repo-only` | F6 soak continuity: rollback window on 2026-09-15 reset the soak; soak-reset events are not recorded anywhere | process (small) |
| T6 | `this-repo-only` | `/usr/local/cargo/bin/unimatrix` refreshed manually (built 2026-09-17); nothing ties it to HEAD | bugfix (small; script step) |
| T7 | `this-repo-only` | Stray tracked `.mcp copy.json` with old debug config | cleanup (trivial) |

---

## Unanswered Questions

- Q6 harness surfaces were not re-verified against current upstream docs (no external fetch performed); Codex hook-file merge across user/project scopes is the least certain point.
- Whether the released 0.11.3 binary in the Q3 experiment had schema v31 was taken from `a86e49b4`, not re-measured.

---

## Out-of-Scope Discoveries

- Rust `hook.rs` is also the parity-corpus oracle (#4751); F6 retirement needs a replacement oracle or frozen goldens — worth a note on #682.
- `scripts/dogfood-switchover.sh rollback` targets `target/release/unimatrix`, while MCP uses `/usr/local/cargo/bin/unimatrix` — two repo-built binary locations.
- Three `core.*` dumps under `product/test/infra-001/` are untracked working-tree debris.

---

## Recommendations Summary

- Q0: Regression = `4df72389` (#982, OpenCode/Codex adoption) 2026-09-15; last good `f99baa74` 2026-06-12; Codex hooks were never Node at HEAD. Hypothesis proven.
- Q1: #682 is the controlling commitment (retire Rust hook client; keep Rust server); supported by ass-068, vnc-027, nan-016 ADRs #5825/#4925/#4926, vnc-031 #4941; #4936 superseded.
- Q2: Disproven — Rust is still the hook client for production Claude local (`wire.js`) and production OpenCode (plugin `emit.js`), plus this repo's Codex at HEAD. Fix P1 + P3.
- Q3: Production install is not portable (absolute paths in project files) and edits host `opencode.json`; leaves `.gitignore` alone. Move machine values to per-harness local overlays.
- Q4: Dogfood differs legitimately only in server binary and client tree source; all other differences are defects or missing tooling.
- Q5: Production procedure with locally packed artifacts is feasible after P1, P2, P6; DB migration makes rollback one-way — add backup step.
- Q6: Overlays: Claude `settings.local.json` + local-scope MCP; Codex `~/.codex/*`; OpenCode `~/.config/opencode/*` / `OPENCODE_CONFIG`.
