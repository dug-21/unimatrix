# nan-016 — UDS Dogfooding Re-Release Capability — SPECIFICATION

> Scope source: `product/features/nan-016/SCOPE.md` (rescoped header authoritative, 2026-06-10).
> Risk source: `product/features/nan-016/SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-09).
> This spec covers Slice A only. macOS-local, remote-init (#725), and container HTTPS (#726) are out of scope.

## Objective

Deliver a reproducible, idempotent build + copy-install of the in-repo TS hook client (`packages/unimatrix`) to a fixed external path `~/.unimatrix/dogfood-client/`, plus a committed switchover script that repoints this repo's hooks at that installed client and a rollback to the Rust `hook.rs`. The capability is **delivered and proven by effect but never executed against this repo's live `.claude/settings.json`** — the live flip and the F6 (#682) soak-clock start are a deferred, deliberate human action in a no-active-feature window.

## Domain Models (Ubiquitous Language)

Downstream agents MUST use these terms with these exact meanings.

- **In-repo client** — the TS hook client source tree at `packages/unimatrix/lib/hook-client/` (29 JS files), bundled inside the `@dug-21/unimatrix` package. Owned by vnc-027; nan-016 does not modify it (C-8).
- **Copy-install** — freezing a complete, runnable copy of the in-repo client at a fixed external path via `npm pack` + extract OR `npm install --prefix` (byte copy). MUST NOT use `npm link` (C-6).
- **`npm link` (forbidden)** — a symlink from the install path back into the working tree. Forbidden because edits to in-repo source would leak into the installed copy, defeating isolation (AC-03).
- **Installed client / installed path** — the frozen copy at the fixed dir `~/.unimatrix/dogfood-client/`. Its runtime entrypoint is `~/.unimatrix/dogfood-client/lib/hook-client/index.js`.
- **Re-release** — running the build + copy-install script to (re)produce the installed client from current in-repo source.
- **Promotion** — synonym for re-release in the soak context: re-running build + copy-install. Each promotion is the named **F6 soak-reset point**.
- **Soak-reset point** — the act of re-releasing; it resets the F6 (#682) validation soak to the newly installed bytes. nan-016 defines and delivers this point; it does not run the soak or define its pass/fail duration.
- **Switchover** — repointing this repo's hooks from the Rust binary to the installed client's `node <installed-path>/lib/hook-client/index.js <EVENT>` command via `mergeSettings`.
- **Deferred flip (live switchover)** — the actual execution of switchover against this repo's live `/workspaces/unimatrix/.claude/settings.json`. Out of scope for nan-016; a deliberate later human action. Starting it is what starts the F6 soak clock.
- **Rollback** — reverting this repo's hooks back to the Rust `target/release/unimatrix hook <EVENT>` form (via `mergeSettings` with the legacy binary command source).
- **Effect-verification harness / scratch fixture** — a throwaway project root + scratch `.claude/settings.json` (NEVER this repo's live settings) used to run the real switchover script through `mergeSettings` and re-fire a hook against the installed path, proving the script's behavior by effect, not by string-diffing the script.
- **Copy-install isolation** — AC-03 property: after install, editing in-repo source does NOT change the installed copy's bytes or behavior. This is **code freezing only**, NOT state-dir isolation. The installed client deliberately shares `~/.unimatrix/{hash}/` runtime state with the Rust binary (SR-07, #4923).
- **Project-root hash** — first 16 hex of SHA-256 over the realpath'd project root (walk-up to `.git`), per `config.js::computeProjectHash`. Keys the socket/state path on the project root, NOT the client install location — so the installed copy run from this repo's cwd resolves the same `~/.unimatrix/{hash}/unimatrix.sock`.
- **Fail-open** — a hook command that, on any failure (including daemon absent), exits 0 and never breaks the host Claude Code session (C-7).

## Functional Requirements

Each is testable; verification method appears in Acceptance Criteria.

- **FR-1 — Build + copy-install script.** A committed script builds `packages/unimatrix` and copy-installs a frozen copy to the fixed path `~/.unimatrix/dogfood-client/`. Mechanism MUST be `npm pack` + extract OR `npm install --prefix` — a copy install, never `npm link` (C-6).
- **FR-2 — Complete frozen runtime tree.** The installed tree MUST contain the full runtime asset set declared by the package `files` array: `lib/` (including `lib/hook-client/**`), `bin/`, `skills/`, `postinstall.js`, `protocols/`. The installed entrypoint `~/.unimatrix/dogfood-client/lib/hook-client/index.js` MUST exist and be runnable (SR-01, A1).
- **FR-3 — No host-mutating postinstall.** The copy-install MUST NOT run a postinstall that mutates the host or this repo (the package `postinstall.js` exists; the install mechanism must avoid triggering it against the host) (SR-01).
- **FR-4 — Idempotent clean-replace.** Re-running FR-1 MUST produce a clean replacement of `~/.unimatrix/dogfood-client/` (not an overlay onto stale prior bytes). Behavior when the target pre-exists MUST be defined as: remove/replace so the result is deterministic across repeated runs and container rebuilds (SR-02). Re-running is the F6 soak-reset point.
- **FR-5 — Switchover script (delivered, not executed).** A committed script at `scripts/dogfood-switchover.sh` repoints a target repo's hooks to `node <installed-path>/lib/hook-client/index.js <EVENT>` by calling `mergeSettings`. It MUST accept a target settings path / project root parameter so it can be exercised against a scratch fixture without touching this repo's live settings (SR-06).
- **FR-6 — Switchover uses mergeSettings semantics.** Repointing MUST go through `mergeSettings` (not a raw command-string substitution), so the resulting settings carry shipped matcher semantics — including the narrowed PreToolUse matcher `context_cycle|mcp__unimatrix__context_cycle` (replacing a stale `"*"`) and the full 9-event registration with `isUnimatrixHook` idempotent re-point (SR-05).
- **FR-7 — Rollback script/path.** A committed mechanism reverts hooks to the Rust binary command form `LD_LIBRARY_PATH=<dir> target/release/unimatrix hook <EVENT>` via `mergeSettings` (legacy binaryPath command source), restoring the pre-switchover hook commands. Exercised against a scratch fixture.
- **FR-8 — Fail-open switchover command.** The emitted `node <installed-path>/lib/hook-client/index.js <EVENT>` hook command MUST fail-open identically to today's Rust hook: exit 0 and not break the host session on any failure, including the local UDS daemon being absent (C-7, SR-08).
- **FR-9 — No daemon lifecycle management.** Neither switchover nor build/install starts, stops, or probes the UDS daemon. The runbook states the daemon is assumed already running and that hooks fail-open if it is absent (OQ-1d, A4).
- **FR-10 — Effect-verification harness.** A committed test harness creates a scratch project root + scratch `.claude/settings.json`, runs the real switchover script through `mergeSettings`, and re-fires at least one hook event against the installed path. It asserts a real effect (resulting hook commands point at the installed path; resulting matcher set matches shipped semantics), NOT a string-diff of the script (SR-04, SR-05).
- **FR-11 — Daemon-absent effect case.** The harness MUST include a case where the UDS daemon is absent and assert the installed-path hook command still exits 0 / fails open (SR-08).
- **FR-12 — Copy-install isolation proof.** A committed test demonstrates that editing in-repo `packages/unimatrix/lib/hook-client/` source AFTER install does not change the installed copy's bytes or behavior, exercised against the external absolute path. The proof is code-freezing, NOT state-dir separation (SR-07).
- **FR-13 — No live-settings mutation in any test.** No test or harness in nan-016 may read-modify-write this repo's live `/workspaces/unimatrix/.claude/settings.json`. All switchover/rollback exercise routes through scratch paths (SR-06).
- **FR-14 — Runbook.** A committed runbook documents: (a) promotion = re-run build+install = F6 soak-reset point; (b) rollback = revert hooks to Rust `hook.rs`; (c) the live flip is deferred to a no-active-feature window; (d) the PreToolUse matcher-narrowing is an intended behavioral delta applied at flip time; (e) the daemon is assumed running and hooks fail-open if absent.
- **FR-15 — Init local path unchanged.** nan-016 adds only scripts/runbook/tests; it MUST NOT modify `lib/init.js`, `lib/merge-settings.js`, `lib/hook-client/config.js`, or `package.json` runtime behavior such that `npx @dug-21/unimatrix init` (Linux local) wiring changes (C-8, SR-09).

## Non-Functional Requirements

- **NFR-1 — Idempotency (measurable).** Running the build+install script N>1 times yields byte-identical installed trees (modulo timestamps); the second run leaves no stale files from the first (FR-4). Verified by install, mutate-installed-then-reinstall, compare.
- **NFR-2 — Fail-open posture.** Switchover hook command exit code MUST be 0 in the daemon-absent case (FR-8/FR-11). No new code path may throw to the host session.
- **NFR-3 — Copy-install isolation (measurable).** After install, a content hash of `~/.unimatrix/dogfood-client/lib/hook-client/` is invariant under edits to in-repo source until the next re-release (FR-12).
- **NFR-4 — No live-repo mutation during delivery.** Zero writes to `/workspaces/unimatrix/.claude/settings.json` across the entire test suite and delivery (FR-13).
- **NFR-5 — Size gate preserved.** The C-04 hook-client size gate (`test/check-hook-client-size.js`): comment-stripped ≤ 100 KB primary, raw ≤ 160 KB backstop over `lib/hook-client/**/*.js`, MUST still pass (C-04). Since C-8 forbids client changes, this is a regression guard, not a budget to spend.
- **NFR-6 — Zero-deps preserved.** The shipped JS remains dependency-free (`test/check-zero-deps.js` gate) (C-9).
- **NFR-7 — Container-rebuild durability.** The fixed dir `~/.unimatrix/dogfood-client/` is chosen over an npm global prefix because the global prefix is node-version-pinned and breaks on container rebuild; the install MUST target the fixed dir (OQ-1, SR-02).
- **NFR-8 — Linux-local only.** Build/install/switchover target Linux UDS-local mode. No arch-specific client packaging (pure-JS client has no CPU-arch dimension).

## Acceptance Criteria

Each maps to a SCOPE.md AC-ID with a concrete verification method. Per SR-04, AC-02/AC-03 demand a real effect test, not a string-diff.

- **AC-01 — Reproducible idempotent copy-install.**
  Criteria: a committed script builds + copy-installs `packages/unimatrix` to `~/.unimatrix/dogfood-client/` via `npm pack`/extract or `npm install --prefix` (never `npm link`); re-running is idempotent (clean-replace) and is the named F6 soak-reset point.
  Verification: run the script twice; assert (a) install mechanism is copy not symlink (no symlink at the install root pointing into the working tree); (b) `~/.unimatrix/dogfood-client/lib/hook-client/index.js` exists and the full `files`-array tree is present (FR-2); (c) re-run after deleting/mutating an installed file restores a clean tree identical to a fresh install (FR-4, NFR-1). (Covers FR-1, FR-2, FR-3, FR-4.)

- **AC-02 — Switchover repoints by effect.**
  Criteria: `scripts/dogfood-switchover.sh`, when run, repoints hooks to `node <installed-path>/lib/hook-client/index.js <EVENT>` — verified by the script's effect on a scratch fixture, NOT by flipping this repo's live settings.
  Verification: in the effect harness (scratch project root + scratch `settings.json`), run the real script, then assert: (a) every Unimatrix-owned hook command in the scratch settings points at the installed path entrypoint; (b) the resulting matcher set matches shipped `EVENT_MATCHERS`, specifically PreToolUse = `context_cycle|mcp__unimatrix__context_cycle` (narrowed, SR-05); (c) re-firing a hook event against the installed path produces the expected behavior; (d) a daemon-absent run of the installed-path hook command exits 0 (FR-8/FR-11/SR-08). No assertion touches this repo's live settings (FR-13). (Covers FR-5, FR-6, FR-8, FR-10, FR-11, FR-13.)

- **AC-03 — Copy-install isolation (code-freezing).**
  Criteria: after install, editing in-repo `packages/unimatrix/lib/hook-client/` does NOT alter the installed copy's behavior, verified against the installed path. Isolation = installed `lib/` bytes/behavior unchanged after editing in-repo source; NOT state-dir separation (the installed client deliberately shares `~/.unimatrix/{hash}/` state — SR-07, #4923).
  Verification: install; record a content hash of `~/.unimatrix/dogfood-client/lib/hook-client/`; make a behavior-changing edit to an in-repo `lib/hook-client/` file; assert (a) the installed-path content hash is unchanged; (b) running the installed-path entrypoint exhibits pre-edit behavior; (c) the test explicitly does NOT assert separate state dirs (documents shared `{hash}` state). Restore the in-repo edit. (Covers FR-12, NFR-3.)

- **AC-04 — Promotion/rollback runbook.**
  Criteria: the runbook documents promotion = re-run build+install (= F6 reset point); rollback = revert hooks to Rust `hook.rs`; the flip is deferred to a no-active-feature window; the PreToolUse matcher-narrowing is an intended delta; daemon assumed running / fail-open if absent.
  Verification: runbook file exists and contains each of the five documented items (a–e of FR-14); cross-check rollback steps actually reproduce the Rust binary command form via `mergeSettings`. (Covers FR-7, FR-9, FR-14.)

- **AC-05 — Init local path byte-identical (no regression).**
  Criteria: `npx @dug-21/unimatrix init` (Linux local) produces byte-identical wiring to pre-F5 behavior.
  Verification: run the existing init local-path integration/unit tests (`test/init.test.js`, `test/init-integration.test.js`, `test/merge-settings.test.js`) and confirm green; confirm `lib/init.js`, `lib/merge-settings.js`, `lib/hook-client/config.js`, `package.json` runtime behavior is unmodified by nan-016 (git diff shows no behavioral change to these). (Covers FR-15, SR-09.)

- **AC-06 — Size gate passes.**
  Criteria: the C-04 hook-client size gate (stripped ≤ 100 KB / raw ≤ 160 KB) still passes.
  Verification: run `node test/check-hook-client-size.js`; assert exit 0. Also confirm `test/check-zero-deps.js` passes (NFR-6/C-9). (Covers NFR-5, NFR-6.)

## User Workflows

1. **Re-release (promotion / soak reset)** — operator runs the build+install script → frozen client at `~/.unimatrix/dogfood-client/`. Re-running resets the F6 soak.
2. **Switchover (deferred live flip — NOT performed by nan-016)** — in a no-active-feature window, a human runs `scripts/dogfood-switchover.sh` against this repo to repoint live hooks at the installed client, starting the F6 soak clock. nan-016 delivers and proves this mechanism only.
3. **Rollback** — human reverts this repo's hooks to the Rust binary command form, restoring pre-switchover behavior.
4. **Validation (CI / dev, what nan-016 actually runs)** — the effect harness exercises switchover/rollback against scratch fixtures and proves isolation against the installed path, never touching live settings.

## Constraints

- **C-6 (copy-install only)** — copy install (`npm pack`+extract / `npm install --prefix`), never `npm link`.
- **C-7 (fail-open hook posture)** — hooks exit 0 / fail open; switchover introduces no host-breaking hook path; no daemon dependency.
- **C-8 (no client changes)** — do not modify `lib/hook-client/` logic, nor change runtime behavior of `lib/init.js`, `lib/merge-settings.js`, `lib/hook-client/config.js`, `package.json`.
- **C-9 (dependency-free client)** — shipped JS remains dependency-free (`test/check-zero-deps.js`).
- **C-04 (hook-client size gate)** — stripped ≤ 100 KB / raw ≤ 160 KB over `lib/hook-client/**/*.js`.
- **No live-settings mutation** — no test/harness writes this repo's live `.claude/settings.json` (SR-06).
- **Fixed install dir** — `~/.unimatrix/dogfood-client/` (not an npm global prefix; OQ-1).

## Dependencies

- **Existing code surfaces (verified, must be held to / not regressed):**
  - `packages/unimatrix/lib/merge-settings.js` — `mergeSettings(filePath, commandSource, {dryRun})`; legacy string `commandSource` → Rust `LD_LIBRARY_PATH=<dir> <binary> hook <EVENT>` (rollback path); object `{events, commandForEvent}` → node-client form (switchover path); `buildHookClientCommand(clientPath, event)` quotes paths with whitespace; `isUnimatrixHook` recognizes binary + node-client forms (idempotent re-point); `EVENT_MATCHERS` with narrowed `PRETOOLUSE_CYCLE_MATCHER = "context_cycle|mcp__unimatrix__context_cycle"`; `HOOK_EVENTS` = 9 events (SubagentStop opt-in).
  - `packages/unimatrix/lib/init.js` — local init flow (`.mcp.json`, hooks via `mergeSettings(settingsPath, binaryPath, …)`, skills, DB pre-create, validate) — frozen (AC-05).
  - `packages/unimatrix/lib/hook-client/config.js` — `resolve(cwd)` walks to project root, `computeProjectHash` → 16-hex, `socketPathFor` → `~/.unimatrix/{hash}/unimatrix.sock`. Confirms the installed copy run from this repo's cwd derives the SAME socket/state (no separate state dir — AC-03 framing).
  - `packages/unimatrix/package.json` — `files: [bin/, lib/, skills/, postinstall.js, protocols/]` (frozen tree set, FR-2); `postinstall: node postinstall.js` (FR-3 must avoid host-mutating run); version `0.7.2`.
  - `packages/unimatrix/test/check-hook-client-size.js` (C-04 gate), `test/check-zero-deps.js` (C-9 gate), existing init tests under `test/` (AC-05 regression).
- **Tooling:** Node ≥ 18, `npm pack` / `npm install --prefix`, POSIX shell. Scripts directory convention: repo-root `scripts/` (existing `.sh` scripts) or `packages/unimatrix/scripts/` — architect to pin (open question OQ-A).
- **External services:** local UDS daemon (assumed running; not managed by nan-016; hooks fail-open if absent).
- **Upstream features:** F3 (#679, shipped), F4a/vnc-027 (#680, shipped). Enables but does not start F6 (#682).

## NOT in Scope

- Executing the live switchover / flipping this repo's hooks (deferred human action; starts the F6 clock).
- Starting or defining the F6 (#682) soak pass/fail duration.
- Retiring or modifying Rust `hook.rs` (→ F6).
- Any change to `lib/hook-client/` logic, transports, or parity (owned by vnc-027/#680).
- `npm link`-based dogfooding (forbidden, C-6).
- macOS local mode, darwin packages, `build-darwin-*` jobs (CUT — Mac is client-only).
- `init --remote` unification, skills-in-remote, remote-install size gate (DEFERRED → #725).
- Container HTTPS serving + network security posture (→ #726).
- Appending the knowledge block to CLAUDE.md (that is `uni-init`'s job; OQ-4).
- Creating the follow-up flip-tracking issue on #682 (flagged for the human; nan-016 does not create it).
- Daemon lifecycle management (start/stop/probe).

## Open Questions (for architect / human)

- **OQ-A (architect):** Script location convention — repo-root `scripts/` (matches existing `*.sh`) vs `packages/unimatrix/scripts/`. Scope names `scripts/dogfood-switchover.sh`; confirm which `scripts/` and where the build+install script lives.
- **OQ-B (architect):** Pin the single install mechanism — `npm pack` + extract vs `npm install --prefix` — and how each guarantees the complete `files`-array tree without a host-mutating postinstall (SR-01). Both are scope-allowed; one must be chosen.
- **OQ-C (architect):** Effect-harness "re-fire a hook" mechanics — how to invoke the installed-path entrypoint with a synthetic hook event payload and a scratch project root such that it resolves a scratch (or daemon-absent) UDS without contacting the live daemon, while still proving fail-open (FR-10/FR-11).
- **OQ-D (human/architect):** AC-03 in-repo edit during the test mutates tracked source transiently; confirm the test restores it cleanly and that this transient edit is acceptable (it is not a live-settings mutation, but it does touch tracked files).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- no directly relevant entries (returned general packaging/binary-rename ADRs only; nothing on dogfood copy-install or switchover). Verified code surfaces directly instead.
