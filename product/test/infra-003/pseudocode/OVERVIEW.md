# Pseudocode Overview — infra-003 Multi-Tenant HTTP Isolation Gate

> Test-only. One standalone shell gate
> `product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh` composed of
> seven logical components (C1–C7, ADR-001), plus an in-PR delivery action
> updating the #815 new-smoke-script invariant (R-15). No `crates/` change.
> Implementation agents translate each per-component file directly into shell.

## Components & Files

| File | Component | Maps to |
|------|-----------|---------|
| `c1-preflight.md` | C1 — Read-dependency preflight (docker / sqlite3 / vol) | ADR-001, SR-01/03 |
| `c2-registration.md` | C2 — Two-slug registration + single restart + route-liveness **precondition** | ADR-004, SR-11 |
| `c3-observe-probe.md` | C3 — Observe writes, both directions (`obs-a`, `obs-b`) | ADR-002, SR-04 |
| `c4-mcp-probe.md` | C4 — MCP-write probe, both directions, per-route own `Mcp-Session-Id` | ADR-003, SR-10/R-17 |
| `c5-read-as-barrier.md` | C5 — Per-cell write + read-as-barrier positive control (retry-until-present) | ADR-002, SR-02/03 |
| `c6-two-store-read.md` | C6 — Cross-store negative read + two-store read primitive (non-substring 2×2) | ADR-002, SR-02/07 |
| `c7-verdict.md` | C7 — Verdict gate (bidirectional 2×2, positive-gates-negative, tri-state exit) | ADR-002/003, SR-10/R-10 |
| `r15-invariant-update.md` | R-15 — register the new script in the #815 invariant (same PR) | RISK R-15 |

## Component Interaction (orchestration order)

The script is a single top-level executable. `main` flow:

```
C1 preflight (docker / sqlite3 / vol / busybox)        — INFRA or SKIP on absence
  → C2 boot image, register A and B, single restart,
       route-liveness PRECONDITION (4 routes non-404)   — INFRA on absence
  → derive RUN nonce + 4 markers (C3/C4 build, C6/C7 read)
  → per cell, SEQUENTIAL (write via C3/C4 then C5 barrier):
       A-obs (C3) → C5 retry-read A.observations
       B-obs (C3) → C5 retry-read B.observations
       A-mcp (C4) → C5 retry-read A.entries
       B-mcp (C4) → C5 retry-read B.entries
  → C6 cross-store negative reads (single read each, after own PRESENT)
  → C7 verdict (per-surface 2×2, positive-gates-negative) → GREEN / RED / INFRA
```

C5 and C6 share **one** parameterized two-store read primitive (`read_marker`,
defined in C6). C5 wraps it in a bounded retry-until-present loop; C6 calls it
once for the negative cell. C7 consumes only the recorded per-cell results
(`PRESENT` / `ABSENT` / `INFRA`) — it issues no reads itself.

## Data Flow Across Boundaries

| Boundary | What crosses | Direction |
|----------|--------------|-----------|
| C1 → all | confirmed `sqlite3` / `vol` / `docker` availability (else exit) | host |
| C2 → C3/C4 | `$PORT`, `$TMP/cert.pem`, `$TOKEN` (one bearer), live 4 routes, `$HASH_DIR`, per-slug `$SLUG_DIR_A`/`$SLUG_DIR_B` | host → container HTTPS |
| build → C3/C4/C6 | `$RUN` nonce + 4 marker literals | in-process |
| C3/C4 → server | marked write (observe payload / MCP `tools/call`) per slug route | host → container |
| C5/C6 → host | `vol cat` db+`-wal`+`-shm` snapshot → host `sqlite3 -json` | container vol (ro) → host |
| C5/C6 → C7 | per-cell result token (`PRESENT`/`ABSENT`/`INFRA`/`RED`) | in-process |

## Shared Types / Variables (script-global)

Declared once in `main`, consumed by components:

```
SLUG_A      = "arch-research"   # existing constant — NOT a re-typed allowlist literal
SLUG_B      = "isolation-b"     # neutral test-scoped literal (ADR-004 / R-11)
PORT        = <host port, e.g. 18443>
VOL, CNAME  = per-run docker volume + container names ($$-suffixed)
IMAGE       = built or IMAGE= prebuilt tag
HASH_DIR    = /data/.unimatrix/<hash>   # token + tls/cert.pem live here
SLUG_DIR_A  = /data/.unimatrix/arch-research
SLUG_DIR_B  = /data/.unimatrix/isolation-b
TMP         = host mktemp -d (token, cert, db snapshots); reaped by cleanup trap
TOKEN       = one bearer (slug is in URL path; token authorizes caller, not tenant)
RUN         = per-run nonce, charset [a-z0-9-] only (e.g. "$$-$(date +%s)")

# Four mutually NON-SUBSTRING markers (ADR-002 / C-14 / R-18):
M_OBS_A = "infra003-obs-a-${RUN}"   # A observe → observations.topic_signal
M_OBS_B = "infra003-obs-b-${RUN}"   # B observe → observations.topic_signal
M_MCP_A = "infra003-mcp-a-${RUN}"   # A MCP    → entries.content (+ topic)
M_MCP_B = "infra003-mcp-b-${RUN}"   # B MCP    → entries.content (+ topic)
```

Per-cell result variables C5/C6 set and C7 reads (one per matrix cell):

```
POS_OBS_A, POS_OBS_B, POS_MCP_A, POS_MCP_B   ∈ {PRESENT, INFRA}
NEG_OBS_A, NEG_OBS_B, NEG_MCP_A, NEG_MCP_B   ∈ {ABSENT, RED, SKIPPED}
# NEG_OBS_A = "does A's store hold the FOREIGN obs marker (M_OBS_B)?" etc.
```

