# C5 — Per-cell write + read-as-barrier positive control

> Source: ARCH C5, ADR-002 §4, SPEC FR-06/AC-03/AC-07/AC-10, RISK R-05/R-04.
> SR-02/SR-03. The load-bearing soundness mechanism.

## Purpose

For each of the four cells, issue the write (via C3/C4) **then** confirm the
cell's own marker is durable and readable via a **bounded retry-until-present**
loop — the marker-keyed read **is** the durability barrier. There is **no
aggregate `store_size` barrier** ("A grew AND B grew") — that is satisfied by the
first of a store's two writes and proves nothing about the second; under
`tokio::spawn` fire-and-forget + WAL `synchronous=NORMAL` a content read gated on
size races an unsynced write and false-REDs (C-08/#5321).

**Disposition (per direction):**
- own marker appears before the deadline → `POS_* = PRESENT` (durability proven).
- own marker absent **at** the deadline → `POS_* = INFRA` (durability/infra
  failure, the own-write never became durable) — **NEVER RED**, never a vacuous
  pass (ADR-002 §4 / AC-10). A genuine mis-route surfaces as RED at the C6
  cross-store cell, independent of this outcome.

## Sequencing (strict per-store; C-08)

The four cells run **strictly sequentially**, each write immediately followed by
its own barrier read — no shared aggregate barrier:

```
run_cells():
    POS_OBS_A := write_then_barrier(observe, SLUG_A, SLUG_DIR_A, M_OBS_A)
    POS_OBS_B := write_then_barrier(observe, SLUG_B, SLUG_DIR_B, M_OBS_B)
    POS_MCP_A := write_then_barrier(mcp,     SLUG_A, SLUG_DIR_A, M_MCP_A)
    POS_MCP_B := write_then_barrier(mcp,     SLUG_B, SLUG_DIR_B, M_MCP_B)
    # Each result ∈ {PRESENT, INFRA}; C6 negative cells run AFTER, gated on PRESENT.
```

## Functions

### `write_then_barrier(surface, slug, store_dir, marker)` → {PRESENT, INFRA}

```
write_then_barrier(surface, slug, store_dir, marker):
    # 1. WRITE (C3 for observe, C4 for mcp).
    if surface == observe: observe_write(slug, marker)     # C3 (asserts 204)
    else:                  mcp_write(slug, marker)         # C4 (asserts RPC ok)

    # 2. READ-AS-BARRIER: bounded retry-until-present for THIS cell's marker in its
    #    OWN store. Deadline-poll shape mirrors posture-smoke Gate 7 (:322-329) but
    #    keyed to a marker query, not a du delta.
    table, predicate := query_for(surface, marker)        # see C6 read primitive
    deadline := now + READ_DEADLINE_SECS                  # ~10s; arm64-CI headroom
    loop:
        n := read_marker(store_dir, table, predicate)     # C6 two-store read prim
        if n == INFRA_SENTINEL:                           # missing main db / sqlite3
            infra_fail "read-as-barrier <slug>/<surface>: store read failed
                        (missing db / dep) — INFRA"
        if n >= 1:
            log "positive control <slug>/<surface> PRESENT (marker durable)."
            return PRESENT
        if now > deadline:
            # Own marker never appeared → durability not established. INFRA, NEVER
            # RED, never a vacuous pass (ADR-002 §4 / AC-10 / R-05 sc.3).
            log "positive control <slug>/<surface> timed out — own marker absent at
                 deadline => INFRA (durability/infra failure, not an isolation RED)."
            return INFRA
        sleep 1                                            # bounded poll, not a fixed sleep
```

### `query_for(surface, marker)` → (table, predicate)

```
query_for(surface, marker):
    if surface == observe:
        # exact match, proven capture idiom (cloud-bundle-lib.sh).
        return ("observations",
                "topic_signal = '<marker>'")
    else:  # mcp — substring match (WHY markers must be mutually non-substring, R-18)
        return ("entries",
                "content LIKE '%<marker>%' OR topic = '<marker>'")   # AC-07 canonical
    # marker is [a-z0-9-] only (R-12) — no LIKE wildcard / quote can enter the
    # predicate. (Built into the sqlite3 query string by C6's read primitive.)
```

## Why no aggregate `store_size` barrier (ADR-002 §1 / FR-06.1)

`store_size()` (`vol du -s`) is retained ONLY for C2 boot/liveness waits. It is
**not** used here: each store takes two writes, so "store grew" is satisfied by
the first and says nothing about the second's durability. The marker-keyed read is
the only sound barrier — it keys durability to the *exact* cell marker.

## Data Flow

| In | Out |
|----|-----|
| `surface`, `slug`, `store_dir`, `marker`, `READ_DEADLINE_SECS` | writes the marker (C3/C4), then polls the own store; returns `PRESENT`/`INFRA` recorded into `POS_*` for C7 |

Reuses C6's `read_marker` primitive (the `vol cat` db+`-wal`+`-shm` → host
`sqlite3 -json` pattern from `cloud-bundle-lib.sh:51-97`). C5 does not
re-implement the copy; it wraps `read_marker` in the retry loop.

## Error Handling

| Condition | Outcome |
|-----------|---------|
| write fails (C3 non-204 / C4 RPC error) | INFRA (propagated from C3/C4) |
| store read fails (missing main db / sqlite3) | INFRA (`infra_fail`) |
| marker not present before deadline | keep polling (INFRA-retry state) |
| marker absent **at** deadline | `POS_* = INFRA` (never RED, never pass) |
| marker present | `POS_* = PRESENT` |

## Key Test Scenarios

1. Each positive control is a marker-keyed retry-until-present read with a bounded
   deadline; it polls db+`-wal`+`-shm` for the specific cell marker (R-05 sc.2).
2. No aggregate "store grew" gate runs before any content read — `store_size` is
   used only for C2 liveness/boot waits (R-05 sc.1 / AC-10).
3. An own-store marker that never appears within the deadline classifies INFRA,
   never RED and never a vacuous pass (R-05 sc.3 / AC-10).
4. A marker injected into the **wrong** store still surfaces as RED at the C6
   cross-store cell even when the own-store positive timed out INFRA — the
   mis-route is never masked (R-05 sc.4).
5. The loop terminates (bounded deadline, no unbounded hang; no fixed `sleep`
   substituted for the poll) (R-05 residual).
6. Writes are strictly sequential per store, each followed immediately by its own
   barrier (C-08 / FR-06.1).
