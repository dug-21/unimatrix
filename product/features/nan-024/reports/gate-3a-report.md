# Gate 3a Report: nan-024

> Gate: 3a (Component Design Review)
> Date: 2026-11-26
> Result: REWORKABLE FAIL

Unimatrix entries consulted: briefing (#2577, #4333, #3548, #1204, #1496, #3644, #3598, #3612 lessons on multi-doc consistency), `context_lookup` nan-024 decisions #5894 (ADR-006), #5896 (ADR-001), #5897 (ADR-003), #5898 (ADR-005), #5900 (ADR-002), #5901 (ADR-004). ADR IDs in ADR-INDEX.md match the active set. ADR text read from `architecture/ADR-*.md`.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | FAIL | 14 components match ARCHITECTURE/ADRs and D-072..D-091. One defect: trust.md Windows per-event path (F4) contradicts ADR-002 |
| 2. Specification coverage | PASS (WARN) | Every FR/NFR/AC has pseudocode. No scope additions. Minor wording drift (W1, W2) |
| 3. Risk coverage | PASS (WARN) | R-01..R-40 and AC-01..AC-16 all map to test plans. WARNs W3-W5 |
| 4. Interface consistency | FAIL | Four pseudocode/test-plan contradictions (F1, F2, F3, F5) |
| 5. Knowledge stewardship | PASS (WARN) | Pseudocode, test-plan and architect-rework reports carry stewardship and "Unimatrix entries consulted". Session-1 architect/risk reports lack the consulted line (W6) |

## Detailed Findings

### Check 1: Architecture alignment — FAIL (one item)
Verified: component table (shim, shim-entry, install-store, resolve-runtime, trust, project-state, launcher, hook-ownership, merge-settings, wire/init, codex-install, opencode-install, hook-client) matches ARCHITECTURE.md. Contracts match the Integration Surface: `isUnimatrixProject`, `isInstalled(project, platform, {env, home})`, `resolveRuntime` + `allowSelf`, `effectiveEnv`, `checkTrust`, hook command forms, MCP forms, `status` incl. `cloudEnv` text/JSON order, exit table, pre-Node check clauses (a)-(d). Decision spot-checks, all consistent: D-072 (Windows MCP = D-047 fallback), D-077/D-086 (spawn, signal mirroring), D-078 (binary in `pkg/<version>/node_modules`), D-079 (superset test, Node D-052-strict), D-081/D-084/D-085 (Codex detection, MCP gating, `$CODEX_HOME`), D-088 (Windows pre-check disabled), D-089, D-090 (sibling `trust.js`), D-091 (`effectiveEnv` call sites, raw env in `status`).

**F4 (FAIL) — trust.md Windows per-event path.** `trustSet` adds `trust.json` and the per-event loop in `checkTrustWindowsPerEvent` compares every path in `paths` to `tj[...]`. `recordTrustIdentity` records identities before it rewrites `trust.json`, so `trust.json`'s own identity can never match after the write. Result: every event takes `fullAndRewrite` (icacls spawn), breaking D-036 / ADR-002 "no process spawn on the matching path" and R-22/R-30. ADR-002 says `trust.json` is covered per event by the identity of its parent `~/.unimatrix` dir, not by itself. Also: `parentId` is computed before the `tj == null` check (can dereference null) and its result is never used.
Fix: exclude `trust.json` from the per-event identity loop on win32; compare parent-dir identity and act on it; null-check `tj` first.

### Check 2: Specification coverage — PASS (WARN)
FR-01..FR-22 incl. 07a-07j, 12a0-12h, NFR-01..06 and AC-01..AC-16 have pseudocode homes (launcher, shim, shim-entry, install-store, merge-settings/claude-json/mcp-entry, codex-install, opencode-install, hook-client, ci-and-docs Parts A/B/C). No unrequested features found.
- W1: pseudocode OVERVIEW header and test-plan OVERVIEW still cite "D-001..D-083" and old ADR IDs (#5889, #5892, #5893, #5891). Body content applies D-084..D-091. Per D-067, cite ADR numbers. Cosmetic.
- W2: wire-init D-082 non-Windows preflight is "warning if PATH node exists, else exit 1". FR-07h/AC-04 state exit 1 for stale recorded Node, scoped to the Windows fallback entry. Acceptable reading, but see F3.

### Check 3: Risk coverage — PASS (WARN)
All 40 risks and 16 ACs map to a component test plan (test-plan/OVERVIEW.md table plus per-component files). Critical R-03: positive/negative/property tables plus L1-L6 seeded re-init. High risks (R-04, R-07, R-08, R-17, R-18, R-26, R-28, R-29, R-34, R-36, R-37, R-38, R-39) each have concrete scenarios. Behavioural-Outcome table is reflected in the wiring-e2e assembled tests.
- W3: ci-and-docs says the Linux local E2E row uses a "stub binary". A stub cannot store an observation over UDS, so AC-07 local proof needs the built server (overview lists a "linux local E2E (UDS)" job and `cargo build --release`). State this explicitly.
- W4: R-39 "record what each harness displays for a 0-exit MCP server" has no explicit test-plan item.
- W5: Vague asserts: project-state "old two-arg call form removed or still works", resolve-runtime `clientSource` "install or self". Pseudocode fixes both (third arg required; `"install"`). R-37 plan does not say init may exit 1 after 3 retries ("changed while writing") under the 50-init stress. hook-ownership plan lists only the new MCP forms, not legacy MCP forms 1-3. AC-12 suite list is not enumerated (relies on existing jobs plus count guard).

### Check 4: Interface consistency — FAIL
- **F1 (FAIL)** install-store.md test plan ends with "Soft guard only: fail if pkg copy > a limit recorded in the report". D-087 says no cap, Stage 3c reports size only; test-plan OVERVIEW says "no pass/fail threshold". Remove the guard.
- **F2 (FAIL)** launcher.md test plan "Rework: status additions" expects `binary: MISSING <path>`. No such field exists in the spec (FR-12g), ADR-004, ARCHITECTURE or launcher pseudocode text/JSON. Remove it, or have the architect add the field (not in scope; no decision authorises it).
- **F3 (FAIL)** wire-init.md test plan expects `wire` / non-re-recording upgrade with the recorded node missing to exit 1. Pseudocode on non-win32 only warns when a PATH node exists (always true in tests). Align: either test the Windows fallback / no-PATH-node case for exit 1, or change pseudocode to exit 1 per D-082/FR-07h. Pseudocode phrasing "check applies only when install.json exists and no PATH node" is also ambiguous.
- **F5 (FAIL)** trust.md test plan says "Symlinked trust input resolved before check"; trust.md pseudocode refuses symlinks (`lstat`, "symlink not allowed") and the pseudocode OVERVIEW lists this as a conscious conservative choice. Test plan must assert refusal.
- Other cross-file contracts (OVERVIEW shared types, `UnsafeShimPathError` single definition, `buildShimCommand` 4-arg, `codexApprovalStep`, `NOT_ATTACHED_HINT`, `effectiveEnv` call-site list) are coherent across pseudocode files.
- Opencode test plan mentions "respects `XDG_CONFIG_HOME` fixture"; pseudocode and spec write `~/.config/opencode/opencode.json` with no XDG. Treat as isolation only (WARN, clarify).

### Check 5: Knowledge stewardship — PASS (WARN)
- uni-pseudocode report (original, rework, rework 2/3): `Queried:` entries, deviations stated; consulted lines present.
- uni-tester report: `Queried:` and `Stored: nothing novel -- reason`; consulted lines present.
- Architect rework report: `Queried:` and `Stored:` (context_correct #5896-#5899).
- W6: `nan-024-agent-1-architect-report.md` and `nan-024-agent-3-risk-report.md` (Session 1 artefacts) have stewardship blocks but no "Unimatrix entries consulted" line. Not in 3a scope; note only.

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| F4 trust.json self-identity / parentId misuse (trust.md) | uni-pseudocode | Exclude `trust.json` from per-event identity loop; null-check `tj`; use parent-dir identity result |
| F5 symlink behaviour (test-plan/trust.md) | uni-tester | Assert symlinked redirect/install/shim refused (matches pseudocode) |
| F1 size guard (test-plan/install-store.md) | uni-tester | Delete the "soft guard / fail if > limit" sentence (D-087) |
| F2 `binary: MISSING` (test-plan/launcher.md) | uni-tester | Remove the line (not in spec/ADR-004/pseudocode) |
| F3 D-082 preflight mismatch (wire-init.md both) | uni-pseudocode + uni-tester | Make Linux preflight rule explicit and make the test plan match it (exit 1 vs warn when PATH node exists) |
| W1, W3, W4, W5 | uni-pseudocode / uni-tester | Optional cleanup: stale D/ADR ID headers, UDS row must use built server, R-39 harness-notice item, vague asserts |

## Notes
No SCOPE FAIL indicators. All FAILs are text-level fixes in pseudocode/test-plan files.
