# init-remote — `init --remote` Branch + merge-settings Generalization

## Purpose
Modify `lib/init.js` (remote branch + HOOK_EVENTS fix) and `lib/merge-settings.js`
(ownership patterns + `commandSource` generalization + HOOK_EVENTS/matchers fix), plus
the `bin/unimatrix.js` flag plumbing. Includes the DESIGN-LEVEL FIX for the
ownership-regex spaced-path defect (Alignment WARN 2 — the only remaining open gate note).

---

## 1. Ownership-Pattern Spaced-Path Fix (WARN 2 — resolved here)

### require.resolve output shapes (confirmed per platform)
`require.resolve("./hook-client/index.js")` from `lib/init.js` returns an absolute,
platform-native path:
- **Linux/macOS**: `/abs/.../node_modules/@dug-21/unimatrix/lib/hook-client/index.js`
  (forward slashes; MAY contain spaces — `/Users/d/My Projects/...`).
- **Windows**: `C:\Users\...\node_modules\@dug-21\unimatrix\lib\hook-client\index.js`
  (drive letter + backslashes; MAY contain spaces — `C:\Program Files\...`); UNC form
  `\\server\share\...` possible on network installs.

### Defect
Proposed pattern `/(^|\s|\/)node\s+\S*\/hook-client\/index\.js\s/` fails because `\S*`
cannot cross the space inside a spaced install path, and `\/` never matches backslashes.

### Fix (two coordinated pieces)

**(a) Command builder quotes spaced paths** — the command we write is deterministic:
```js
function buildHookClientCommand(clientPath, event) {
  const quoted = /\s/.test(clientPath) ? '"' + clientPath + '"' : clientPath;
  return "node " + quoted + " " + event;
}
```
(Double quotes work for POSIX shells and Windows cmd; settings.json hook commands are
executed via shell, so an unquoted spaced path would mis-parse anyway — quoting is
required for execution correctness, not only for the regex.)

**(b) Ownership pattern matches both forms and both separators.** Append ONE regex to
`UNIMATRIX_PATTERNS` (existing four legacy patterns unchanged):
```js
// node <path>/hook-client/index.js <EVENT> — quoted or bare, / or \ separators,
// spaced paths allowed. Anchors: a `node` token, then a path whose LAST segments are
// hook-client/index.js, then whitespace (the event argument always follows).
/(^|[\s"'/\\])node(\.exe)?\s+("[^"]*[/\\]hook-client[/\\]index\.js"|'[^']*[/\\]hook-client[/\\]index\.js'|\S+[/\\]hook-client[/\\]index\.js)\s/
```
Three alternations: double-quoted path (spaces ok), single-quoted path (spaces ok),
unquoted path (`\S+` — correct because WE only write unquoted when the path has no
whitespace). `node.exe` accepted defensively. A foreign `node something-else.js`
command does NOT match (no `hook-client/index.js` tail); a command merely *mentioning*
the path as a later argument does not match the `node\s+<path>` adjacency.

### Positive/negative table (unit-test fixture, R-11)
```
MATCH:  node /a/b/lib/hook-client/index.js SessionStart
MATCH:  node "/Users/d/My Projects/n/lib/hook-client/index.js" Stop
MATCH:  node "C:\Program Files\n\lib\hook-client\index.js" PreCompact
MATCH:  node C:\u\lib\hook-client\index.js PostToolUse
MATCH:  /usr/bin/env node '/a b/lib/hook-client/index.js' SubagentStart   (leading wrapper)
NO:     node /a/b/lib/other-client/index.js Stop
NO:     node script.js /a/hook-client/index.js Stop          (path not adjacent to node)
NO:     unimatrix hook SessionStart                          (matches legacy pattern instead — intended)
NO:     echo hook-client/index.js
```

---

## 2. merge-settings.js Changes

