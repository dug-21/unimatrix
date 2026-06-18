# C4 — `lib/init.js` `initRemote()` (MODIFIED, Scope A+B)

**Purpose.** On the bundle path: write the credential via `credstore` (B) instead of
`writeRemoteSettingsLocal`; write the stdio `.mcp.json` bridge entry (A) instead of the
"Skipped .mcp.json" line. On the legacy path: emit a loud, deterministic unsupported message and
wire NO bridge. Remove `gitignoreWarning`. Delete a stale in-tree `unimatrix.remote` subtree on
bundle init. Sources: ADR-002 (entrypoint), ADR-003/004 (store), ADR-005 (bundle-only). Risks:
R-09,R-10,R-11,R-12,R-15.

New imports: `const credstore = require("./hook-client/credstore.js");` and
`const { computeProjectHash } = require("./hook-client/config.js");`.

## Distinguish bundle vs legacy

`resolveRemoteTarget(options)` (unchanged, `init.js:331`) already returns
`{ mcpUrl, observeUrl, token, pinnedFp }` where `pinnedFp` is the bundle `fp` on the bundle path and
`null` on the legacy `{remote,token}` path. Use the same discriminator the function uses:

```
isBundlePath = !!options.bundle      // true => bundle (pinned); false => legacy (--remote/--token)
```

`pinnedFp` being non-null is equivalent to the bundle path; `isBundlePath` is the explicit signal.

---

## New helper: `writeMcpBridgeEntry(projectRoot, bridgePath, projectHash, dryRun) -> string[]`

A remote analogue of `writeMcpJson` (`init.js:59-104`) — same idempotent, merge-preserving,
dry-run-aware, malformed-throws contract (R-10/AC-07). Writes the TOKEN-FREE stdio entry (AC-09).

```
function writeMcpBridgeEntry(projectRoot, bridgePath, projectHash, dryRun):
    mcpPath = path.join(projectRoot, ".mcp.json")
    actions = []
    existing = {}
    if fs.existsSync(mcpPath):
        try: existing = JSON.parse(fs.readFileSync(mcpPath, "utf8"))
        catch parseError: throw Error("Malformed .mcp.json at " + mcpPath + ": " + parseError.message +
                                      "\nFix the JSON syntax and re-run 'npx unimatrix init'.")   // R-10
        actions.push("Updated .mcp.json (preserved existing servers)")
    else:
        actions.push("Created .mcp.json")
    if not existing.mcpServers: existing.mcpServers = {}

    // TOKEN-FREE stdio entry (AC-09 / FR-17): command node, args [bridgePath, projectHash], env {}.
    // No token, no mcp_url, no fp — the bridge resolves the credential from the store by projectHash.
    existing.mcpServers.unimatrix = {
        command: "node",
        args: [bridgePath, projectHash],
        env: {},
    }

    if not dryRun:
        fs.writeFileSync(mcpPath, JSON.stringify(existing, null, 2) + "\n", "utf8")
    else:
        actions[last] = "[dry-run] " + actions[last]
    return actions
```

`bridgePath` is resolved per-install via `require.resolve` (ADR-002), mirroring the hook-client path
resolve at `init.js:409`:
```
try: bridgePath = require.resolve("./hook-client/mcp-bridge.js")
catch: bridgePath = path.join(__dirname, "hook-client", "mcp-bridge.js")
```

---

## New helper: `cleanStaleRemoteSubtree(projectRoot, dryRun) -> string[]`

Best-effort deletion of a stale in-tree `unimatrix.remote` subtree from
`.claude/settings.local.json` (ADR-004 §migration / OQ-5 residual). Merge-preserving: only
`unimatrix.remote` is removed; other `unimatrix.*` and Claude Code keys survive. Failure does NOT
abort init (R-12 §migration).

```
function cleanStaleRemoteSubtree(projectRoot, dryRun):
    actions = []
    slPath = path.join(projectRoot, ".claude", "settings.local.json")
    if not fs.existsSync(slPath): return actions
    try:
        parsed = readJsonOrEmpty(slPath, ".claude/settings.local.json")   // throws on malformed
        if parsed.unimatrix and parsed.unimatrix.remote is present:
            if dryRun:
                actions.push("[dry-run] Would remove stale unimatrix.remote from .claude/settings.local.json")
            else:
                delete parsed.unimatrix.remote
                // if unimatrix object is now empty, leave it (harmless) or delete — leave for simplicity
                fs.writeFileSync(slPath, JSON.stringify(parsed, null, 2) + "\n", "utf8")
                actions.push("Removed stale unimatrix.remote credential from .claude/settings.local.json")
    catch e:
        // best-effort: a malformed settings.local.json must not block relocation; note and continue
        actions.push("Note: could not clean stale .claude/settings.local.json (" + e.message + ")")
    return actions
```
> NOTE: removing the stale subtree closes the commit-leak at root (the human-favored direction,
> ARCHITECTURE §11 OQ-2). If the human flips to non-destructive (Open Question 2), this helper just
> stops writing and the stale file is left — a one-line change.

