# shim-entry (`lib/shim-entry.js` -> `~/.unimatrix/bin/shim-entry.js`) — JS

## Purpose
Version-independent entry run by the shim (and by the Windows D-047 MCP fallback: `<node> <home>/.unimatrix/bin/shim-entry.js mcp-stdio`). Order (ADR-002/004): inline project predicate -> trust check -> inline installed decision -> read `install.json` -> `require(<packageRoot>/bin/unimatrix.js).run(args, {preflight})` in-process. One Node process per event.

## Constraints
- Only Node builtins. No `require` of package code before trust passes. Trust code is loaded from the sibling `./trust.js` (copied by install-store); everything else comes from `packageRoot` after the check.
- Installed layout: `~/.unimatrix/bin/shim-entry.js`; home dir = `path.resolve(__dirname, "..", "..")`, uhome = `path.resolve(__dirname, "..")`. HOME for predicate = `os.homedir()` (tests override via env HOME/USERPROFILE).
- Must be a file with a testable inline function: `module.exports = { isUnimatrixProject, installedDecision, run }` and `if (require.main === module) run(process.argv.slice(2))` so the parity test imports the built copy (R-28).
- Inline copies: `walkToProjectRoot`, `resolveGitFile`, `realpathOrSelf`, `computeProjectHash` (sha256 first 16 hex), `headlessEnvSet`, `headlessRootContains`, `isUnimatrixProject` — byte-semantic copies of project-state.md / config.js.

## Pseudocode
```
function run(args):                       // args = e.g. ["hook","SessionStart"] or ["mcp-stdio"]
  home = os.homedir(); uh = join(home,".unimatrix"); platform = process.platform; env = process.env
  verb = args[0]
  project = isUnimatrixProject(process.cwd(), env, home)       // inline
  if project == null: return                                    // exit 0 silent, BEFORE trust/require (R-06); process.exitCode untouched
  inst = readInstallJsonRaw(uh)                                 // may be null; no validation beyond JSON shape
  try:
     trust = require("./trust.js")
     trust.checkTrust(trust.trustSet(home, {installJson: inst}), platform, {home})
  catch e (TrustError or require failure): stderr(e.message); process.exitCode = 1; return
  d = installedDecision(project, platform, env, home, inst)     // inline: {installed, transport, reason}
  if !d.installed:
     if verb == "mcp-stdio": stderr("unimatrix: not attached on this machine; run 'npx unimatrix init'"); process.exitCode = 1; return     // D-043 (any OS)
     if platform == "linux": stderr("unimatrix: not attached on this machine; run 'npx unimatrix init'")   // D-023 one line
     return                                                     // exit 0 (macOS/Windows silent, D-050)
  if inst == null or typeof inst.packageRoot != "string":       // cloud/bundle without a package copy: client unreachable
     stderr("unimatrix: install pointer missing or invalid; run 'npx unimatrix init'"); process.exitCode = 1; return
  try:
     launcher = require(join(inst.packageRoot, "bin", "unimatrix.js"))
  catch e: stderr("unimatrix: cannot load " + inst.packageRoot + ": " + e.message); process.exitCode = 1; return
  Promise.resolve(launcher.run(args, {preflight: {project, trustChecked: true, install: inst, decision: d}}))
     .catch(e => { stderr("unimatrix: " + e.message); process.exitCode = 1 })
  // no process.exit(): let the event loop drain (R-04)
```
`installedDecision` is the exact `isInstalled` logic of project-state.md, using `inst` shape validity (`version`, `packageRoot`, `node` strings; `fs.existsSync(packageRoot)`) instead of requiring package code.

Redirect override: shim-entry always loads the INSTALLED launcher; if `redirect.client` is set the launcher's `run` resolves the client via `resolveRuntime` (so the redirect client governs hook execution, not the launcher itself). The redirect file is in the trust set (via `trustSet`).

## Error handling
Any failure before the launcher: stderr + exit 1 (inside a project). Outside a project nothing prints, even with stale install, missing node, broken trust (FR-12a0). Never exit 2; never write stdout (R-01, R-07).

## Key test scenarios
- Parity table vs `lib/project-state.js` and `config.js` hash, importing the built `shim-entry.js` copy.
- `require` trace: outside a project with every trust input broken -> no package module loaded.
- Inside project: group-writable install.json -> exit 1, stderr names path; `packageRoot` deleted -> exit 1; Linux store without install -> exit 0 + one stderr line; macOS/Windows w/o credential -> exit 0 silent; `mcp-stdio` not installed -> exit 1 with hint, empty stdout.
- Same event via `node hook-client/index.js E`, `bin/unimatrix.js hook E`, literal shim command stores an identical record (R-04).
