# state.js — stamp_miss Canary

**Source**: `packages/unimatrix/lib/hook-client/state.js` (extend). **ADR**:
ADR-006 rev2. **Constraints**: C-04 fail-open (never-throw RMW), content-free
breadcrumb (count only — no topic/session-id/path, vnc-026 ADR-005 contract).

## Purpose

Add a `stamp_miss` counter to the `health.json` content-free breadcrumb and a
never-throw `bumpStampMiss(stateDir)` that increments it. The counter is a
**zero-tolerance inheritance-drift invariant** (`stamp_miss == 0` at test time AND
production) — no threshold, no denominator, no baseline. Incremented by the
decoration miss branch ONLY for subagent-context inheritance drift
(index-decoration.md owns the gating logic; this module just does the RMW).

## Changes to state.js

### `defaultBreadcrumb()` — add the field (`state.js:178-187`)

```
function defaultBreadcrumb() {
  return {
    last_success: null,
    last_failure: null,
    failure_class: null,
    consecutive_failures: 0,
    queue_depth: 0,
    url_host: "",
    stamp_miss: 0,            // NEW — zeroed default
  };
}
```

### `readBreadcrumb()` — degrade the field field-by-field (`state.js:200-214`)

Add to the returned object, matching the existing field-by-field safe-int degrade:
```
stamp_miss:
  Number.isSafeInteger(parsed.stamp_miss) && parsed.stamp_miss >= 0
    ? parsed.stamp_miss : 0,
```

### `recordSendOutcomes()` / `writeBreadcrumb()` — preserve `stamp_miss`

Both currently build a `next` object that OMITS `stamp_miss`, so a normal send or
config-miss would DROP the counter to default on rewrite (data loss / R-19
masking). Each must carry `prev.stamp_miss` through (they already read `prev` via
`readBreadcrumb`):
```
// in recordSendOutcomes' `next` (state.js:236-248) add:
  stamp_miss: prev.stamp_miss,
// in writeBreadcrumb' `next` (state.js:262-270) add:
  stamp_miss: prev.stamp_miss,
```
(prev now carries `stamp_miss` because readBreadcrumb returns it.)

### NEW `bumpStampMiss(stateDir) -> bool`

Content-free RMW, count-only, never-throw — mirrors `writeBreadcrumb`'s structure
but touches ONLY `stamp_miss` and leaves every other field at its prior value.
```
function bumpStampMiss(stateDir) {
  if (!usable(stateDir)) return false;
  const prev = readBreadcrumb(stateDir);
  const next = {
    last_success: prev.last_success,
    last_failure: prev.last_failure,
    failure_class: prev.failure_class,
    consecutive_failures: prev.consecutive_failures,
    queue_depth: prev.queue_depth,
    url_host: prev.url_host || "",
    stamp_miss: prev.stamp_miss + 1,        // the only mutation
  };
  if (!ensureStateDir(stateDir)) return false;
  return atomicWrite(healthPath(stateDir), JSON.stringify(next));
}
```
Export `bumpStampMiss` in `module.exports` (state.js:275-289).

## Content-Free Guarantee (security, ADR-006 §1)

`bumpStampMiss` takes NO topic, NO session_id, NO path — only `stateDir` (already
a derived hash path, not user content). It writes a count. A malicious cycle topic
can never poison the breadcrumb because the topic never enters this function. This
is asserted by a test (no topic/sid/path string ever appears in health.json).

## Data Flow

- IN: `stateDir` only.
- OUT: bool (atomicWrite result). The persisted `health.json` gains
  `"stamp_miss": N`.
- Caller: `decorateCycleStamp` (index-decoration.md), miss branch, subagent-gated.

## Error Handling

Never throws. `usable`/`ensureStateDir`/`atomicWrite` failures → `false`. No
stdout, no stderr (this module is silent). Field-by-field degrade on a corrupt
`health.json` means a partially-bad breadcrumb still yields `stamp_miss: 0` rather
than throwing.

## Zero-Tolerance Invariant (FR-10)

The contract is `stamp_miss == 0`. No ratio, no `fnf_record_send_count`
denominator, no 0.20 threshold, no per-deployment baseline, no human re-set. Test
docs pin claude 2.1.167; any CLI bump re-runs the AC-06 fixtures. The F6 gate
review consults the counter before trusting stamp coverage.

## OQ-E Independence (production existence is probe-gated)

This module ships either OQ-E branch. The increment SITE is in
index-decoration.md (subagent-gated). Branch A → the site calls `bumpStampMiss` in
production. Branch B → the production call is gated to no-op (subagent-context
detection co-dependent with inheritance → undetectable client-side), but the
test-time fixtures still drive `bumpStampMiss` directly and assert the exact
counts. `bumpStampMiss` itself is branch-agnostic.

## Key Test Scenarios

- Fresh health.json → `readBreadcrumb().stamp_miss === 0`.
- `bumpStampMiss` increments by exactly 1; two calls → 2 (monotonic within
  breadcrumb lifetime).
- A subsequent `recordSendOutcomes` (normal send) PRESERVES `stamp_miss` (does not
  reset it) — the carry-through assertion (R-19 masking guard).
- `writeBreadcrumb` (config-miss) preserves `stamp_miss`.
- Corrupt/mistyped `stamp_miss` field → degrades to 0, no throw.
- Content-free: assert no topic/session-id/path string is ever written to
  health.json by `bumpStampMiss` (security scenario).
- Failure injection (EACCES on atomicWrite) → `false`, no throw, exit 0.
- Healthy declared-subagent fixture → `stamp_miss == 0` end-to-end (ships both
  branches); inheritance-break fixture → exactly one increment (Branch A site).

## Open Questions / Gaps

- **OQ-E** decides whether the index.js call site fires in production (above).
  No change to this module's API either way.
