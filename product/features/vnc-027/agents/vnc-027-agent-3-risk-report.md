# Agent Report: vnc-027-agent-3-risk (architecture-risk mode)

## Deliverable
`product/features/vnc-027/RISK-TEST-STRATEGY.md` — 18 risks (R-01..R-18), full SR-01..SR-13 traceability, security-surface table, failure-mode matrix, coverage summary.

## Risk Summary
- Critical: 1 (R-02 — size-gate merge ordering; AC-09 must be the literal first commit; vnc-030 cross-feature dependency)
- High: 4 (R-01 FNF frame loss, R-04 TaskCompleted keying unreachable, R-06 sync read/exit sequencing, R-11 cycle-interception via narrowed matcher)
- Medium: 9 — Low: 4

## Verification Findings (residual-note checks, code-verified)
1. **R-04 (needs human/spec decision)**: `TaskCompleted` is NOT in `HOOK_EVENTS` (merge-settings.js) nor `.claude/settings.json` — the AC-10 TaskCompleted delete branch is unreachable; effective mechanism is the 7-day age-prune. ADR-006 also mislabels the frame type (TaskCompleted → SessionClose per build-request.js:60, not RecordEvent). Decision required: register the event or resolve AC-10's "and/or" to age-prune-only.
2. **R-12**: No server-side lifecycle depends on SubagentStop — listener.rs:2919 is an all-None fallthrough. ADR-004 does not state this; converted to an explicit lifecycle test scenario (full session with SubagentStop never sent).
3. **R-13 (spec-writer flag resolved)**: queue bounds apply on the no-daemon enqueue-only path — eviction runs inside `enqueue` (MAX_FILES=500, MAX_TOTAL_BYTES=5 MiB, MAX_AGE_MS=24 h, queue.js:107-146). Not a blocking finding; pinned with a test scenario.
4. **vnc-030 obligations recorded**: AC-09 first-commit ordering as a cross-feature contract; UDS-path stamp regression test owed to vnc-030 post-merge (explicitly not an F4a test); OQ5 residual reduced to a zero-cost soak-time stderr dump (ADR-007 hash fixtures make it immaterial for socket-path resolution).

## Knowledge Stewardship
- Queried: /uni-knowledge-search for lessons, patterns, parity history, Node lifecycle — found #3448 (FNF expected-error taxonomy → R-01), #4473 (warn-continue masks failure paths → R-01/R-16), #4780 (size-gate rework → R-02 elevated to Critical), #4751 (corpus mechanism), #4777/#4778/#4783 (lone-surrogate + injection-header discriminator → R-09, FR-22), #4768/#4774 (node:test grep-gate / async-spawn patterns → R-06 scenarios), #3471 (event registration checklist — informed R-04).
- Stored: entry #4809 "Keying client behavior to a hook event requires verifying install-surface registration, not just normalize/build-request recognition" via /uni-store-pattern (cross-client trap: present in both hook.rs and the TS client).
