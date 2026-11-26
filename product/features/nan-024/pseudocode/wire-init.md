# wire-init (`lib/wire.js`, `lib/init.js`) — JS

## Purpose
Orchestrate per-machine wiring identically for local init, cloud attach and `wire` (AC-01, D-040): install store -> hooks -> MCP -> removal of owned legacy entries across scopes -> warnings/approval step. All user-level edits go through writers that support `dryRun` and return legs so the dry-run list equals the real changes (FR-12d). `init.js` shrinks (763 lines); new logic lives in the other modules.

## wire.js
```
wire(projectRoot, opts) -> {actions, manifest}
opts = {harness?, dryRun, home?, platform?, node?, binaryPath?, mcp? (cloud credential present), force NEVER}
```
```
function wire(projectRoot, opts):
  home = opts.home ?? os.homedir(); platform = opts.platform ?? process.platform; node = opts.node ?? process.execPath
  dryRun = opts.dryRun === true; harnessSel = opts.harness || null
  validate harnessSel (unknown -> actions ["unknown --harness: x"], no writes)       // unchanged
  transport = resolveTransport(projectRoot, opts)         // {kind:"local"|"cloud"|"none"}: Linux binary present => local; credential/bundle => cloud; none => legs skipped "no transport"
  manifest = []
  if transport.kind == "none": push skipped legs for claude-code mcp+hooks; (same as today) ; return
  // 1. user-level footprint (once; idempotent)
  store = installStore({home, platform, nodePath: node, sourcePackageRoot: packageRoot(), projectRoot, dryRun})   // UnsafeShimPathError propagates -> caller exit 1
  manifest.push(storeLeg(store))
  // 2. preflightRecordedNode (wire / upgrade-without-re-record only, D-082): see below; init relies on installStore's NodeNotRecordableError
  // 3. Claude
  dispatchClaude: 
     commandSource = {events: HOOK_EVENTS, commandForEvent: e => buildShimCommand(e, undefined, platform, home)}
     r = mergeSettings(<root>/.claude/settings.local.json, commandSource, {dryRun, crossScopeFiles: [<root>/.claude/settings.json, <home>/.claude/settings.json]})
     manifest: one hooks leg per managed event {path: settings.local.json, command}; one leg per cross-scope removal
     entry = buildMcpEntry(platform, home, node)
     c = writeClaudeMcp({home, projectRoot, entry, dryRun}) -> mcp leg (path ~/.claude.json) ; warning -> manifest leg reason
  // 4. Codex (D-081): maybeWireCodex decides via detectsCodex({root, env, home, harnessSel}) (project .codex/ | existing $CODEX_HOME dir | codex on PATH | --harness codex-cli)
     legs = maybeWireCodex({home, env: process.env, platform, node, projectRoot, harnessSel, dryRun})      // hooks to <codexHome>/hooks.json, removal of owned project/config hooks, MCP to <codexHome>/config.toml (same detection rule, D-084)
  // 5. OpenCode (if detected or harnessSel=="opencode")
     writeOpencodeMcp({home, platform, node, harnessSelected: harnessSel=="opencode", dryRun})
  // 6. owned project-file MCP migration (always, independent of harness selection) (D-060/D-062/D-064)
     removeOwnedMcpJson(projectRoot), removeOwnedCodexProjectMcp(projectRoot), removeOwnedOpencodeProjectMcp(projectRoot)   // each preserves foreign; warnings -> stderr; malformed -> throw (exit 1)
  // 7. gitignore warning (D-039, never edits .gitignore)
     if !gitIgnored(root, ".claude/settings.local.json"): actions.push("warning: .claude/settings.local.json is not git-ignored; add it to .gitignore so local hook wiring is not committed")   // also in dry-run
  actions = manifest.map(legToActionLine) (+ "[dry-run] " in dry-run)
  return
```
`gitIgnored(root, rel)`: `execFileSync("git", ["check-ignore","-q",rel],{cwd:root})`: status 0 ignored; status 1 not ignored -> warn; any other error (not a repo, git missing) -> no warning. Init-time only.

### Node-recording and path-exists preflight (D-082, D-057, D-066; resolves prior OQ-2)
Two separate checks, one decision each:
1. **init (always re-records):** `installStore` validates `nodePath` (absolute, file exists) before any write and throws `NodeNotRecordableError`; `init` (local, cloud attach) catches it at the `bin/unimatrix.js` boundary: `unimatrix init failed: <msg>`, exit 1. This is init's preflight (its re-record step IS the check). Nothing is written (also in `--dry-run`).
2. **Path-exists preflight, `preflightRecordedNode(home, platform)`:** applies to (a) `wire` (does not re-record when `install.json.node` is still valid) and (b) an upgrade that does not re-record node (store rewrite with `prev.node == nodePath`). It reads `install.json.node` and, on win32, also every owned D-047 fallback entry's `command` found in `~/.claude.json` projects[root], `<codexHome>/config.toml`, `~/.config/opencode/opencode.json`. Any recorded node path that does not exist -> `Error("unimatrix: recorded node <path> missing; run 'npx unimatrix init'")` -> exit 1 BEFORE any write. On non-win32 only `install.json.node` is checked (the sh shim falls back to PATH node at fire time, so a missing recorded node is not fatal there; the check applies only when `install.json` exists and no PATH node is found: `onPath("node")` false -> exit 1; otherwise a warning). init is never subject to check 2 (check 1 supersedes it).

