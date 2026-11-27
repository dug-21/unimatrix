# Test Plan — C5 git exclude (`lib/git-exclude.js`) — new `test/git-exclude.test.js` + `init-integration`

Risks: R-14, R-07, R-03 (exemption). AC-8. Real `git` in temp repos (`makeProject({git:true})`).

## FR-42 rows (one fixture each)
| Fixture | Expect |
|---|---|
| repo, not ignored, no exclude | `created`, one line `/.claude/settings.local.json`, `git status --porcelain` omits it |
| exclude exists without entry | `updated`, exactly one appended line, prior bytes preserved; CRLF and no-trailing-newline variants |
| ignored via `.gitignore` / `core.excludesFile` / exclude | `unchanged`, exclude bytes+mtime equal |
| file tracked | `skipped-tracked` + warning; exclude and index unchanged |
| entry present but `!` override | `appended-not-effective`; repeat run appends nothing |
| non-repo `--project-dir` | `skipped-not-repo`, path R |
| PATH without git | `skipped-no-git` |
| git exits unexpectedly (injected `run`) | `skipped-git-error`, exit code unaffected |
| linked worktree | entry in `git rev-parse --git-path info/exclude` file |
| submodule `--project-dir` | entry in `<super>/.git/modules/<n>/info/exclude`; no `outside-project` |
| subdir root `a*b[1]!` | prefixed, escaped entry; check-ignore then passes |
| missing `info/` | created |
| symlinked exclude | still link, target appended |
| parent env `GIT_DIR=<other repo>` | other repo untouched; scrubbed |
| `dryRun` | reports `created`/`updated`, writes nothing, only read-only git commands |
| second run | `unchanged` |

Every case: `git diff --exit-code` and `git diff --cached --exit-code` pass. Bounded git timeout (hang → `skipped-git-error`).
