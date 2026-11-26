# launcher (`bin/unimatrix.js`, new `lib/hook-route.js`, new `lib/status.js`) — JS

## Purpose
Route `hook`, `mcp-stdio`, `status` in Node; keep init/wire/mcp-bridge; Rust pass-through for everything else on Linux (ADR-004). `bin/unimatrix.js` stays small (283 + ~40 lines); hook/mcp-stdio logic in `lib/hook-route.js`, status in `lib/status.js` (PL-10).

## bin/unimatrix.js changes
```
exports: run(args, ctx = {}) -> Promise<void> | void       // ctx.preflight = {project, trustChecked, install, decision} from shim-entry
bottom: if (require.main === module) run(process.argv.slice(2))      // replaces the unconditional main()
```
Existing `init`, `wire`, `mcp-bridge` branches move into `run` unchanged except: `mcp-bridge` headless detection delegated to headless gate (ci-and-docs Part B); `routeWire` resolves transport through the resolver (see wire-init) — `resolveWireTransport` no longer calls `resolveBinary()` blindly (null is normal on macOS/Windows).
```
function run(args, ctx):
  switch args[0]:
    "init" | "wire" | "mcp-bridge": existing code paths
    "hook":      return hookRoute.run(args, ctx)
    "mcp-stdio": return hookRoute.runMcpStdio(args, ctx)
    "status":    return status.run(args)           // exit 0 always
    default:     return passThrough(args)
function passThrough(args):
  if process.platform != "linux": stderr(`unimatrix: ${args[0]} requires the local server (Linux only)`); exitCode = 1; return    // FR-02, D-071
  try rt = resolveRuntime({env, home, platform, transport:"local"})     // trust check first when project/redirect present: checkTrust(trustSet) -> TrustError => exit 1
  catch RuntimeResolutionError e: stderr(e.message + " (sources tried: UNIMATRIX_BINARY, redirect, install)"); exitCode = 1; return
  existing execFileSync(rt.binary, args, {stdio:"inherit", env: LD_LIBRARY_PATH = dirname(binary):existing})   // behaviour identical to today incl. status propagation
```
Pass-through applies the trust check to redirect.json (the redirect is the only user-controlled pointer here) before resolving.

