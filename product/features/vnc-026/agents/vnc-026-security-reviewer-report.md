# Security Review: vnc-026-security-reviewer

## Risk Level: low

## Summary
PR #696 (TS HTTP hook client, F3) is a defensively-engineered, zero-dependency
CommonJS port of the Rust hook. Full diff read cold; source files read in full.
No blocking findings, no required changes. Two low-severity advisory notes.

## Findings

### F1 — gitignore warning is best-effort literal match (advisory)
- **Severity**: low
- **Location**: packages/unimatrix/lib/init.js (gitignoreWarning)
- **Description**: Warning that `settings.local.json` is not gitignored uses a
  fixed literal-pattern list, no glob/negation engine. False positives (covered
  by an unlisted pattern) and false negatives (global gitignore) are possible.
- **Recommendation**: Accept as-is — defense-in-depth only; the file is already
  written 0600 and the gap is documented.
- **Blocking**: no

### F2 — token on `init --remote --token` argv (advisory)
- **Severity**: low
- **Location**: packages/unimatrix/bin/unimatrix.js, lib/init.js (initRemote)
- **Description**: The bearer token appears on the one-time interactive init
  command line (shell history / process listing). Documented and explicitly out
  of RQ-3 scope (RQ-3 governs the HOOK command + checked-in files). Token never
  reaches settings.json, breadcrumb, stderr, queue frames, or logs.
- **Recommendation**: Optional one-line user-doc note suggesting the
  UNIMATRIX_REMOTE_TOKEN env path for the security-conscious.
- **Blocking**: no

## Verified (no issues)
- Dependencies: zero new runtime deps; hard CI zero-dep audit gate.
- Token leakage (R-16): token only in config + Authorization header; never
  logged; settings.json carries only `node <path> <EVENT>`; settings.local.json
  0600 + re-chmod.
- Path traversal (A01): sanitizeSessionKey constrains/hashes session_id.
- Injection (A03): no eval/Function/shell; only pre-existing execFileSync uses
  array argv (untouched local path).
- Untrusted parse (A08): 1 MiB stdin cap, serde-parity typing, lone-surrogate
  rejection, never throws; TOCTOU + rewrite guards on transcript/delta.
- Server-controlled stdout (R-15): only 200 text/plain non-empty writes stdout.
- DoS bounds: stdin/body 1 MiB, delta 64 KiB + post-serialize assert, queue
  500 files / 5 MiB / 24h prune, replay 32/256 KiB, per-request timeouts with
  single-settle unref'd timers.

## Blast Radius Assessment
Fail-open design (exit 0, no stdout on failure) makes the worst credible failure
silent under-delivery, not corruption/escalation. Worst case: a delta
offset-accounting regression mis-aligns a session's spans — bounded by F2's
offset-bounded idempotent merge to degraded learning, never host/server data
corruption. No write path escapes ~/.unimatrix/{hash}/hook-client/ (0700/0600);
no path writes the host conversation except the gated text/plain sync envelope.

## Regression Risk
Contained. Local-mode init preserved via normalizeCommandSource back-compat
wrapper (byte-identical except the two intended FR-21 events). The R-11
spaced-path ownership-regex defect is resolved and tested (init-remote.test.js
ownership table incl. Windows Program Files + spaced POSIX paths). Rust change is
test-only (#[cfg(test)] corpus generator) — no production Rust behavior change.

## PR Comments
- Posted 1 review comment on PR #696 (state: COMMENTED).
- Blocking findings: no

## Knowledge Stewardship
- Stored: nothing novel to store — findings are PR-specific (best-effort
  gitignore match, interactive-init argv token), both already documented in the
  feature's own RISK-TEST-STRATEGY/ADRs. No generalizable cross-feature security
  anti-pattern surfaced.