---

## Modified `initRemote(options)` flow

Replace Steps 3, 3b, and the "Skipped .mcp.json" line. Keep Steps 0,1,4,5(skills),6(Ping) intact.

```
async function initRemote(options):
    dryRun = options?.dryRun || false
    actions = []

    // Step 0 (unchanged): resolve target — LOUD on bad input (resolveRemoteTarget throws BundleError)
    target = resolveRemoteTarget(options)
    { mcpUrl, observeUrl, token, pinnedFp } = target
    isBundlePath = !!options.bundle

    // Step 1 (unchanged): project root (throwing detectProjectRoot)
    projectRoot = options.projectDir ? path.resolve(options.projectDir) : detectProjectRoot(process.cwd())
    actions.push("Project root: " + projectRoot)

    // Step 1b (NEW): derive the store key — SAME oracle both consumers use (R-07, ADR-003)
    projectHash = computeProjectHash(projectRoot)     // config.js:123 — one derivation, cannot disagree

    // Step 2 (unchanged): resolve installed hook-client path for the hooks merge (Step 4)
    clientPath = require.resolve("./hook-client/index.js")  (fallback to computed path)

    // ---- LEGACY PATH (bundle-only boundary, ADR-005 / AC-10) ----
    if not isBundlePath:
        // Scope B relocation is UNIVERSAL: legacy credential ALSO moves out of tree, fingerprint:null
        // (R-15). Hook client stays unpinned for legacy (preserves today's behavior).
        actions.push(...credstore.write(projectHash, {
            mcp_url: mcpUrl, observe_url: observeUrl, token, fingerprint: null,
        }, { dryRun }))
        // No bridge entry. Clean any stale in-tree creds.
        actions.push(...cleanStaleRemoteSubtree(projectRoot, dryRun))
        // LOUD, DETERMINISTIC unsupported message (AC-10, R-11). Exact text is a testable AC:
        actions.push(LEGACY_MCP_UNSUPPORTED_MESSAGE)
        // continue to hooks + skills + Ping (legacy observe path is UNCHANGED — still works)

    // ---- BUNDLE PATH (Scope A + B) ----
    else:
        // Step 3 (CHANGED): write credential via credstore (B) instead of writeRemoteSettingsLocal.
        // Bundle path carries a real fingerprint -> hook client pins (ADR-004 fix).
        actions.push(...credstore.write(projectHash, {
            mcp_url: mcpUrl, observe_url: observeUrl, token, fingerprint: pinnedFp,
        }, { dryRun }))

        // Step 3a (NEW): delete stale in-tree unimatrix.remote subtree (ADR-004 §migration)
        actions.push(...cleanStaleRemoteSubtree(projectRoot, dryRun))

        // Step 3b (CHANGED): write the stdio .mcp.json bridge entry (A) — replaces the
        // "Skipped .mcp.json" line. Token-free (AC-09).
        bridgePath = require.resolve("./hook-client/mcp-bridge.js") (fallback computed)
        actions.push(...writeMcpBridgeEntry(projectRoot, bridgePath, projectHash, dryRun))

    // Step 3c (REMOVED): gitignoreWarning is GONE — no in-tree creds file to warn about (FR-25/AC-08).

    // Step 4 (unchanged): merge hooks (full event set; command `node <clientPath> <EVENT>`).
    settingsPath = path.join(projectRoot, ".claude", "settings.json")
    settingsResult = mergeSettings(settingsPath, { events: HOOK_EVENTS,
        commandForEvent: e => buildHookClientCommand(clientPath, e) }, { dryRun })
    actions.push(...settingsResult.actions)

    // Step 5 (unchanged): copy skills. REMOVE the two old lines:
    //   "Skipped .mcp.json: remote mode does not register a local MCP server"   <- gone (bundle path)
    //   keep "Skipped binary/database steps: no local binary in remote mode"
    actions.push(...copySkills(projectRoot, dryRun))
    actions.push("Skipped binary/database steps: no local binary in remote mode")

    // Step 6 (unchanged): Ping validation over the PINNED TLS connection (observe_url verbatim).
    // pinnedFp is the bundle fp (pinned) or null (legacy unpinned) — preserved exactly.
    if not dryRun:
        res = await transport.pingForInit(observeUrl, token, undefined, pinnedFp)
        if not res.ok: throw Error("Remote validation failed: " + res.message + "\n...")
        actions.push("Ping OK: " + res.message)
    else:
        actions.push("[dry-run] Would Ping " + hostOf(observeUrl))

    printSummary(actions, dryRun)
```