## lib/hook-route.js
```
async function run(args, ctx):                      // args = ["hook", EVENT, ...rest]
  env = process.env; home = os.homedir(); platform = process.platform
  pre = ctx.preflight
  project = pre ? pre.project : isUnimatrixProject(process.cwd(), env, home)
  if project == null: return                         // exit 0 silent (D-051 incl. cloned repo, D-052 headless outside root)
  if !(pre && pre.trustChecked):
     try checkTrust(trustSet(home, {installJson: readInstallJson(home)}), platform, {home}) catch TrustError e: stderr(e.message); exitCode=1; return
  d = pre ? pre.decision : isInstalled(project, platform, {env, home})
  if !d.installed:
     if platform == "linux": stderr("unimatrix: not attached on this machine; run 'npx unimatrix init'")   // exactly one line
     return                                          // exit 0
  try rt = resolveRuntime({env: effectiveEnv(env, process.cwd(), home) /* PENDING H1: else `env` */, home, platform, transport: d.transport})
  catch RuntimeResolutionError e: stderr(e.message); exitCode = 1; return     // Linux local + unresolvable binary lands here (Broken)
  try client = require(join(rt.clientRoot, "lib", "hook-client", "index.js"))
  catch e: stderr(`unimatrix: cannot load hook client at ${rt.clientRoot}: ${e.message}`); exitCode = 1; return
  argv = ["node", join(rt.clientRoot,"lib","hook-client","index.js"), ...args.slice(1)]   // argv[2] = EVENT, rest incl --provider (client convention)
  try await client.main(argv)                       // main never sets a nonzero code for connection errors (D-042)
  catch e: stderr("unimatrix: hook client error: " + e.message); exitCode = 1
  // never process.exit(); never exitCode = 2; hint/errors stderr only
```
```
async function runMcpStdio(args, ctx):                // R-07: no stdout bytes before the child/bridge starts
  project = pre?.project ?? isUnimatrixProject(cwd, env, home)
  if project == null: return                          // exit 0 silent, fast (R-39)
  trust check as above (stderr, exit 1)
  d = pre?.decision ?? isInstalled(...)
  if !d.installed: stderr("unimatrix: not attached on this machine; run 'npx unimatrix init'"); exitCode = 1; return    // D-043, every OS
  try rt = resolveRuntime({... transport: d.transport}) catch -> stderr, exit 1
  if d.transport == "cloud":
     require(join(rt.clientRoot,"lib","hook-client","mcp-bridge.js")).main(["node","mcp-bridge", project.hash], {env: effectiveEnv(env, cwd, home) /* PENDING H1: else `env` */})
     return                                           // bridge owns stdio loop and exits
  // local (Linux) (D-077): Node has no exec. Spawn the Rust binary with INHERITED stdio and mirror the exit code (D-086: Windows mirrors exit code only; POSIX also re-raises the child's signal).
  // ADR-004's "exec" is satisfied by this semantic; no ADR change. Nothing is written to stdout before the spawn (R-07);
  // the only pre-spawn output allowed is stderr on failure.
  childEnv = {...env, LD_LIBRARY_PATH: dirname(rt.binary) + (env.LD_LIBRARY_PATH ? ":"+env.LD_LIBRARY_PATH : "")}
  child = spawn(rt.binary, [], {stdio: "inherit", env: childEnv})            // no shell, no args (matches args: [] of the old entry)
  forward = sig => { try child.kill(sig) catch {} }
  for sig in ["SIGINT","SIGTERM","SIGHUP"]: process.on(sig, () => forward(sig))   // parent never dies first; the child decides
  child.on("error", e => { stderr("unimatrix: cannot start " + rt.binary + ": " + e.message); process.exitCode = 1 })   // ENOENT/EACCES
  child.on("exit", (code, sig) => {
     if platform == "win32":                          // D-086: no POSIX signals on Windows; exit code only
        process.exitCode = code ?? 1
     else if sig != null:                             // POSIX: child killed by signal: re-raise the SAME signal on ourselves after removing our handlers,
        for s in ["SIGINT","SIGTERM","SIGHUP"]: process.removeAllListeners(s)   // so our own exit status shows the signal (shell sees 128+n / WIFSIGNALED)
        process.kill(process.pid, sig)
        process.exitCode = 128 + signalNumber(sig)    // only reached if the signal is ignored/uncatchable on re-raise
     else process.exitCode = code                     // mirrored exactly (never remapped, never 2 added by us)
  })
  return new Promise(resolve => child.on("close", resolve))   // keep the in-process caller alive until the child is done
```
Rust stdio server is started with no args (matches existing `writeMcpJson` entry `args: []`). Project cwd identification for the Rust server is by its cwd (unchanged).

