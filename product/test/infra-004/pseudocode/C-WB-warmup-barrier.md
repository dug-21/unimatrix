# C-WB — Warmup / Readiness Barrier

> File: `product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh`
> ADR-001 (#5349). The **only** permitted gate-script change. Inserted between
> `assert_routes_live` and `run_isolation_matrix`.

## Purpose

Before the C3/C4 load-bearing writes, confirm the embedding model is loaded and a store write
becomes durable, on a #767-derived bounded deadline. A healthy run — including the cold
first-boot HuggingFace download path — proceeds deterministically; a genuine not-ready state past
the deadline → INFRA (exit 2), **never RED, never GREEN**. This makes the blocking flip safe by
removing warmup-attributable INFRA flap from the matrix (NFR-1, R-01).

No new readiness mechanism: the barrier reuses `write_then_barrier` (which already routes through
the `SMOKE_*_CMD` stub seam) on a longer deadline. The barrier's only delta over #767's measured
profile is model-load — per-slug store liveness + registration are already established by
`assert_routes_live` / C2.

## New Global Constant (add to the script-global block, ~line 59)

```
WARMUP_DEADLINE_SECS = "${WARMUP_DEADLINE_SECS:-180}"
  # #767-derived: docker-embed-readiness-smoke.sh READY_TIMEOUT_SECS=180,
  # ~2.5x over the ~70s (10s/20s/40s) embed retry/backoff floor under a real
  # cold HF download. env-overridable for arm/slow runners. Comment MUST cite
  # the #767 derivation + headroom (AC-01).
```

## New Function

```
FUNCTION warmup_barrier():
    # --- 1. Establish RUN deterministically (idempotent; reused by derive_markers) ---
    RUN = "${RUN:-$$-$(date +%s)}"          # same default shape as derive_markers (:347)
    IF RUN contains any char outside [a-z0-9-]:
        infra_fail "RUN nonce '$RUN' not [a-z0-9-]"     # never RED/GREEN

    # --- 2. Build the throwaway warmup marker ---
    WARMUP_MARKER = "infra003-warmup-${RUN}"
    IF WARMUP_MARKER contains any char outside [a-z0-9-]:
        infra_fail "warmup marker '$WARMUP_MARKER' not [a-z0-9-]"

    # --- 3. Load-bearing non-substring assertion vs the four cell markers (R-02) ---
    derive_markers()        # idempotent: sets M_OBS_A/M_OBS_B/M_MCP_A/M_MCP_B from the SAME RUN
                            # (re-invoked harmlessly inside run_isolation_matrix later)
    FOR cell IN [M_OBS_A, M_OBS_B, M_MCP_A, M_MCP_B]:
        IF WARMUP_MARKER is a substring of cell  OR  cell is a substring of WARMUP_MARKER:
            infra_fail "warmup marker '$WARMUP_MARKER' collides (substring) with cell marker '$cell' (R-02) — non-substring invariant broken"
    log "warmup marker is charset-safe and pairwise non-substring of the four cell markers. PASS"

    # --- 4. One throwaway durable warmup write, on the LONGER deadline ---
    #   Reuse write_then_barrier verbatim; widen its poll deadline to WARMUP_DEADLINE_SECS
    #   for this one call by temporarily overriding READ_DEADLINE_SECS, then restore.
    #   surface=observe, slug=SLUG_A, store_dir=SLUG_DIR_A  (see OQ-WB-1).
    saved_read_deadline = READ_DEADLINE_SECS
    READ_DEADLINE_SECS  = WARMUP_DEADLINE_SECS
    log "warmup barrier: durable warmup write to $SLUG_A (bound ${WARMUP_DEADLINE_SECS}s, #767-derived) ..."
    write_then_barrier(observe, SLUG_A, SLUG_DIR_A, WARMUP_MARKER)   # sets WTB ∈ {PRESENT, INFRA}
    READ_DEADLINE_SECS  = saved_read_deadline                        # matrix uses the tight bound again

    # --- 5. CONSUME the PRESENT signal to gate proceed-to-matrix (R-01 funnel) ---
    CASE WTB IN
        "PRESENT":
            log "warmup PRESENT — embedding path warm + store $SLUG_A write durable; proceed to matrix."
            # fall through: control returns to main, which calls run_isolation_matrix
        *:   # "INFRA" (deadline timeout) — note: a store-read failure already infra_fail'd inside write_then_barrier
            infra_fail "warmup barrier: own-store warmup write not durable within ${WARMUP_DEADLINE_SECS}s => INFRA (model not loaded / store not durable). NOT a RED, NOT a GREEN."
```

### Call-site insertion (in `main`, between `:429` and `:430`)

```
assert_routes_live           # C2 route-liveness PRECONDITION
warmup_barrier               # C-WB — NEW: model-load + durable-write readiness gate
run_isolation_matrix         # C3/C4 -> C5 -> C6 -> C7 verdict (exits)
```

## State Machine — `WTB` (reused from `write_then_barrier`)

```
       write_then_barrier(warmup)
              │
    ┌─────────┴───────────────┐
 own marker durable        deadline reached / read-fail
 (>=1 row before bound)        │
    │                          ▼
 WTB=PRESENT             WTB=INFRA  ──or── infra_fail() inside write_then_barrier
    │                          │ (on INFRA_SENTINEL read failure)
    ▼                          ▼
 proceed to matrix       infra_fail() → exit 2
```

No PRESENT→RED and no PRESENT→GREEN edge exists in the barrier; it can only proceed or INFRA.

## Initialization Sequence

The barrier introduces no boot/connection logic of its own — C2 (`setup_container`,
`register_both_and_restart`, `assert_routes_live`) has already booted the container, registered
both slugs, and confirmed per-slug dbs + 4 routes non-404 before `warmup_barrier` runs. The
barrier only adds the `WARMUP_DEADLINE_SECS` global (env-overridable, default 180) and reuses
`derive_markers` (idempotent) to obtain the non-substring check set.

## Data Flow

- **Inputs:** `RUN` (env or default), `WARMUP_DEADLINE_SECS` (env or 180), `SLUG_A`, `SLUG_DIR_A`,
  the four cell markers (`derive_markers`), and the stub-seam env `SMOKE_WRITE_CMD` /
  `SMOKE_READ_MARKER_CMD` / `READ_DEADLINE_SECS` when present.
- **Transformation:** one durable write of `WARMUP_MARKER` to slug A's store, then a
  read-as-barrier poll on the widened deadline.
- **Outputs:** sets `RUN` (consumed later by `derive_markers`), `WARMUP_MARKER` (discarded — inert
  to the matrix), and `WTB` (consumed immediately to gate proceed). On PRESENT, control returns;
  on INFRA, `infra_fail` exits 2.

## Error Handling

| Condition | Outcome |
|-----------|---------|
| `RUN` / warmup marker not `[a-z0-9-]` | `infra_fail` (exit 2) |
| Warmup marker substring-collides with a cell marker | `infra_fail` (exit 2) — R-02 false-RED guard |
| Store-read failure mid-poll (`INFRA_SENTINEL`) | `write_then_barrier` calls `infra_fail` (exit 2) directly |
| Own warmup marker absent at `WARMUP_DEADLINE_SECS` | `WTB=INFRA` → `infra_fail` (exit 2); diagnostic last-state already logged by `write_then_barrier` (R-04 diagnosability) |
| Warmup write durable in time | `WTB=PRESENT` → proceed to matrix |

The barrier NEVER calls `fail` (RED) and NEVER `exit 0` (GREEN). Not-ready is always INFRA.

## Key Test Scenarios (hints — full plan in test-plan/)

1. **Load-bearing PRESENT (R-01 sc.1):** stub `SMOKE_WRITE_CMD` + `SMOKE_READ_MARKER_CMD` so the
   warmup write round-trips (read returns ≥1) → `WTB=PRESENT` → control reaches
   `run_isolation_matrix`. The PRESENT path must exercise the SAME `SMOKE_*_CMD` a real durable
   write uses — NOT a liveness-only `store_size` poll.
2. **Funnel / consumed-not-discarded (R-01 sc.2):** confirm `WTB` is read in the `CASE` that gates
   proceed; grep the function for any computed-and-discarded result binding.
3. **Forced timeout → INFRA (AC-03, R-04):** stub read returns 0 rows + short
   `WARMUP_DEADLINE_SECS` → exit 2 (assert ≠ 0 and ≠ 1); assert the timeout diagnostic line logs.
4. **Non-substring collision trip (R-02):** force a `RUN`/marker that would make the warmup marker
   a substring of a cell marker → `infra_fail`, not a silent proceed.
5. **Stub-seam compatibility (AC-05):** with the barrier present, the full verdict truth-table
   (exit 0/1/2/3) is still reachable off-Docker through `SMOKE_*_CMD`.
6. **Cold-path determinism (AC-04 via AC-11):** the dispatch cold-model run reaches
   `run_isolation_matrix` GREEN with zero warmup-attributable INFRA flap; observed wall-clock
   carries documented headroom under `WARMUP_DEADLINE_SECS`.
7. **Diff confinement (AC-01/AC-15):** `git diff` of the smoke shows ONLY the `WARMUP_DEADLINE_SECS`
   global, the `warmup_barrier` function, and the one call line between `assert_routes_live` and
   `run_isolation_matrix`; the bound comment cites the #767 derivation.