## Shared Shell Helpers / Idioms (reused from infra-001)

Sourced or replicated **for primitives only** — infra-001 files are NOT modified
(C-01). Per ADR-001 the gate is self-contained (SR-12): it sources only
define-on-source libs and replicates the thin executable boot idiom.

| Idiom | Source | Used by |
|-------|--------|---------|
| `vol() { docker run --rm -v "$VOL:/data:ro" busybox "$@"; }` | posture-smoke `:47` | C1, C2, C5, C6 |
| `store_size(dir)` (`vol du -s` WAL-robust) — **liveness/boot waits ONLY, never the barrier** | posture-smoke `:55` | C2 |
| `vol cat` db + `-wal` + `-shm` → host `sqlite3 -json`; sqlite3-absent = hard INFRA | `cloud-bundle-lib.sh:51-97` (`capture_behavioral_topic_signals`) | C5, C6 |
| boot / `project register` / `docker restart` / `HTTP transport active` deadline-poll | posture-smoke `:390-430` | C2 |
| `HASH_DIR` discovery (`ls -d /data/.unimatrix/*/` with a `token` file) | posture-smoke `:414` | C2 |
| cert/token pull (`vol cat $HASH_DIR/token`, `tls/cert.pem`) | posture-smoke `:441-446` | C2 |
| cert-pinned bearer `curl` POST (`--cacert`, `Authorization: Bearer`) | posture-smoke `:450-455` | C3, C4 |
| deadline-poll shape (`deadline=$(( $(date +%s)+N ))` … `sleep 1`) | posture-smoke `:322-329` | C2, C5 |
| `log()` / `fail()` exit contract | posture-smoke `:32-33` | all |

The C5 retry-until-present wrapper is the **new** part, modelled on the Gate-7
deadline-poll (`:322-329`) but keyed to a marker query instead of a `du` delta.

## Verdict State Machine — INFRA vs RED vs GREEN vs SKIP (C7)

Four distinct, non-collapsible exit states (C-12 / R-10 / #5180). No non-GREEN
outcome ever rounds to exit 0.

```
                         docker absent
            ┌──────────────────────────────────► SKIP   exit 3
            │
   [PREFLIGHT C1] ── sqlite3/vol/busybox absent ─► INFRA  exit 2  (distinct)
            │
            ▼
   [PRECONDITION C2] ── any of 4 routes 404 / store db missing /
            │            HTTP transport never active ─────► INFRA  exit 2
            ▼
   [PER-CELL  C5]  write → retry-read own marker
            │
            ├─ own marker PRESENT before deadline ─► POS_* = PRESENT
            │
            └─ own marker absent AT deadline ──────► POS_* = INFRA   (NEVER RED)
                 (durability/infra not established; per-direction non-verdict)
            │
            ▼
   [NEG CELL  C6]  evaluated ONLY after own POS_* == PRESENT
            │      (positive-gates-negative, per direction)
            │
            ├─ foreign marker ABSENT ─► NEG_* = ABSENT
            └─ foreign marker PRESENT ─► NEG_* = RED     (real leak)
            │
            ▼
   [VERDICT  C7]
       if any NEG_* == RED                          ──► RED    exit 1
       elif any POS_* == INFRA                       ──► INFRA  exit 2
       elif all 4 POS PRESENT and all 4 NEG ABSENT   ──► GREEN  exit 0
```

Decision precedence is deliberate (C7): **RED dominates INFRA**. A genuine
mis-route surfaces as RED at the cross-store cell even when that direction's
own-store positive timed out INFRA — the mis-route is never masked by an
own-store durability timeout (ADR-002 §4–5, R-05 sc.4). INFRA dominates GREEN:
any unestablished positive forbids a GREEN. SKIP (docker absent) is the only
exit-0-adjacent state and is exit 3, never 0.

> Exit-code assignment (RED=1, SKIP=3) mirrors posture-smoke exactly. INFRA gets
> a **distinct** code (proposed `2`; delivery confirms it does not collide with
> posture-smoke's `4` "image unavailable"). The load-bearing invariant: SKIP and
> INFRA never round to 0, and INFRA is distinguishable from RED (R-10/#5180).

## Sequencing Constraints (what must be built/run first)

1. C1 before everything (no write/read without confirmed deps).
2. C2 before any marked write (routes must be live; a missing route is INFRA, not
   a 0-row cell — R-07). Route-liveness probe must **not** write a marker.
3. Per cell, the C3/C4 write precedes its C5 barrier read (strict per-store
   sequence, no aggregate `du` barrier — C-08).
4. Per direction, C5 (own positive PRESENT) precedes C6 (foreign negative) —
   positive-gates-negative (FR-05 / FR-06.3).
5. C7 runs only after all cells recorded.
6. R-15 invariant update ships in the **same delivery PR** as the script (not a
   follow-up), cross-linked on #815.

## Marker Non-Substring Invariant (load-bearing, asserted at runtime)

Before any write, `main` asserts the four markers are pairwise non-substring (a
cheap self-check; R-18). They are by construction — they differ at the `obs/mcp`
and `a/b` positions before the shared `${RUN}` suffix — but the assertion makes
the load-bearing property explicit and fails LOUD (INFRA) if a future edit breaks
it. Charset is constrained to `[a-z0-9-]` so no `LIKE` wildcard (`%`/`_`) or
quote (`'`) can enter a `sqlite3` predicate (R-12). See c6/c7.
