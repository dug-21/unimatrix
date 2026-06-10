# nan-016 — UDS Dogfooding Switchover (F5, rescoped)

> **STATUS — RESCOPED 2026-06-10 (uni-zero). This header is authoritative and supersedes the draft body below, which is retained for design-session reference.**
>
> **Deployment matrix settled:** the server is **Linux-only** (local binary or docker, arm/intel); every non-Linux target is a **pure-JS remote client**. This repo dogfoods in **UDS-local** mode.
>
> **nan-016 now ships ONLY Slice A — the UDS local dogfooding re-release capability** (build + copy-install the in-repo TS client to a stable external path, plus the switchover mechanism + runbook to repoint this repo's hooks). It DELIVERS the F6 #682 soak-reset mechanism but does NOT execute the switchover here; the flip — and therefore the F6 soak-clock start — is a deliberate later human action taken in a no-active-feature window. Smallest, lowest-dependency slice; nothing gates it; publish-independent (local copy-install — never `npm link`, never a release).
>
> **CUT (macOS local mode):** Goals 3–4, Slice C, AC-07/08/09/10/11, OQ-5 (codesigning), OQ-6 (darwin-x64), C-1 (macOS build infra), `build-darwin-*` release jobs, C-2/C-3 darwin lockstep. Mac is client-only → no Mac server binary → no macOS build infrastructure (the "largest net-new technical surface" the draft named — gone).
>
> **DEFERRED → #725 (client-only / remote init):** `init --remote` unification, skills-copy-in-remote, Mac-arm + Windows pure-JS client support, remote-install <250 KB gate (AC-06/07/08, OQ-3, OQ-7). Built with the consuming cloud project; validated end-to-end against a live cloud.
>
> **RELATED → #726 (container HTTPS serving wiring + network security posture):** the cloud-serving gap (W2-2 transport shipped, nan-014 container artifacts stale UDS-only). Not part of nan-016; tracked separately under goal:personal-cloud.
>
> **Resolved open questions:**
> - **OQ-2 (switchover mode):** UDS-local. This repo soaks the TS client over UDS against a locally-run server.
> - **OQ-4 (CLAUDE.md block):** `init` does **not** append the knowledge block. It copies skills so `/unimatrix-init` is reachable and prints the next-step pointer; the append is `uni-init`'s job (out of scope).
>
> **Surviving acceptance criteria for nan-016:** see Acceptance Criteria below (renumbered AC-01..AC-06 — local re-release, switchover-by-effect, copy-install isolation, promotion/rollback runbook, Linux-local byte-identical, C-04 size gate). **OQ-1 (install location)** is now resolved in the body (fixed dir `~/.unimatrix/dogfood-client/`).
>
> **Arch note:** pure-JS clients have no CPU-arch dimension; "Mac-arm / Windows" is a support claim, not a packaging constraint — no arch-specific client packages.
>
> _The draft below predates this rescope. Read it for the verified F3/F4 baseline, packaging/release-pipeline background, and Slice A detail; ignore its macOS-local / remote-init / multi-slice framing._

## Problem Statement

This repo dogfoods the **Rust** hook: `/workspaces/unimatrix/.claude/settings.json` points every hook at `target/release/unimatrix hook <EVENT>` (Rust `hook.rs`). The F6 (#682) retirement soak — which validates the TS hook client in production by running this repo's own hooks on it — cannot begin because there is **no reproducible local re-release of the in-repo TS client** to switch to, and no isolation guaranteeing that editing the in-repo client source won't perturb a running soak.

The single real gap nan-016 closes: deliver a reproducible build + copy-install of the in-repo TS client to a stable external path, plus the mechanism + runbook to repoint this repo's hooks at it. nan-016 makes the switch **possible and repeatable**; it does not perform the switch. The flip is a deliberate later human action (a no-active-feature window) so it does not perturb live delivery sessions.

Affected: the Unimatrix project itself — it cannot start the F6 soak without a re-release mechanism that is isolated from active client development.

## Goals

1. **Local re-release capability** — reproducible build + copy-install (NEVER `npm link`) of the in-repo TS client (`packages/unimatrix`) to a stable absolute path outside the dev tree (`~/.unimatrix/dogfood-client/`). Re-running it is the F6 (#682) soak-reset point.
2. **Switchover mechanism + runbook, delivered but NOT executed.** Tooling/runbook to repoint this repo's hooks at the installed-path client, plus rollback (revert to Rust `hook.rs`). The actual flip is a deliberate later action taken when no feature is in active delivery — **out of scope to execute here**.
3. **Prove copy-install isolation** against the installed copy — edits to in-repo client source do not change installed-path behavior — verified **WITHOUT** flipping the live repo.
4. **No regressions / no behavioral change** to `lib/hook-client/`, Rust `hook.rs`, or the `npx … init` local path.

> **Soak-clock note:** The F6 soak clock starts when the switchover is **executed** (a post-delivery human action), NOT at nan-016 merge. nan-016 delivers and enables the reset mechanism; it does not start the clock.

## Non-Goals

- **Executing the live switchover** — flipping this repo's hooks onto the installed-path client is a deferred, deliberate human action taken in a no-active-feature window. nan-016 delivers the mechanism and proves it by effect; it does not flip the live repo.
- **Rust `hook.rs` retirement or behavioral change** → F6 (#682). nan-016 enables the soak's reset mechanism; it does not delete or modify `hook.rs`.
- **TS hook client logic, transports, parity** → shipped in F4a/vnc-027 (#680). This feature consumes the client; it does not change `lib/hook-client/`.
- **`npm link`-based dogfooding** — explicitly forbidden; a symlink to the working tree reintroduces the source-mutation leak the copy-install isolation exists to prevent.
- **macOS local mode + darwin packages** — CUT. Mac is a client-only target (no Mac server binary), so `@dug-21/unimatrix-darwin-*` packages and `build-darwin-*` release jobs are not this feature and not currently planned.
- **`init --remote` unification + skills-in-remote + remote-install size gate** — DEFERRED → #725 (client-only / remote init), built and validated against a live cloud.
- **Container HTTPS serving + network security posture** → #726.
- **Defining F6 soak pass/fail duration** — F6 owns the soak policy; nan-016 defines only the *reset point* (re-run build+install) and the rollback *mechanism* (revert to Rust hook).

## Background Research

### F3/F4 baseline this builds on (verified in code)
- **Single package, client bundled.** `packages/unimatrix/lib/hook-client/` (29 JS files) ships inside `@dug-21/unimatrix`. `files` array ships `bin/ lib/ skills/ postinstall.js protocols/`.
- **Init is JS, delegates to Rust** (ADR-003, entry #1200). `bin/unimatrix.js` routes `argv[0] === "init"` to `lib/init.js`; everything else execs the resolved binary. The local flow wires binary + `.mcp.json` + skills + DB pre-create + validate; this must remain byte-identical (Goal 4 / AC-05).
- **Transport selection already exists at hook runtime** in `lib/hook-client/config.js`: env pair `UNIMATRIX_REMOTE_URL`/`_TOKEN` → http; else `{root}/.claude/settings.local.json` key `unimatrix.remote` → http; else **local UDS** with a derived socket path (`~/.unimatrix/{hash}/unimatrix.sock`). This repo dogfoods in **UDS-local** mode.
- **Project-root-hash detail (verified).** The copy-installed client, run from this repo's cwd, derives the **same** socket hash as the in-repo client would (the hash keys on the project root, not the client install location). So the installed copy resolves the same `~/.unimatrix/{hash}/unimatrix.sock` — **no separate state dir is needed**, and the local UDS daemon pid was confirmed running.
- **Hook command forms** (`lib/merge-settings.js`): local binary mode emits `LD_LIBRARY_PATH=<dir> <binary> hook <EVENT>`; client mode emits `node <path>/hook-client/index.js <EVENT>` (path quoted iff it contains whitespace). `isUnimatrixHook` ownership regex recognizes binary, legacy `unimatrix-server`, and node-client command forms — idempotent re-points across command forms are already supported. The switchover repoints via `mergeSettings` so the eventual soak exercises **shipped matcher semantics** (narrowing the stale PreToolUse `"*"` matcher) rather than a bare command-string swap.
- **C-04 size gate** (`test/check-hook-client-size.js`): comment-stripped ≤ 100 KB primary + raw ≤ 160 KB backstop over `lib/hook-client/**/*.js`. Cap changes are a human decision on the GH issue. nan-016 must keep this passing.

### Current dogfood state (verified)
- `/workspaces/unimatrix/.claude/settings.json` hooks all point at `/workspaces/unimatrix/target/release/unimatrix hook <EVENT>` (Rust). The switchover script, **when eventually run**, repoints these to the absolute installed TS-client path; rollback reverts these lines to Rust.
- The local UDS daemon is running (pid confirmed); hooks fail-open per C-7 if it is absent.

> macOS local mode, darwin packaging, and the macOS release pipeline were previously researched as F5 background; they are no longer in scope (CUT — Mac is client-only). Remote-init unification and the remote-install size gate are deferred to #725.

## Proposed Approach

A single slice: deliver the **local re-release capability and the (unexecuted) switchover mechanism**.

1. **Build + copy-install script** — a committed script builds the in-repo `packages/unimatrix` and copy-installs a frozen copy to the fixed path `~/.unimatrix/dogfood-client/` via `npm pack` + extract or `npm install --prefix` (copy install — **never** `npm link`). The fixed dir is chosen over an npm global prefix because the global prefix is node-version-pinned and breaks on container rebuild (decision, OQ-1). Re-running the script is idempotent and is the named F6 soak-reset point.

2. **Switchover script (delivered, not executed)** — a committed script, e.g. `scripts/dogfood-switchover.sh`, repoints this repo's hooks to `node <fixed-path>/lib/hook-client/index.js <EVENT>` via `mergeSettings`, so one command = promotion = F6 reset point (decision, OQ-1b/OQ-1c). Repointing via `mergeSettings` (rather than a minimal command-string swap) means the eventual soak exercises shipped matcher semantics and narrows the stale PreToolUse `"*"` matcher — a deliberate behavioral delta from the current dogfood settings, applied only when the flip is eventually executed. The script does **not** manage the daemon lifecycle (a UDS daemon already runs locally; hooks fail-open per C-7 if absent — decision, OQ-1d). Its correctness is verified by **effect** (test fixture / dry-run / scratch dir), NOT by flipping this repo's live `.claude/settings.json`.

3. **Isolation proof** — after install, demonstrate that editing in-repo `packages/unimatrix/lib/hook-client/` does not alter the installed copy's behavior (verified against the installed path, without flipping the live repo).

4. **Runbook** — document promotion (= re-run build+install = F6 reset point), rollback (revert hooks to Rust `hook.rs`), and that the flip itself is deferred to a no-active-feature window.

## Acceptance Criteria

- **AC-01**: A committed script builds + copy-installs `packages/unimatrix` to `~/.unimatrix/dogfood-client/` via `npm pack`/extract or `npm install --prefix` (copy install, never `npm link`); re-running it is idempotent and is the named F6 soak-reset point.
- **AC-02**: The switchover script, when run, repoints hooks to `node <fixed-path>/lib/hook-client/index.js <EVENT>` — verified by the script's **effect** (test fixture / dry-run / scratch dir), NOT by flipping this repo's live `.claude/settings.json`.
- **AC-03**: Copy-install isolation — after install, editing in-repo `packages/unimatrix/lib/hook-client/` does not alter the installed copy's behavior (verified against the installed path).
- **AC-04**: The runbook documents promotion = re-run build+install (= F6 reset point); rollback = revert hooks to Rust `hook.rs`; and that the flip itself is deferred to a no-active-feature window.
- **AC-05**: `npx @dug-21/unimatrix init` (Linux local) produces byte-identical wiring to pre-F5 behavior (no regression).
- **AC-06**: The C-04 hook-client size gate (stripped ≤ 100 KB / raw ≤ 160 KB) still passes.

## Constraints

- **C-6 (copy-install only)**: The re-release MUST be a copy install (`npm pack`+extract / `npm install --prefix`), NEVER `npm link`. A symlink to the working tree reintroduces the source-mutation leak.
- **C-7 (fail-open hook posture)**: Hooks keep exit-0 / fail-open behavior; init is the one loud, throwing checkpoint (F3 posture). The switchover must not introduce a hook path that can break the host session, and must not depend on the daemon being up (hooks fail-open if absent).
- **C-8 (no client changes)**: nan-016 must not modify `lib/hook-client/` logic (owned by vnc-027); only the re-release/switchover scripts, runbook, and config.
- **C-9 (dependency-free client)**: The TS client and shipped JS remain dependency-free (existing `test/check-zero-deps.js` gate).
- **C-04 (hook-client size gate)**: comment-stripped ≤ 100 KB / raw ≤ 160 KB over `lib/hook-client/**/*.js` (cap change = human decision on the GH issue).

## Open Questions

None — all resolved during scope refinement. (OQ-1 → fixed dir `~/.unimatrix/dogfood-client/`; OQ-1b → committed `scripts/dogfood-switchover.sh`; OQ-1c → repoint via `mergeSettings`; OQ-1d → no daemon lifecycle management; OQ-2 → UDS-local; OQ-4 → init does not append the CLAUDE.md block.)

## Tracking

GitHub Issue: #681 (nan-016). Branch `feature/nan-016`. Depends on F3 (#679, shipped) and F4a (#680/vnc-027, shipped).

nan-016 **ENABLES** the F6 (#682) soak clock by delivering the reset mechanism; it does **not start** it. The eventual switchover flip (post-delivery, no-active-feature window) should be tracked as a follow-up — a checklist item on #682 or a small follow-up issue. **Flagged for the human; this feature does not create that issue.**
