# Security Review — vnc-030 (PR #702)

Agent: uni-security-reviewer · Feature: vnc-030 · GH Issue: #699 · PR: #702

**Risk level: LOW** · **Blocking findings: NO** (review state: COMMENTED)

Read cold against the full `main...feature/vnc-030` diff, ARCHITECTURE.md, and RISK-TEST-STRATEGY.md.

## Findings

- **SQL injection** — Both `INSERT INTO observations` sites (`listener.rs` single + batch) bind `topic_source` as parameterized `?10`. No string interpolation of topic content anywhere.
- **Path traversal** — Tracker filenames derive only from `sanitizeSessionKey(session_id)` (collapses any value not matching `^[A-Za-z0-9_-]{1,64}$` to a SHA-256 prefix), never from topic content. All paths route through `config.resolve(cwd).stateDir`; no raw-cwd hashing. Topic stored verbatim but never used in any server filesystem path.
- **Deserialization / frozen-F1 (C-01)** — `cycle_stamp` additive with `skip_serializing_if`; no `deny_unknown_fields` regression (verified by old-server-simulation + tolerance tests). Server-inbound only; no trust escalation.
- **Access control** — The two precedence inversions are internal attribution decisions (declared-beats-vote), not auth/trust boundaries; minimal-diff, well-tested.
- **Fail-open (C-04)** — All new client fs touchpoints and `decorateCycleStamp` are try/catch wrapped, never throw, never emit stdout. `health.json` breadcrumb (`bumpStampMiss`) is count-only — no topic/session-id/path.
- **Dependencies (C-08)** — Zero new npm/cargo deps (manifest diffs empty). `cargo audit`: RUSTSEC-2023-0071 (rsa via sqlx-mysql) and unmaintained-crate warnings are pre-existing and unchanged.
- **Secrets** — None hardcoded. `credential_type`/`audit_log` string matches are inside a v27 baseline test fixture, not production.

## Blast radius

Worst case is silent degradation of feature attribution to the existing heuristic floor — never a thrown hook (host session protected by fail-open wrapping), never DB corruption (observation rows immutable, never retro-stamped), never information disclosure (only new persisted artifact is a content-free count).

## Outcome

Non-blocking review comment posted on PR #702. No changes requested.