### HOOK_EVENTS + EVENT_MATCHERS (FR-21 — local AND remote, 9 events)
```js
const HOOK_EVENTS = ["SessionStart","Stop","UserPromptSubmit","PreToolUse",
  "PostToolUse","PostToolUseFailure","PreCompact","SubagentStart","SubagentStop"];
EVENT_MATCHERS += { PostToolUseFailure: "*", PreCompact: "" }   // others unchanged
```
This is the ONLY local-mode behavior change (SR-07/C-10 blast radius).

### mergeSettings generalization (back-compat preserved)
```js
// New signature: mergeSettings(filePath, commandSource, options)
// commandSource = { events: string[], commandForEvent(event) -> string }
function mergeSettings(filePath, commandSource, options):
  source = normalizeCommandSource(commandSource)
  // ... existing Steps 1-2 unchanged (read, validate hooks object) ...
  // Step 3 loop becomes:
  for (const event of source.events) {
    const hookCommand = source.commandForEvent(event)
    // ... identical matcher-group merge/dedup logic, isUnimatrixHook unchanged
    //     except UNIMATRIX_PATTERNS now contains the new node-client pattern,
    //     so re-runs REPLACE old-style `unimatrix hook` entries when switching
    //     modes and recognize node-client entries (SR-08) ...
  }
  // Step 4 write unchanged.

function normalizeCommandSource(cs):
  if (typeof cs === "string") {                 // BACK-COMPAT: legacy binaryPath call site
    const binDir = path.dirname(cs)
    return { events: HOOK_EVENTS,
             commandForEvent: e => "LD_LIBRARY_PATH=" + binDir + " " + cs + " hook " + e }
    // byte-identical command strings to the current implementation (integration risk:
    // local init flow must produce byte-identical settings output — except the two NEW
    // events FR-21 adds, which is the intended fix)
  }
  return cs
```
Remote call site passes:
```js
{ events: HOOK_EVENTS,        // same 9 — remote set == local set
  commandForEvent: e => buildHookClientCommand(clientPath, e) }
```
Exports gain `buildHookClientCommand` (init.js + tests consume it).

---

## 3. init.js Remote Branch

### bin/unimatrix.js flag plumbing
```js
// inside the existing args[0] === "init" branch:
remote = valueAfter(args, "--remote"); token = valueAfter(args, "--token")
init({ dryRun, projectDir, remote, token })
// token passes through process argv of *init* (interactive, user-typed — RQ-3 forbids
// the token in the HOOK command line / checked-in files, not the init invocation itself)
```

