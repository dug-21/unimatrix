# C6 — Cross-store negative read + two-store read primitive

> Source: ARCH C6, ADR-002 §2/§5, SPEC FR-03/AC-04/AC-08/AC-11/AC-12, RISK
> R-04/R-06/R-07/R-18. SR-01/SR-02/SR-04/SR-07.

## Purpose

Provide the single parameterized two-store content-read primitive
(`read_marker`) that both C5 (own-store positive, wrapped in retry) and C6 (the
cross-store negative) use, and run the four **negative** cells: each store must
**not** contain the *other* slug's marker. Built **fresh** — it does NOT extend
the removed D6 dir-count/`other_count` heuristic (ass-084 OoS#2, C-11). Each store
is read **on disk via the `vol` sidecar** (AC-12/SR-04) — no slug credential reads
another's store; no over-the-wire B read.

## The read primitive (shared by C5 and C6)

### `read_marker(store_dir, table, predicate)` → row-count (≥0) or `INFRA_SENTINEL`

```
read_marker(store_dir, table, predicate):
    # Reuses cloud-bundle-lib.sh:51-97 (capture_behavioral_topic_signals) idiom:
    # vol cat db + -wal + -shm out to a host sandbox, then host-side sqlite3.
    slug_db := "${store_dir}/unimatrix.db"
    tmp     := "${TMP}/read.$$.db"            # fresh per call (reaped by cleanup)

    # Main db is mandatory (a missing main db = INFRA, never a 0-row cell — R-07).
    if not (vol cat slug_db > tmp) or [ ! -s tmp ]:
        return INFRA_SENTINEL                 # caller raises infra_fail with context

    # Sidecars are mandatory for the DURABLE post-barrier view (SR-02/R-04):
    # a single-file copy reads a PRE-checkpoint false-empty snapshot. Absent
    # (already-checkpointed) sidecar is fine ONLY because the main db is present.
    vol cat "${slug_db}-wal" > "${tmp}-wal"  or  rm -f "${tmp}-wal"
    vol cat "${slug_db}-shm" > "${tmp}-shm"  or  rm -f "${tmp}-shm"

    # sqlite3 presence is asserted in C1 preflight; re-assert defensively (AC-11).
    if not command_exists("sqlite3"):
        infra_fail "sqlite3 absent at read time — INFRA, never an empty-pass"

    # Count rows matching the predicate. Predicate carries the marker literal,
    # which is [a-z0-9-] only (R-12) so no LIKE wildcard / quote can alter it.
    count := sqlite3 -json tmp \
                "SELECT count(*) AS n FROM ${table} WHERE ${predicate};" \
             | node-extract(.[0].n)           # robust JSON extraction, default 0
    rm -f tmp "${tmp}-wal" "${tmp}-shm"
    return count                              # integer >= 0
```

`INFRA_SENTINEL` is a distinguished non-integer return (e.g. the literal string
`INFRA`) so callers never confuse "read failed" with "0 rows". A 0-row read is a
genuine, trusted result (negative-control ABSENT); only a failed copy/missing main
db is INFRA.

## Cross-store negative cells (positive-gates-negative; ADR-002 §5/§7)

Each negative cell is a **single** read (not a retry), evaluated **only after**
that direction's positive control reached PRESENT (C5). The negative checks the
**foreign** marker against a store:

```
run_negatives():
    # Observe matrix: A must NOT hold M_OBS_B; B must NOT hold M_OBS_A.
    NEG_OBS_A := negative_cell(POS_OBS_A, SLUG_DIR_A, observe, M_OBS_B)  # B's marker in A?
    NEG_OBS_B := negative_cell(POS_OBS_B, SLUG_DIR_B, observe, M_OBS_A)  # A's marker in B?
    # MCP matrix: A must NOT hold M_MCP_B; B must NOT hold M_MCP_A.
    NEG_MCP_A := negative_cell(POS_MCP_A, SLUG_DIR_A, mcp,     M_MCP_B)  # B's marker in A?
    NEG_MCP_B := negative_cell(POS_MCP_B, SLUG_DIR_B, mcp,     M_MCP_A)  # A's marker in B?
```

### `negative_cell(own_pos, store_dir, surface, foreign_marker)` → {ABSENT, RED, SKIPPED}

```
negative_cell(own_pos, store_dir, surface, foreign_marker):
    # POSITIVE-GATES-NEGATIVE (FR-05 / FR-06.3): only evaluate the cross-store cell
    # AFTER this direction's own positive reached PRESENT. If the own positive is
    # INFRA, we do NOT report a "other store clean" pass (no vacuous GREEN) — but we
    # STILL read the cross cell for a RED, because a mis-route is RED even when the
    # own positive timed out INFRA (ADR-002 §4-5; R-05 sc.4). The distinction:
    #   - own PRESENT + foreign ABSENT  -> ABSENT  (clean, gates a GREEN)
    #   - foreign PRESENT (either case) -> RED     (leak; dominates)
    #   - own INFRA + foreign ABSENT    -> SKIPPED (no GREEN claim; surface is INFRA)
    table, predicate := query_for(surface, foreign_marker)   # C5 helper
    n := read_marker(store_dir, table, predicate)
    if n == INFRA_SENTINEL:
        infra_fail "negative read <store_dir>/<surface>: store read failed — INFRA"
    if n >= 1:
        log "CROSS-STORE LEAK: foreign marker <foreign_marker> present in <store_dir>"
        return RED                              # real leak — independent of own_pos
    # foreign absent:
    if own_pos == PRESENT:
        return ABSENT                           # clean; eligible to gate a GREEN
    else:
        return SKIPPED                          # own positive INFRA — no GREEN claim
```

> Rationale for still reading the cross cell when own positive is INFRA: the
> headline coverage gain (B mis-resolving into A) can present as **own-store
> timeout INFRA + foreign-marker-in-other-store RED**. Reading the cross cell
> unconditionally for a RED ensures a real mis-route is caught even if the
> own-write's durability could not be confirmed. RED dominates INFRA in C7.

## Data Flow

| In | Out |
|----|-----|
| `store_dir`, `table`, `predicate` (built from a marker) | row count via `vol cat`+`sqlite3`; or `INFRA_SENTINEL` |
| `POS_*` (from C5), foreign markers | `NEG_* ∈ {ABSENT, RED, SKIPPED}` recorded for C7 |

Reads are on-disk via `vol` only (AC-12/SR-04). The single bearer write token is
never repurposed as a read proof; no per-slug read credential is used.

## Error Handling

| Condition | Outcome |
|-----------|---------|
| missing main `unimatrix.db` | INFRA (never a 0-row cell — R-07) |
| `sqlite3` absent at read time | INFRA |
| foreign marker present | `NEG_* = RED` (leak) |
| foreign marker absent + own PRESENT | `NEG_* = ABSENT` |
| foreign marker absent + own INFRA | `NEG_* = SKIPPED` (no GREEN claim) |

## Key Test Scenarios

1. `vol cat` copies `unimatrix.db` **plus** `-wal`/`-shm` for both A and B before
   every query; a leaked cross-marker in an uncopied WAL cannot false-GREEN
   (R-04 sc.1).
2. A missing main db is INFRA, not a 0-row pass (R-04 sc.2 / R-07 sc.2).
3. With all four markers present in their own stores, each `LIKE '%marker%'`
   cross-cell returns 0 — mutually non-substring markers prevent cross-match
   (R-18 sc.2).
4. The negative cell is read only after the own positive PRESENT for a GREEN
   claim; an own-INFRA direction never yields an ABSENT-pass (R-03 / FR-06.3).
5. A marker injected into the wrong store returns ≥1 → RED, even if the own
   positive timed out INFRA (R-05 sc.4).
6. Read primitive is fresh — no dir-count/`other_count` heuristic (C-11; ass-084
   OoS#2). Each read is content-only.
7. The `vol` mount stays read-only (`-v $VOL:/data:ro`) so a read cannot mutate
   the property it measures (Security).