## lib/status.js (D-038, D-050, D-051, D-068; no network, no writes, always exit 0)
```
function run(args):
  json = args.includes("--json")
  info = collect()            // wrapped in try/catch per field; any throw becomes that field's "unknown"/failure value; exit stays 0
  print(json ? JSON.stringify(info) : renderText(info))
function collect():
  cwd = process.cwd(); root = walkToProjectRoot(cwd)
  project = isUnimatrixProject(cwd, env, home)                      // {kind, hash, root} | null
  legacy = []                                                       // order: .mcp.json, .codex/config.toml, opencode.json (project-relative, project root = root)
    for file in order: if projectFileHasOwnedMcp(root, file) legacy.push(file)       // uses hook-ownership.isUnimatrixMcpEntry; toml via codex-install.readTomlTable body->entry; parse failures -> not listed
  d = project ? isInstalled(project, platform, {env, home}) : {installed:false, transport:null, reason: project ? ... : "no install.json"}
  trust = try checkTrust(trustSet(...)) -> {ok:true} catch TrustError e -> {ok:false, path:e.path, reason:e.reason}     // for win32 per-event path; informational
  mcpNode = platform=="win32": find owned D-047 fallback entry in ~/.claude.json projects[root].mcpServers.unimatrix or <codexHome>/config.toml; present -> exists(command) ? {status:"ok",path} : {status:"missing",path}; none -> {status:"n/a",path:null}.  non-win32 -> "n/a"
  hooks["claude-code"].wired = any owned command in <root>/.claude/settings.local.json (also checks isUnimatrixCommand); lastFired = read last-hook.json[provider] (corrupt/missing -> null; corrupt reported as "unknown")
  hooks["codex-cli"].wired  = any owned command in <codexHome>/hooks.json (codexHome(env, home) from codex-install.js, D-081; default ~/.codex) ; lastFired from last-hook.json["codex-cli"]
```
Text order (exact keys): `project:`, zero or more `legacy: <file> has a Unimatrix MCP entry; re-run 'npx unimatrix init' to migrate`, `installed: yes|no[ (reason)]`, `transport: local|cloud|none`, `trust: ok|FAIL <path>: <reason>`, `mcp node: ok|n/a|MISSING <path>; run 'npx unimatrix init'`, `hook claude-code: wired, last fired <ISO>|wired, never fired|not wired`, `hook codex-cli: ...` followed (only for `wired, never fired`) by a line with the Codex `/hooks` approval step (same text as init prints: `codexApprovalStep()` exported from codex-install.js, no approval key named). `project:` when null: `none (this machine has no Unimatrix store or credential for <root>; run 'npx unimatrix init' to attach)`. `installed:` reason `missing credential` (macOS/Windows) or `no install.json`. JSON shape per brief. Never reads Codex trust hashes.

## Exit outcome table implemented (ADR-004)
not a project -> 0 silent; Linux project not installed -> 0 + one stderr line; macOS/Windows not installed -> 0 silent; resolution/trust/node failure -> 1 stderr; connection errors -> client's existing codes; `mcp-stdio` inside project not installed -> 1 + hint, outside -> 0 silent; pass-through macOS/Windows -> message + 1.

## Error handling
Every thrown error is caught at the route boundary: stderr line, `process.exitCode = 1` (never 2, never uncaught). Hint text constants in one place (`lib/hook-route.js` exports `NOT_ATTACHED_HINT`).

## Key test scenarios
- Exit matrix {not project, cloned repo, mac/Win no credential, Linux store no install, Linux installed no binary, trust failure, invalid redirect, client throw sync/async, connection error} x {hook, mcp-stdio} x OS (mock platform).
- `unimatrix hook E` legacy L1 route with stub Rust binary: Node client executed, stub never exec'd (AC-03). Pass-through regression: `serve`/`version` still exec Rust with `LD_LIBRARY_PATH` on Linux (SR-12); override matrix (AC-14).
- `status`: cloned repo -> `project: none (...)`; leftover legacy entries in each project file -> `legacy:` lines in order and `legacyProjectMcp`; Codex wired-never-fired prints approval step; stale Windows fallback node -> `mcp node: MISSING`; always exit 0 even with everything broken; corrupt `last-hook.json` does not crash.
- `mcp-stdio` local (D-077): stub Rust binary exiting with code N -> launcher exits N; POSIX: stub killed by SIGTERM/SIGKILL (self-signal) -> launcher process terminates by that same signal; Windows: exit code only, no signal assertion (D-086) (assert via a parent harness: `signal` field in `child_process` result); SIGINT sent to the launcher reaches the stub; stdout bytes captured by a pipe before the stub writes its first byte = 0 (no Node stdout bytes before spawn); spawn ENOENT -> stderr + exit 1.
- `mcp-stdio`: first stdout byte is JSON-RPC `{` from the child; nothing written before; outside project exit 0 empty output, zero network; headless env + cwd outside root -> exit 0 silent.
