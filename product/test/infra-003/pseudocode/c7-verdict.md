# C7 — Verdict gate: bidirectional 2×2, positive-gates-negative, tri-state exit

> Source: ARCH C7, ADR-002 §7, SPEC FR-04/FR-05/AC-05/AC-09, RISK R-03/R-10.
> SR-10/SR-12. The central integrity invariant and the script's exit contract.

## Purpose

Consume the eight recorded cell results (`POS_*` from C5, `NEG_*` from C6) and emit
one of four distinct, non-collapsible outcomes: **GREEN / RED / INFRA / SKIP**. No
non-GREEN outcome ever rounds to exit 0 (C-12 / R-10 / #5180). The verdict applies
the matrix **per surface, per direction**: each store holds only its own slug's
marker (present in own, absent in other), both directions.

C7 issues **no reads** — it only evaluates the in-process result variables. (SKIP
for docker-absence and INFRA for preflight/precondition failures already exited in
C1/C2; C7 handles the post-write disposition.)

## Verdict logic (decision precedence)

```
verdict():
    # Cells (set by C5/C6):
    #   POS_OBS_A POS_OBS_B POS_MCP_A POS_MCP_B   ∈ {PRESENT, INFRA}
    #   NEG_OBS_A NEG_OBS_B NEG_MCP_A NEG_MCP_B   ∈ {ABSENT, RED, SKIPPED}
    #   (NEG_X = "does store X hold the FOREIGN marker for that surface?")

    # 1. RED DOMINATES — any cross-store leak is a hard isolation failure,
    #    independent of the positive outcomes (a mis-route is RED even when its
    #    own-store positive timed out INFRA). ADR-002 §4-5 / R-05 sc.4.
    if any(NEG_*) == RED:
        for each NEG_* == RED: log_red_cell(name)        # attribute every leak
        fail "ISOLATION BROKEN — cross-store marker present (see cells above)"   # exit 1

    # 2. INFRA DOMINATES GREEN — any unestablished own positive forbids a pass.
    #    An own-store positive timeout is a per-direction non-verdict, never a
    #    vacuous GREEN (AC-10 / FR-05.2). Distinct exit from RED.
    if any(POS_*) == INFRA:
        for each POS_* == INFRA: log "INFRA: own positive <name> never reached PRESENT"
        infra_fail "durability/precondition not established for >=1 direction —
                    INFRA (not an isolation pass, not a RED)"                     # exit 2

    # 3. GREEN — every positive PRESENT in its own store AND every cross-cell ABSENT.
    #    (If we reach here, no RED and no INFRA, so every NEG_* is ABSENT and every
    #    POS_* is PRESENT — positive-gates-negative is satisfied by construction:
    #    a SKIPPED negative only arises with an INFRA positive, already excluded.)
    assert all(POS_*) == PRESENT and all(NEG_*) == ABSENT
    log_surface_verdict(observe): "A has obs-a not obs-b; B has obs-b not obs-a => observe GREEN"
    log_surface_verdict(mcp):     "A has mcp-a not mcp-b; B has mcp-b not mcp-a => mcp GREEN"
    log "ALL GATES PASSED — bidirectional 2x2 isolation holds on both surfaces."
    exit 0                                                                        # GREEN
```

### Why positive-gates-negative holds (FR-05 / AC-05/AC-09)

The gating is enforced jointly by C6 and C7:
- C6 returns `SKIPPED` (not `ABSENT`) for a foreign-absent cell whose own positive
  is INFRA — so a clean cross read on a silently-failed own-write is **never**
  labeled a pass.
- C7 step 2 converts any INFRA positive into an INFRA verdict before any GREEN can
  be claimed. So no surface passes unless **both** directions' positives are
  PRESENT **and** both negatives ABSENT (FR-05.3).

A direction's RED is independent: step 1 fires on any leak regardless of the
positives (the mis-route is never masked).

## Exit contract / state machine (C-12, R-10, #5180)

| State | Trigger | Exit | Helper |
|-------|---------|------|--------|
| **SKIP** | docker absent (C1) | 3 | direct exit in C1 |
| **INFRA** | sqlite3/vol absent (C1); route 404 / missing db / HTTP-never-active (C2); any own positive timeout (C5→C7 step 2) | 2 | `infra_fail` |
| **RED** | any cross-store marker present (C7 step 1) | 1 | `fail` |
| **GREEN** | all 4 positives PRESENT + all 4 negatives ABSENT | 0 | C7 step 3 |

```
fail(msg):        printf '[infra003] FAIL: %s\n' msg >&2;  exit 1   # RED
infra_fail(msg):  printf '[infra003] INFRA: %s\n' msg >&2; exit 2   # INFRA (distinct)
# SKIP uses exit 3 with a clear reason (C1).
```

> RED=1 and SKIP=3 mirror posture-smoke exactly. INFRA=2 is the **distinct** code
> (delivery confirms 2 does not collide with posture-smoke's `4` "image
> unavailable" if both run in one lane). Load-bearing: SKIP and INFRA never round
> to 0; INFRA is distinguishable from RED (R-10). The single terminal run-marker
> `ALL GATES PASSED` is emitted only on GREEN (verify-by-name contract, #5180).

## Self-containment (SR-12 / R-13)

C7 (and the whole script) is a **separate** top-level script with self-contained
assertions. It sources only define-on-source libs for primitives and does not
graft onto posture-smoke's Gates 1–4 flow, so an upstream posture-smoke change
surfaces here as an explicit failure, not a silent skip.

## Data Flow

| In | Out |
|----|-----|
| `POS_OBS_A/B`, `POS_MCP_A/B`, `NEG_OBS_A/B`, `NEG_MCP_A/B` | one of exit 0/1/2/3 with an attributable terminal message |

No reads, no writes, no network. Pure evaluation of recorded results.

## Key Test Scenarios

1. All 4 positives PRESENT + all 4 negatives ABSENT → GREEN exit 0, terminal
   `ALL GATES PASSED` emitted (Failure-Modes table).
2. Any cross-store marker present → RED exit 1, with the leaking cell attributed;
   RED fires even when that direction's own positive is INFRA (R-05 sc.4 / R-03).
3. Any own positive INFRA (and no RED) → INFRA exit 2, distinct from RED, never a
   vacuous GREEN (AC-10 / R-10 sc.1).
4. Docker absent → SKIP exit 3 (C1); SKIP/INFRA never round to exit 0 (R-10 sc.3).
5. Positive-gates-negative: a forced own-positive failure for one direction yields
   INFRA/RED for that direction and never an ABSENT-pass on its cross cell
   (R-03 sc.1-2 / AC-05/AC-09).
6. The four surfaces/directions are independent — one direction's RED does not let
   another pass on residue; distinct non-substring markers make cross-attribution
   impossible (R-03 sc.3).
7. Exactly three (plus SKIP) distinct exit states; no non-GREEN maps to exit 0
   (R-10 / #5180).