### init(options) — remote mode
```js
async function init(options):
  if (options.remote || options.token):
    return initRemote(options)        // remote branch; local flow untouched below
  // ... existing local steps 1-8, with mergeSettings now called through the
  //     back-compat path: mergeSettings(settingsPath, binaryPath, { dryRun }) ...

async function initRemote({ remote, token, dryRun, projectDir }):
  actions = []
  // Step 0: argument validation — LOUD failures (init is the one loud checkpoint)
  if (!remote || !token) throw Error("--remote and --token are both required")
  u = new URL(remote)  // throws → "invalid --remote URL"; must be http: or https:

  // Step 1: project root (existing throwing detectProjectRoot — correct for init UX)
  projectRoot = options.projectDir ? path.resolve(projectDir) : detectProjectRoot(process.cwd())

  // Step 2: resolve the installed client path
  clientPath = require.resolve("./hook-client/index.js")     // absolute, platform-native

  // Step 3: write settings.local.json unimatrix.remote (ADR-006; FR-18)
  slPath = path.join(projectRoot, ".claude", "settings.local.json")
  existing = readJsonOrEmpty(slPath)        // malformed JSON → THROW with fix-it message
                                            // (same posture as writeMcpJson — never clobber)
  existing.unimatrix = existing.unimatrix || {}
  existing.unimatrix.remote = Object.assign({}, existing.unimatrix.remote,
                                            { url: remote, token: token })
  // merge-preserving: ONLY the unimatrix.remote subtree is touched; other top-level
  // keys (Claude Code's) and other unimatrix.* keys survive verbatim
  if (!dryRun) { mkdir .claude; writeFileSync(slPath, JSON2(existing), {mode:0o600});
                 chmodSync(slPath, 0o600) /* wrapped — Windows no-op */ }
  actions.push("Wrote unimatrix.remote to .claude/settings.local.json (mode 0600)")

  // Step 3b: gitignore warning (best-effort string check, no glob engine)
  giLines = lines of {projectRoot}/.gitignore (missing → [])
  covered = giLines.some(l => l is one of ".claude/settings.local.json",
            "settings.local.json", "**/settings.local.json", ".claude/" , "*.local.json")
  if (!covered) actions.push("WARNING: .claude/settings.local.json is not gitignored — " +
                             "it contains your token; add it to .gitignore")

  // Step 4: merge hooks (FULL 9-event remote set; idempotent; preserves foreign hooks)
  mergeSettings(path.join(projectRoot,".claude","settings.json"),
    { events: HOOK_EVENTS, commandForEvent: e => buildHookClientCommand(clientPath, e) },
    { dryRun })
  // hook command carries ONLY `node <path> <EVENT>` — no URL, no token (RQ-3/R-16)

  // Step 5: explicit skips with messages (FR-20)
  actions.push("Skipped .mcp.json: remote mode does not register a local MCP server")
  actions.push("Skipped binary/database steps: no local binary in remote mode")
  // NO resolveBinary(), NO DB pre-create, NO binary validation, NO skills copy (F5)

  // Step 6: Ping validation — the ONE loud checkpoint (FR-19, ADR-005, R-18)
  if (!dryRun):
    res = await transport.pingForInit(remote, token)   // strict Pong; auth exercised
    if (!res.ok) throw Error("Remote validation failed: " + res.message +
      "\nConfiguration files were written; fix the URL/token and re-run init.")
    actions.push("Ping OK: " + res.message)
  else: actions.push("[dry-run] Would Ping " + u.host)

  printSummary(actions, dryRun)
```

## Error Handling
- init failures THROW → bin catches → stderr + exit 1 (existing pattern). This is
  deliberately opposite to the hook client's exit-0 posture: init is interactive.
- Malformed existing settings.local.json / settings.json: fail with a fix-it message,
  never overwrite user content.
- Ping failure after files were written: error message says so explicitly (re-run is
  idempotent — Step 3/4 merges are replace-in-place).

## Key Test Scenarios (AC-11 matrix + AC-16 + R-11 + R-16)
1. Fresh config → 9 events written, node-command entries, matchers ("" for
   SessionStart/Stop/UserPromptSubmit/PreCompact, "*" for tool/agent events).
2. Re-run idempotency: entries recognized (new pattern), replaced not duplicated; after
   two runs, unimatrix entries per event == 1 (double-fire detection).
3. Mode switch: config with old-style `unimatrix hook` entries → replaced by node form.
4. Foreign hooks preserved, incl. a foreign `node` command that must NOT match.
5. Spaced-path matrix: the positive/negative regex table above, end-to-end on a mock
   install under a path with spaces (Linux + Windows separators).
6. settings.local.json: 0600 mode, merge-preserving (foreign keys + other unimatrix
   keys survive), gitignore warning fires when uncovered; token NEVER appears in
   settings.json or any hook command argv (content scans, R-16).
7. Ping: wrong token → loud `auth` failure (proves Bearer exercised); non-Pong 200 →
   failure; unreachable host → actionable connect message; dry-run skips network write
   steps but reports.
8. AC-16 regression: LOCAL init over a pre-existing 7-event config → full 9-event set
   written + recognized on re-run; diff to local mode is list+matchers only
   (SR-07 blast-radius gate); back-compat wrapper produces byte-identical commands.
