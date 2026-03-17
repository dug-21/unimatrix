# Agent Report: vnc-005-agent-3-risk

## Role
Architecture-Risk Strategist

## Output
`/workspaces/unimatrix/product/features/vnc-005/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 5 |
| High     | 7 |
| Med      | 5 |
| Low      | 1 |
| **Total**| **18** |

## Critical Risks for Human Attention

**R-01 (Critical): Arc::try_unwrap(store) fails at graceful shutdown.**
Historical evidence from bugfix #92 (entry #312) shows this exact failure mode occurred
during vnc-006–009. The new multi-session model multiplies the risk: every session task
clone of `UnimatrixServer` holds an `Arc<Store>` reference. ADR-002's 30-second join
timeout is the mitigation — but only if all session handles are in the join set. The
race in R-02 (a session spawned after the accept loop breaks) is the most likely way
this invariant is violated. Both risks must be tested together.

**R-03 (Critical): Session EOF triggers graceful shutdown (old code path).**
This is the silent regression that cannot be caught in a quick smoke test. The only
observable symptom is that the daemon dies on the first client disconnect — which looks
identical to correct stdio behavior. AC-04 is the specific acceptance criterion that
catches this, but the test must include a second connection attempt after the first
disconnect to confirm the daemon is truly still alive.

**R-12 (Critical): `unimatrix serve --stdio` no longer exits on stdin close.**
The `QuitReason::Closed` → `graceful_shutdown` path is the same code being restructured
for daemon mode. If the restructuring is incomplete or introduces a regression, the stdio
path hangs indefinitely. This is the highest-visibility regression — it breaks the
current `.mcp.json` behavior pattern that all existing CI pipelines rely on.

## Open Questions for Tester

1. How does the tester simulate "daemon socket file exists but no process is listening"
   (the stale-socket scenario for R-09)? The test harness needs to be able to create
   files in `~/.unimatrix/{hash}/` without starting a real daemon.

2. R-05 (concurrent drain/upsert) requires concurrent test execution. Is the existing
   test harness set up for multi-threaded integration tests, or does this need a
   dedicated concurrency test module?

3. For R-04 (double-daemon on macOS), is macOS CI available, or is this a Linux-only
   test run? The `/proc`-based `is_unimatrix_process` fallback on macOS is the gap.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for risk patterns on daemon UDS socket, graceful
  shutdown, concurrent session — found #81, #245, #300, #312, #731, #735. Entries #312
  and #735 directly elevated risk severity for R-01 and R-05 respectively.
- Stored: nothing novel to store — patterns observed are specific to vnc-005 design
  decisions and do not yet generalize across 2+ features.
