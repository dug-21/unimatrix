# vnc-030 cwd Probe Report (shared with vnc-027 OQ5)

**Agent**: vnc-030-cwd-probe · **Date**: 2026-06-08 · **Issue**: #699

**Question**: When Claude Code runs a worktree-isolated subagent, does the hook stdin JSON `cwd` field carry the WORKTREE path or the MAIN checkout path?

## Verdict: WORKTREE path (high confidence)

The hook's effective cwd for worktree-subagent tool events is the worktree path under **every** branch of `resolve_cwd` (hook.rs:352-365: `--project-dir` > stdin `cwd` > process cwd):

- Project hooks pass no `--project-dir` (`/workspaces/unimatrix/.claude/settings.json` — commands are bare `unimatrix hook <Event>`).
- The hook **process is spawned with working directory = the worktree** (captured live, below), so even the process-cwd fallback yields the worktree.
- Claude Code's per-thread cwd for the subagent — the documented source of the stdin `cwd` field ("current working directory when the hook is invoked") — is the worktree (transcript evidence, below).

Caveat for byte-level purists: no persisted channel captures the literal stdin JSON (see "No persisted stdin discriminator"), so the stdin field itself was not byte-captured. But because spawn-cwd = worktree, the stdin-vs-fallback distinction is immaterial: the value entering `detect_project_root` is the worktree path either way.

## Probe environment (for correlation)

| Item | Value |
|---|---|
| Worktree (my pwd / `--show-toplevel`) | `/workspaces/unimatrix/.claude/worktrees/agent-a17e55a00af2f56e9` |
| `--git-dir` | `/workspaces/unimatrix/.git/worktrees/agent-a17e55a00af2f56e9` |
| Session / agent | `a18a0f28-1e6e-4ad7-9881-55d3728531ad` / `agent-a17e55a00af2f56e9` |
| Probe start | 2026-06-08T02:27:54Z, marker `vnc-030-cwd-probe-marker-1780885674` |
| Main-root hash | `/workspaces/unimatrix` → `0d62f3bf1bf46a0a` (live: db, log, sockets) |
| Worktree hash | `96f52b85e5959518` → **no `~/.unimatrix/` dir exists** |

## Evidence chain

**E1 — Live capture of hook processes (direct, time-correlated).** A 45 s read-only `/proc` poll (`/tmp/vnc-030-hookcap.txt`) caught three hook spawns:

```
02:34:20.044 pid=17058 [/bin/sh -c .../unimatrix hook PreToolUse]
  cwd=/workspaces/unimatrix/.claude/worktrees/agent-a17e55a00af2f56e9
  CLAUDE_CODE_SESSION_ID=a18a0f28-... CLAUDE_PROJECT_DIR=/workspaces/unimatrix
02:34:27.845 pid=38207 [... hook PreToolUse]  cwd=<worktree>  session=a18a0f28-...
02:34:25.501 pid=31802 [... hook SubagentStop] cwd=/workspaces/unimatrix  session=d659189b-... (control)
```

pid 38207 (02:34:27.845) correlates to my trigger command timestamped 02:34:27.853. The control row — a concurrent main-checkout session — shows main cwd, proving the capture discriminates. Note `CLAUDE_PROJECT_DIR` stays the **main** checkout even for the worktree agent.

**E2 — Per-thread transcript cwd.** My agent transcript `/home/vscode/.claude/projects/-workspaces-unimatrix/a18a0f28-.../subagents/agent-a17e55a00af2f56e9.jsonl`: 67/67 entries have `"cwd":"<worktree>"`. The parent transcript (`a18a0f28-....jsonl`): 71/71 entries `"cwd":"/workspaces/unimatrix"`. Claude Code tracks cwd per thread; the subagent thread's is the worktree.

**E3 — Worktree events landed under the main-root hash (gitdir port exercised).** Server log `~/.unimatrix/0d62f3bf1bf46a0a/unimatrix.log`: `UDS: event recorded event_type="PreToolUse"/"PostToolUse" session_id="a18a0f28-..."` at 02:27:54.739/.753 (my marker ran 02:27:54.853Z) and continuously thereafter. No state dir for hash `96f52b85e5959518` exists. Per the brief's caution this alone is non-discriminating — but combined with E1 (input = worktree path) it is positive confirmation that `detect_project_root`/`resolveGitFile` resolved a worktree cwd to the main root in production.

**E4 — Code: where cwd flows, and where it doesn't.**
- `crates/unimatrix-server/src/uds/hook.rs:352-365` — `resolve_cwd` precedence; `:175-177` — `detect_project_root(cwd)` → main root → hash → socket.
- `crates/unimatrix-engine/src/wire.rs:57-68` — `HookInput.cwd` is an explicit serde field, so it is **excluded** from the flattened `extra` that becomes every tool event's `payload` (hook.rs:543, 568, 656). `wire.rs:225-251` — `ImplantEvent` has no cwd field.
- Only `SessionRegister` carries `cwd` on the wire (hook.rs:462-475); the server logs it (`listener.rs:565-571`) but does **not** persist it to the sessions table (`SessionRecord`, listener.rs:600-609 — no cwd column).
- JS client parity: `packages/unimatrix/lib/hook-client/build-request.js:47-57` (cwd only in SessionRegister), `config.js:42-103` (`walkToProjectRoot`/`resolveGitFile` — `.git` FILE → gitdir → main root, one hash for all worktrees), `state.js` (health.json is content-free per ADR-005 — no cwd).

## No persisted stdin discriminator (client or server)

SessionStart never fires for worktree subagents (last `UDS: session registered` for my session: 01:48:49, cwd=`/workspaces/unimatrix` — the parent's launch, before the worktree existed). Tool-event payloads exclude `cwd`; observations DB rows, queue frames, health.json, and breadcrumbs carry none. So nothing in the unimatrix pipeline records the raw stdin cwd — the live `/proc` capture (E1) plus transcripts (E2) were used instead. If byte-level stdin proof is ever required: a pre-registered tee wrapper (`cat | tee /tmp/hook-stdin.json | unimatrix hook ...`) in a **scratch** repo's settings — not needed given E1–E3 concordance.

## Implications

**vnc-030 AC-08** (stamp written under main-root hash from a worktree): **Confirmed feasible, already the production behavior.** Hooks invoked from a worktree subagent receive the worktree path; the F3 gitdir resolution (`resolveGitFile` / `detect_project_root`) maps it to the main root, so the stamp (like state, queue, socket) lands under the main-root hash. AC-08 needs no extra cwd handling — but it MUST route through `walkToProjectRoot`, never hash the raw cwd.

**vnc-027 OQ5**: hook stdin `cwd` = **worktree path**, not the main checkout. Any vnc-027 logic must not equate `cwd` with the project root; derive the root via the gitdir walk (or `CLAUDE_PROJECT_DIR`, which the harness sets to the main checkout even inside worktree agents — E1).
