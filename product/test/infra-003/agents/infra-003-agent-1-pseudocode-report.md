# Agent Report — infra-003-agent-1-pseudocode

**Role:** Pseudocode specialist (Session 2 Stage 3a)
**Task:** Per-component pseudocode for the standalone bidirectional multi-tenant
HTTP isolation shell gate (`multi-tenant-isolation-smoke.sh`).

## Deliverables

| File | Component |
|------|-----------|
| `pseudocode/OVERVIEW.md` | interaction, data flow, shared helpers, INFRA-vs-RED state machine |
| `pseudocode/c1-preflight.md` | C1 read-dependency preflight (docker/sqlite3/vol) |
| `pseudocode/c2-registration.md` | C2 two-slug registration + single restart + route-liveness precondition |
| `pseudocode/c3-observe-probe.md` | C3 observe writes both directions |
| `pseudocode/c4-mcp-probe.md` | C4 MCP-write probe both directions, per-route own `Mcp-Session-Id` |
| `pseudocode/c5-read-as-barrier.md` | C5 per-cell write + read-as-barrier positive control |
| `pseudocode/c6-two-store-read.md` | C6 cross-store negative + two-store read primitive |
| `pseudocode/c7-verdict.md` | C7 verdict gate (2×2, positive-gates-negative, tri-state exit) |
| `pseudocode/r15-invariant-update.md` | R-15 #815 new-smoke-script invariant edit (delivery action) |

## Invariants encoded faithfully

- Bidirectional 2×2: four distinctly-marked writes over one bearer token (slug in
  path), each store asserted to hold ONLY its own marker both directions (C3-C7).
- Read-as-barrier: bounded retry-until-present per own-store positive; own-store
  absence at deadline = INFRA (never RED); RED reserved for cross-store presence;
  positive-gates-negative per direction (C5/C6/C7). No aggregate `store_size`
  barrier; `store_size` demoted to C2 liveness waits only.
- Markers mutually non-substring `infra003-{obs,mcp}-{a,b}-<run>`, charset
  `[a-z0-9-]`; runtime pairwise non-substring self-check in OVERVIEW/main.
- Slugs A=`arch-research` (existing), B=`isolation-b`; both registered before one
  restart; route-liveness is a precondition, not the verdict.
- sqlite3 hard-fail INFRA; `vol cat` db+`-wal`+`-shm`; all reads via the `vol`
  busybox sidecar (read-only), never `docker exec`.
- MCP per-route session isolation: `SID_A`/`SID_B` bound to distinct variables;
  no shared session variable exists to cross (R-17 structurally excluded).
- Tri-state exit: GREEN=0 / RED=1 / INFRA=2(distinct) / SKIP=3; RED dominates
  INFRA dominates GREEN; no non-pass rounds to 0.

## Integration surface — all traced, none invented

All function signatures and columns reference the architecture Integration Surface
and the existing infra-001 primitives (`vol()`, `store_size()`,
`capture_behavioral_topic_signals` `cloud-bundle-lib.sh:51-97`, boot/register/
restart + deadline-poll idioms in `docker-http-posture-smoke.sh`). The #815
invariant edit targets the real `KNOWN_SMOKE_SCRIPTS` array in
`release-gate-bundle-static-test.sh` (lines ~196-199).

## Open questions / gaps flagged (not placeholdered)

1. **Non-writing route-liveness probe (C2):** exact method/path that yields
   non-404 for a live route without persisting a row, per surface. ADR-004 fixes
   the requirement; the literal request is a tester detail.
2. **Minimal valid wire frames (C3/C4):** smallest valid `RecordEvent`
   persisting a `topic_signal` row, and the literal `initialize`/`tools/call`
   JSON-RPC frames. ADR-002/003 fix the approach + surface; bytes are a tester
   detail.
3. **INFRA exit code value:** proposed `2`; delivery must confirm it does not
   collide with posture-smoke's `4` ("image unavailable") if both run in one lane.
4. **`READ_DEADLINE_SECS`:** mirror the ~10s store-grow wait in
   posture-smoke (`:322`); confirm against arm64-CI headroom (SPEC OQ / R-05
   residual).
5. **Marker column fallback:** if a future payload drops `topic_signal`, the
   spec-named fallback is `observations.input` substring — flag, do not silently
   switch (R-09).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (decision/infra-003 → ADR-001 #5335,
  ADR-002 #5342, ADR-003 #5343, ADR-004 #5344; pattern → #5193 WAL-robust
  store-grew `du -s` over the dir). Read all four ADR files + the infra-001 harness
  source. Findings applied directly.
- Deviations from established patterns: none. The read-as-barrier wraps the
  established `cloud-bundle-lib.sh` content-read idiom; `store_size` (#5193) is
  deliberately retained only for liveness, not the barrier, per ADR-002/004.
