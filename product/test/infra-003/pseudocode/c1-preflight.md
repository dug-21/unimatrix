# C1 — Read-dependency preflight (docker / sqlite3 / vol)

> Source: ARCH C1, ADR-001, SPEC FR-06.4/AC-11, RISK R-06/R-10. SR-01/SR-03.

## Purpose

Before any container boot, write, or read, assert the dependencies the gate needs
to run faithfully. Absence is **never a verdict and never a silent empty-pass**
(`warn+continue` forbidden, #4473). Two distinct non-pass outcomes here:
- **Docker absent → SKIP exit 3** (matches posture-smoke; a deferred CI step).
- **`sqlite3` / `vol` (busybox) absent → INFRA** (a mis-provisioned lane; `sqlite3`
  is provisioned host-side like `node`). INFRA is a distinct exit, never exit 0.

`sqlite3` runs **host-side** on the `vol cat`-extracted snapshot — there is no
SQLite in the distroless image and none is added (no production change, C-01).

## Functions

### `preflight()` → exits on any failure; returns on success

```
preflight():
    # --- Docker (SKIP, not INFRA — mirrors posture-smoke :352-356) ---
    if not command_exists("docker") or not (docker info succeeds):
        echo "[infra003] SKIP: Docker not available in this environment." >&2
        echo "[infra003] This gate MUST run in a Docker-capable CI job (deferred)." >&2
        exit 3                                  # SKIP — never green (R-10)

    # --- sqlite3 (INFRA) — the content-read engine for BOTH surfaces (AC-11) ---
    if not command_exists("sqlite3"):
        infra_fail "sqlite3 not provisioned on the host — mis-provisioned lane
                    (provision like node); absence is INFRA, never an empty-pass"

    # --- busybox image presence for the vol sidecar (INFRA) ---
    # vol() mounts the data volume read-only; without busybox no read is possible.
    # Probe is a cheap no-op sidecar run BEFORE any volume exists is not possible,
    # so probe the image is pullable/present instead (does not touch a volume).
    if not (docker image inspect busybox succeeds
            or docker pull busybox succeeds):
        infra_fail "busybox image unavailable — the vol read-only sidecar cannot
                    mount the data volume (INFRA, never an empty-pass)"

    # curl + node are existing infra-001 idioms (cert-pinned POST / JSON shaping).
    # Assert them too so a missing tool is INFRA here, not an opaque later failure.
    for tool in [curl, node]:
        if not command_exists(tool):
            infra_fail "<tool> not available — required for the cert-pinned write /
                        JSON shaping path (INFRA)"

    log "preflight OK: docker, sqlite3, busybox, curl, node present."
```

### `infra_fail()` — the distinct INFRA exit (shared helper, see C7)

```
infra_fail(msg):
    printf '[infra003] INFRA: %s\n' msg >&2
    exit 2                  # distinct from RED(1) and SKIP(3); never 0 (R-10/#5180)
```

## Data Flow

- **Inputs:** host PATH (`command -v`), `docker info`, `docker image inspect busybox`.
- **Outputs:** on success, control returns to `main` (C2 boots the image). On any
  absence, the process exits with the classified code — no later component runs.
- No markers, no writes, no volume reads here (the volume does not exist yet).

## Error Handling

| Condition | Outcome | Exit |
|-----------|---------|------|
| docker missing / daemon down | SKIP (deferred CI step) | 3 |
| `sqlite3` absent | INFRA (provision like node) | 2 |
| busybox image unavailable | INFRA (no read sidecar) | 2 |
| curl / node absent | INFRA | 2 |
| all present | continue to C2 | — |

`set -euo pipefail` is in force; the explicit `command -v` guards run under
`set +e`/`set -e` only if a non-zero probe would otherwise abort with a
non-attributable message — prefer the `if ! command_exists` form which is
`set -e`-safe.

## Key Test Scenarios

1. `sqlite3` removed from PATH → INFRA exit 2 with the named reason; gate does not
   reach any write (R-06 sc.1).
2. Docker daemon stopped → SKIP exit 3, never exit 0 (R-10 sc.3).
3. busybox image absent and unpullable → INFRA exit 2, never a 0-row pass later.
4. INFRA exit code is **distinct** from RED (1) and SKIP (3) — assert exit 2
   (or whatever distinct code delivery pins) is not coerced to 0 (R-10).
5. All deps present → `preflight` returns and C2 proceeds (happy path).