### Detection
`detectHarnesses` keeps the project markers for OpenCode (`.opencode/`/`opencode.json`; Claude always). Codex is NOT detected here any more: `codex-install.detectsCodex` (D-081: project `.codex/`, existing `$CODEX_HOME` dir, `codex` on PATH, or `--harness codex-cli`) is the single rule for Codex hooks AND MCP. Resolves prior OQ-5 (Codex-only users without project `.codex/` now get hooks).

## init.js
```
init(options):
  if bundle/remote/token: return initRemote(opts)                    // cloud attach shares the same wire() call
  projectRoot, dryRun as before
  if process.platform != "linux": throw Error("local init requires Linux; macOS/Windows are cloud-only (use `init --bundle <bundle>`)")   // D-032
  binaryPath = resolveBinary()                                       // wrapper: allowSelf
  installSkills(...) unchanged (--force stays definitions-only)
  wireResult = wire(projectRoot, {binaryPath, dryRun})               // no clientPath: hooks never reference the client path
  DB pre-create + validate (Linux local, unchanged execFileSync of the binary)
  print summary + codexApprovalStep() (codex-install.js) when any codex hooks leg exists (init stdout; writes no trust hashes, D-028)
initRemote(opts):
  existing credstore write / slug / bundle handling unchanged (writes ~/.unimatrix/<hash>/remote.json)
  wire(projectRoot, {mcp: true /* credential present */, dryRun})    // replaces writeMcpBridgeEntry + buildHookClientCommand path
  cleanStaleRemoteSubtree unchanged
```
Removed from init: `writeMcpJson`, `writeMcpBridgeEntry` (they wrote a committed `.mcp.json`), the `resolveClientPath` use in hook wiring (`resolveClientPath` stays exported if tests need it), `maybeProvisionOpenCode` re-export. Their tests are updated to the per-machine destinations rather than deleted (AC-12).
Exit behaviour: `UnsafeShimPathError`, `ClaudeJsonError`, malformed-config errors, trust self-check failure propagate to `bin/unimatrix.js` catch: `unimatrix init failed: <msg>`, exit 1. Warnings (D-065, AC-16 foreign named `unimatrix`, gitignore) never change the exit code.

## Dry-run contract
Every writer takes `dryRun` and returns the same legs as a real run (`[dry-run] ` prefix only). Includes: `~/.unimatrix/**` files, `~/.claude/settings.json` removal, `~/.claude.json`, `<codexHome>/hooks.json`, `<codexHome>/config.toml` (`$CODEX_HOME`, default `~/.codex`), `~/.config/opencode/opencode.json`, project-file removals, gitignore warning. A test asserts the dry-run file list equals the files changed by the real run (R-08).

## Error handling
First failure before writes (unsafe shim path, unparseable `~/.claude.json`/settings/project MCP file) aborts with exit 1 and no partial state from that writer (tmp+rename). Order puts `installStore` first so an unsafe path is refused before any harness config changes.

## Key test scenarios
- AC-01: clean fixture + temp HOME, local (Linux, stub binary) vs cloud: byte-compare `settings.local.json`, `<codexHome>/hooks.json`, `~/.claude.json` project key, `<codexHome>/config.toml`; no project `.mcp.json` created.
- D-082: `wire` with `install.json.node` deleted -> exit 1, no file changed; Windows fallback entry node deleted -> exit 1; `init` with a non-existent `process.execPath` stub -> exit 1; `init` over a deleted recorded node re-records and succeeds.
- D-081: Codex detection triggers (see codex-install.md) wired through `wire`/`init`.
- AC-05 table: L1-L6 seeded in every scope then init -> one entry per event in destination; AC-13 sequences init/wire x2.
- AC-09/AC-16/R-36/R-40 migration fixtures; gitignore warning in init and dry-run, `.gitignore` bytes unchanged; Codex approval step printed; `--force` never reaches wire.
- `wire --harness opencode` writes only user config; unknown `--harness` still help + exit code (existing behaviour retained: usage error path in launcher).
- Update `wire.test.js`, `init.test.js`, `init-remote.test.js`, `init-integration.test.js`, `dogfood-effect.test.js`, `c14-*.test.js` fixtures (HOME-isolated) without reducing counts.