## Constant: LEGACY_MCP_UNSUPPORTED_MESSAGE (AC-10, R-11 — deterministic, testable)

A fixed module-level string (exact wording is the testable AC; example):
```
LEGACY_MCP_UNSUPPORTED_MESSAGE =
  "Cloud MCP is unsupported on the legacy --remote/--token path: it requires a v:2 bundle " +
  "(run `unimatrix client-bundle` on the server, then `init --bundle <bundle>`). " +
  "No MCP server was wired. The observe/telemetry path still works."
```
Loud but NOT a hard failure — init still succeeds (exit 0) on the legacy path; the message is an
action line, not a throw (legacy observe is intentionally preserved). The gate asserts the EXACT
text and the exit behavior (SR-06).

## Removed / retired

- `writeRemoteSettingsLocal` call in `initRemote` — replaced by `credstore.write`. (The function may
  remain exported for now if other callers/tests reference it, but `initRemote` no longer calls it;
  flag for removal if unreferenced. See gap G-C4-1.)
- `gitignoreWarning(projectRoot)` call (Step 3b old) — removed (FR-25/AC-08). The function may be
  deleted if unreferenced elsewhere (flag G-C4-2).
- The "Skipped .mcp.json: remote mode does not register a local MCP server" action line — removed on
  the bundle path (AC-01); the bundle path now writes a real entry.

## Data flow

IN: `options` `{ bundle | remote+token, projectDir?, dryRun }`.
OUT (bundle): `credstore.write` → `remote.json` (0600, fingerprint set); `.mcp.json` bridge entry
(token-free); hooks; skills; pinned Ping. OUT (legacy): `credstore.write` (fingerprint null); no
`.mcp.json` entry; loud message; hooks; skills; unpinned Ping.

## Error handling (R-09, R-10, R-12)

| Origin | Behavior |
|--------|----------|
| `resolveRemoteTarget` BundleError | throw → init exit 1 (token-free message) |
| `credstore.write` failure | throw → init exit 1 (creds must persist; no token-bearing partial in tree — R-12) |
| malformed existing `.mcp.json` | throw (mirrors `writeMcpJson`) — no silent overwrite (R-10) |
| malformed `.claude/settings.local.json` during clean | best-effort note, continue (migration non-fatal) |
| Ping failure | throw → init exit 1 (config written; fix + re-run) — unchanged |
| token in any action/summary line | MUST be absent (NFR-06/R-09) — `.mcp.json` and store action lines carry no token |

## Key test scenarios (hints; full plan in test-plan/init-remote.md)

- **AC-01**: after `init --bundle`, `.mcp.json` has a stdio `unimatrix` entry
  `{command:"node", args:[<bridge path>, <projectHash>], env:{}}`; no "Skipped .mcp.json" line.
- **AC-07/R-10**: pre-seed `.mcp.json` with a co-resident server; run `init` twice → `unimatrix` not
  duplicated, co-resident preserved; `--dry-run` → no write; malformed `.mcp.json` → throws.
- **AC-08/R-12**: store file out-of-tree at 0600 with the token; `git status --porcelain` /
  `git add -A` dry-run lists no token-bearing path; `.claude/settings.local.json` has no
  `unimatrix.remote` after init.
- **AC-08b/R-07**: write-key === `computeProjectHash(projectRoot)`; two projects → two hash dirs.
- **AC-09/R-09**: no token in `printSummary` output, `.mcp.json`, or any action line.
- **AC-10/R-11**: `init --remote/--token` → no `unimatrix` `.mcp.json` entry; EXACT
  `LEGACY_MCP_UNSUPPORTED_MESSAGE` text asserted; init exit behavior asserted; legacy observe still works.
- **R-12 migration**: pre-seed stale `unimatrix.remote` in `settings.local.json`; bundle `init` →
  subtree deleted, other keys survive, no in-tree token remains.
- **R-15**: legacy path writes `fingerprint: null` to the store (not omitted); bundle path writes the
  real `fingerprint`.
- **FR-25**: no gitignore-warning line emitted.

## Gaps (flagged)

- **G-C4-1:** `writeRemoteSettingsLocal` becomes unused by `initRemote`. Decide in delivery whether
  to delete it (and its tests) or keep exported for compatibility. Not blocking.
- **G-C4-2:** `gitignoreWarning` likewise becomes unused — delete if unreferenced. Not blocking.
