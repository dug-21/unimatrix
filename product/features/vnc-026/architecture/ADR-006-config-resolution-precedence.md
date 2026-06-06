## ADR-006: Spawn-Time Config Resolution — Env Vars over a Single Root-Anchored settings.local.json

### Context

RQ-3 (binding): token + URL live in gitignored `.claude/settings.local.json` (per-project)
with env-var override; never on the hook command line. SR-09 (Medium/Medium): the client
must locate that file at spawn time from whatever cwd the hook runs in; wrong-root
resolution or a missing token fails silently under the exit-0 mandate. The resolution
algorithm must be deterministic, cheap (it runs inside the AC-13 ~12 ms spawn budget),
and aligned with how the state dir is derived — both must agree on what "the project" is.

### Decision

Resolution order, first hit wins:

1. **Env vars**: `UNIMATRIX_REMOTE_URL` + `UNIMATRIX_REMOTE_TOKEN`. If exactly one is
   set, treat as misconfiguration (breadcrumb class `auth`, exit 0). Names to be
   confirmed against F5 (#681) before delivery (open question 3).
2. **Project file**: resolve cwd as the Rust hook does (`stdin.cwd` if non-empty, else
   `process.cwd()`), walk up to the first directory containing `.git`
   (`detectProjectRoot` port — same walk `init.js` already ships; no `.git` found → use
   resolved cwd), then read exactly one location:
   `{project_root}/.claude/settings.local.json`, key:

   ```json
   { "unimatrix": { "remote": { "url": "https://...", "token": "..." } } }
   ```

   No multi-location search, no per-directory probing on the walk — one stat-walk to
   `.git`, one file read. The same `project_root` string feeds the state-dir hash
   (ADR-003), so config identity and state identity can never disagree.

3. **Neither present** → not a remote install (or broken one): breadcrumb + stderr line,
   exit 0, no stdout, no network.

`init --remote` writes the file (merge-preserving — Claude Code owns other keys in
settings.local.json; only the `unimatrix` subtree is touched), sets mode 0600, warns if
the file is not covered by `.gitignore`, and validates the pair via Ping (ADR-005).
The hook command in settings.json carries only `node /abs/path/lib/hook-client/index.js
<EVENT>` — no URL, no token (RQ-3).

Test obligations (SR-09): spawn-time resolution from a subdirectory cwd; stdin cwd
differing from process cwd; missing file; file present but `unimatrix.remote` absent;
env override beating a present file; partial env pair.

### Consequences

- Easier: one canonical config location means "which config did it use?" has one answer;
  root-anchoring matches state-dir derivation, eliminating split-brain between offsets/
  queue and credentials; env override gives CI and containers a file-free path.
- Harder: monorepo users with nested `.git` directories get the nearest root — same
  behavior as the Rust hook and `init.js`, so consistent if surprising; a settings file
  outside any git repo requires running from the project tree (documented).
- Storing the token in a Claude-Code-owned file means Claude Code rewrites of
  settings.local.json could theoretically drop unknown keys — mitigated by init's
  re-run idempotency (re-add) and the env-var escape hatch.
