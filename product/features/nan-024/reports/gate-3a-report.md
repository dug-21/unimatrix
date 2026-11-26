# Gate 3a Report: nan-024

> Gate: 3a (Component Design Review), re-check after rework (iteration 1 of 2)
> Date: 2026-11-26
> Result: PASS

Unimatrix entries consulted: briefing (#3612, #2577, #3611, #3598, #3644, #1204, #4043, #4333, #3548), `context_lookup` nan-024 decisions #5894 (ADR-006), #5896 (ADR-001), #5897 (ADR-003), #5898 (ADR-005), #5900 (ADR-002), #5901 (ADR-004). ADR text read from `architecture/ADR-*.md`; IDs match ADR-INDEX.md.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | F4 fixed; trust.md Windows per-event path now consistent with ADR-002 #5900 |
| 2. Specification coverage | PASS | No gaps, no scope additions; W1 cleaned |
| 3. Risk coverage | PASS (WARN) | R-01..R-40, AC-01..AC-16 mapped; W3/W4/W5 resolved; one minor WARN (dir identity case) |
| 4. Interface consistency | PASS | F1, F2, F3, F5 fixed; no remaining contradictions found |
| 5. Knowledge stewardship | PASS (WARN) | Unchanged from prior run; W6 (session-1 reports lack consulted line, out of 3a scope) |

## Prior findings verification

- **F1 (install-store size guard)**: FIXED. test-plan/install-store.md now reads "Report only (D-087): no cap, no pass/fail threshold"; consistent with test-plan/OVERVIEW.md.
- **F2 (`binary: MISSING`)**: FIXED. launcher test plan now asserts "`status` field set is exactly FR-12g/ADR-004 (no `binary:` line exists)"; only `mcp node: MISSING` remains, which exists in launcher pseudocode.
- **F3 (D-082 preflight)**: FIXED. pseudocode/wire-init.md states the explicit rule (non-win32: recorded node missing and PATH node present -> warn, exit 0; no PATH node -> exit 1; win32 fallback entry or `install.json.node` missing -> exit 1). test-plan/wire-init.md tests all three branches (warn, fail-no-PATH, fail-Windows) and matches the pseudocode.
- **F4 (trust.json Windows self-identity)**: FIXED. `tj` null-check first; `trust.json` skipped in the per-event loop; `~/.unimatrix` dir compared by `{ino, mode}` only, mismatch or missing record -> `fullAndRewrite`; `recordTrustIdentity` skips `trust.json` itself. Matches ADR-002: "covered by the recorded identity of its parent `~/.unimatrix` dir"; no spawn on the matching path (D-036, R-22/R-30).
- **F5 (symlink)**: FIXED. test-plan/trust.md asserts refusal with `reason:"symlink not allowed"`, matches pseudocode `lstat` behaviour.
- **W1**: FIXED (headers cite D-001..D-091 and ADR numbers). **W3**: FIXED (ci-and-docs test plan and pseudocode state built server for UDS row). **W4**: FIXED (R-39 harness-display evidence item in ci-and-docs and OVERVIEW). **W5**: FIXED (project-state third arg required, `clientSource === "install"` exact, R-37 exit-1-after-3-retries allowed, legacy MCP forms 1-3 in hook-ownership, AC-12 suite list enumerated). Opencode XDG wording clarified as isolation only.

## Design call: Windows dir identity (ino+mode only)
Consistent with ADR-002 #5900. ADR text lists `{size, mtimeMs, ino}` as the recorded identity; the pseudocode excludes size/mtime for the `~/.unimatrix` dir (each legitimate `trust.json` tmp+rename changes dir mtime) and adds `mode`. This is a justified refinement of the parent-dir coverage rule, documented in the pseudocode, not a contradiction.

**WARN (minor)**: test-plan/trust.md has no explicit case for a `~/.unimatrix` dir `ino`/`mode` mismatch triggering exactly one full check (generic "mismatch -> one full check" and the file `ino` case exist). Add one scenario in Stage 3b tests. Not blocking.

## Check 5 notes
Pseudocode, test-plan and architect-rework reports carry `Queried:`/`Stored:` stewardship and consulted lines. W6 unchanged: Session-1 architect and risk reports lack the consulted line (not in 3a scope).

## Rework Required
None.
