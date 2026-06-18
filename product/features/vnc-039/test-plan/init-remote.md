# Test Plan — C4 `initRemote()` + `.mcp.json`/store write + legacy message

> Scope **A+B** · `[no-cloud]` (+ `[stub/local]` for the no-leak grep on a bridge run) · MODIFIED `lib/init.js`.
> Risks: **R-09, R-10, R-11, R-12**, contributes R-08/R-15. ACs: **AC-01, AC-07, AC-08, AC-08b, AC-09, AC-10**.
> Cumulative: **extend `test/init-remote.test.js`** (no new file). Reuse its temp-`HOME`/temp-repo fixtures, the `writeMcpJson` idempotency fixtures, and the `CLIENT_PATH` resolve-with-fallback pattern. Override `os.homedir()` to a temp dir so the store lands out-of-tree under a temp root.

## Behavior under test (ARCHITECTURE §4.5/§4.6/§6)
On the **bundle** path: `initRemote()` writes the credential via `credstore.write` (B) and writes the stdio `unimatrix` `.mcp.json` bridge entry (A); removes the "Skipped .mcp.json" line and `gitignoreWarning`; deletes any stale in-tree `unimatrix.remote` subtree. On the **legacy** path: writes the universal store entry with `fingerprint:null`, wires **no** bridge, emits a loud deterministic unsupported message.

---

## AC-01 — `.mcp.json` stdio bridge entry (R-10)
- `test_initBundle_writesStdioUnimatrixEntry` — run `init --bundle` against a fixture `v:2` bundle; assert `.mcp.json` has `mcpServers.unimatrix = {command:"node", args:[<abs bridge path>, <projectHash>], env:{}}`.
- `test_initBundle_bridgeCommandNotRustBinary` — assert the command invokes the **JS bridge** (node + resolved module path), not the Rust platform binary.
- `test_initBundle_noSkippedMcpJsonLine` — assert the prior "Skipped .mcp.json: remote mode does not register a local MCP server" line is gone.

## AC-07 — idempotent + merge-preserving + dry-run (R-10)
- `test_initBundle_reInit_doesNotDuplicateUnimatrix` — pre-seed `.mcp.json` with a co-resident MCP server; run `init` twice; assert `unimatrix` appears once and the co-resident server is preserved (extends the `writeMcpJson` fixtures).
- `test_initBundle_dryRun_noMcpJsonWrite` — `--dry-run` → no write; intended change reported.
- `test_initBundle_malformedExistingMcpJson_throws` — malformed existing `.mcp.json` → throws (mirrors `writeMcpJson`), does not silently overwrite.

## AC-08 / AC-08b — out-of-tree store, 0600, per-project (R-12, R-07)
- `test_initBundle_writesStoreOutOfTree0600` — after `init --bundle`, assert `~/.unimatrix/<projectHash>/remote.json` exists, mode 0600, contains the token (via `credstore`).
- `test_initBundle_repoTreeFreeOfTokenBearingPath` — assert `git status --porcelain` / a `git add -A` dry-run in the fixture repo lists **no** token-bearing path (the commit-leak the feature closes).
- `test_initBundle_noUnimatrixRemoteCredInSettingsLocal` — grep `.claude/settings.local.json` for the token → absent; no `unimatrix.remote` credential key present.
- `test_initBundle_twoProjects_twoDistinctStores` — `init` for project A then B → two distinct hash-dir stores; re-`init` A → A updated, B untouched (directory separation, AC-08b).
- `test_initBundle_storeWriteFailure_initExitsNonZero_noPartialInTree` — force a store-write failure → init throws/exits 1 (creds must persist) and leaves no token-bearing partial in the tree (R-12).

## AC-08 (migration) — stale in-tree creds cleanup (R-12, FR-27)
- `test_initBundle_deletesStaleSettingsLocalRemoteSubtree` — pre-seed `.claude/settings.local.json` with a `unimatrix.remote` credential and a co-resident unrelated key; run bundle `init`; assert the `unimatrix.remote` subtree is deleted (merge-preserving — other keys survive) and no in-tree token remains.
- `test_initBundle_migrationBestEffort_doesNotAbortInit` — a failure to clean the stale subtree does not abort init (best-effort, ARCHITECTURE §4.6).
- `test_initBundle_gitignoreWarningRemoved` — assert the `gitignoreWarning` output path is gone (no in-tree creds file to warn about — FR-25).

## AC-09 — no token in any init surface (R-09)
- `test_initBundle_printSummary_noToken` — capture `printSummary()` output during remote `init`; assert the token string is absent.
- `test_initBundle_stdoutStderr_noToken` — capture all init stdout/stderr; token absent.
- `test_mcpJson_noTokenNoMcpUrlNoFp` — assert `.mcp.json` contains no token, no `mcp_url`, no `fp` — only `command`/`args:[<bridge>,<projectHash>]`/`env:{}` (AC-09, FR-17).
- (Bridge-run no-leak grep lives in the mcp-bridge plan; this asserts the **init** surfaces.)

## AC-10 — legacy path loud, deterministic, no bridge (R-11, R-15)
- `test_initLegacy_noUnimatrixMcpEntry` — run `init --remote <url> --token <tok>`; assert `.mcp.json` has **no** `unimatrix` MCP entry.
- `test_initLegacy_exactUnsupportedMessageAndExit` — assert the **exact** unsupported-message text (cloud MCP requires a `v:2` bundle) and the command's exit behavior — wording + exit are deterministic and assertable (SR-06), not prose.
- `test_initLegacy_observePathUnchanged` — assert the legacy observe path still works (relocated store entry written with `fingerprint:null`, hook stays unpinned — R-15; the unpinned-resolution assertion itself lives in config-resolve).
- `test_initLegacy_storeWrittenWithNullFingerprint` — assert the universal relocation writes the legacy credential out-of-tree with `fingerprint:null` (no in-tree creds for any remote path — WARN-1 posture).

## Scope B independence (R-08, AC-11)
- The store-write, out-of-tree, migration, and legacy-message tests run with **no cloud reachable and the bridge module never spawned**. (The `.mcp.json` stdio-entry write is Scope A but is a pure file write — it needs no cloud.) The coverage report records the Scope B subset as `[no-cloud]` and mergeable independently of Scope A.

## Edge cases
- No-homedir → store path null → init surfaces the terminal posture (same as `socketPathFor`).
- Existing `.mcp.json` with `unimatrix` already present (re-attach) → updated idempotently, args refreshed to the resolved bridge path.
