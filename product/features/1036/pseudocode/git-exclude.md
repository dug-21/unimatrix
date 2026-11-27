# C5 — Git exclude ensure (`lib/git-exclude.js`, new)

## Purpose
Keep `.claude/settings.local.json` git-ignored via the per-repo exclude file (D-011, ADR-006 #5943, FR-40..FR-46). Never throws except `SafeWriteError` from the append.

## Export
`ensureSettingsLocalIgnored(root, {dryRun, run?}) -> WireLeg` (harness `"claude-code"`, surface `"git-exclude"` — see Q2).

## Functions

```
REL = ".claude/settings.local.json"
SCRUB = ["GIT_DIR","GIT_WORK_TREE","GIT_INDEX_FILE","GIT_COMMON_DIR","GIT_PREFIX"]

function defaultRun(root) -> (args) => {status, stdout, error?}
  env = copy of process.env without SCRUB keys       // process.env itself untouched (R1)
  try out = execFileSync("git", args, {cwd: root, env, stdio:["ignore","pipe","pipe"], timeout:10000, encoding:"utf8"})
      return {status:0, stdout:out}
  catch e:
      if e.code === "ENOENT": return {status:null, error:"ENOENT"}
      if typeof e.status === "number": return {status:e.status, stdout:String(e.stdout||"")}
      return {status:null, error: e.message}             // timeout (ETIMEDOUT / signal) or spawn error

function leg(action, p, reason, warning?) -> {harness:"claude-code", surface:"git-exclude", action, path:p, reason, warning?}

function escapePrefix(P)
  s = P.replace(/[*?[\\!]/g, ch => "\\" + ch)
  s = s.replace(/ (\/?)$/, "\\ $1")      // FR-44 trailing space (before the final "/")
  return s

function ensureSettingsLocalIgnored(root, {dryRun, run}) -> WireLeg
  git = run || defaultRun(root)
  // Step 1
  r = git(["rev-parse","--is-inside-work-tree"])
  if r.error === "ENOENT": return leg("skipped-no-git", root, "git not found; cannot ensure .claude/settings.local.json is git-ignored")
  if r.error:              return leg("skipped-git-error", root, "git failed: " + r.error)
  if r.status !== 0 or r.stdout.trim() !== "true": return leg("skipped-not-repo", root, "not inside a git work tree; nothing to ignore")
  // Step 2
  r = git(["ls-files","--error-unmatch","--",REL])
  if r.status === 0:
     w = ".claude/settings.local.json is tracked by git, so ignore rules do not apply and local wiring is committable; untrack it with 'git rm --cached .claude/settings.local.json'"
     return leg("skipped-tracked", join(root,REL), "tracked by git", w)
  // Step 3
  r = git(["check-ignore","-q","--",REL])
  if r.status === 0: return leg("unchanged", join(root, REL), "already git-ignored")   // exclude file not read
  if r.status !== 1: return leg("skipped-git-error", root, `git check-ignore failed (exit ${r.status ?? r.error})`)
  // Step 4
  gp = git(["rev-parse","--git-path","info/exclude"]); sp = git(["rev-parse","--show-prefix"])
  if gp.status !== 0 or sp.status !== 0: return leg("skipped-git-error", root, "git rev-parse failed")
  X = path.resolve(root, gp.stdout.trim()); P = sp.stdout.replace(/\r?\n$/,"")
  E = "/" + escapePrefix(P) + REL
  // Step 5 + 6 via the writer (single read, CAS, verify)
  let already = false
  compute = raw =>
    lines = (raw||"").split("\n").map(l => l.replace(/\r$/,"").replace(/\s+$/,""))
    if lines.includes(E): return {text:null, detail:{already:true}}
    base = raw || ""
    sep = base !== "" and !base.endsWith("\n") ? "\n" : ""
    return {text: base + sep + E + "\n", detail:{already:false}}
  res = safeWrite.updateFileSync(X, raw => compute(raw).text, {dryRun, newMode:0o600})   // no containWithin (git metadata)
  if compute(res.before).detail.already:
     w = `${X} already lists ${E} but git does not ignore .claude/settings.local.json; a higher-precedence rule (for example '!' in .gitignore) overrides it`
     return leg("appended-not-effective", X, "already listed; still not ignored", w)
  out = leg(res.action, X, "added " + E)
  // Step 7 (real run only)
  if !dryRun:
     r = git(["check-ignore","-q","--",REL])
     if r.status !== 0:
        out.action = "appended-not-effective"; out.reason = `added ${E}; still not ignored (higher-precedence rule)`
        out.warning = `${X}: added ${E} but git still does not ignore .claude/settings.local.json; a higher-precedence rule overrides it`
  return out
```

The verify fixpoint holds: `compute(written).text === null` because E is now a line.

## Error handling
Git failures → skip legs. `SafeWriteError` from the append propagates (FR-40, FR-31).

## Key tests
- Inject `run` stub for each FR-42 row (ENOENT, 128, stdout "false", timeout, tracked, check-ignore 0, exit 128 on check-ignore, step 5 existing line with `\r\n` and trailing spaces, step 7 not-effective).
- Real git: fresh repo → `created` X, `git status --porcelain` omits file; second run → `unchanged`, mtime of X unchanged; linked worktree → entry in common-dir exclude; submodule with `--project-dir` → `.git/modules/<n>/info/exclude`; subdir prefix with `[` and `!` escaped.
- Dry-run: X not created, leg `created`; only read-only git commands invoked (assert via `run` recorder).
- No-trailing-newline exclude file gets `\n` inserted before E.
