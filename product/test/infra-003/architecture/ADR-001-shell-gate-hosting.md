## ADR-001: Host the isolation test as a standalone shell gate reusing infra-001 primitives

### Context

infra-003 must drive the shipped multi-slug container's real HTTP routing seam,
read two per-slug SQLite stores on a distroless volume, and gate a
positive-control-before-negative-control verdict. SCOPE Q3 asks whether to host
this as a **new shell gate** (mirroring `docker-http-posture-smoke.sh`) or a
**pytest suite**. ass-084 H1 (the recommended option) leans shell; H2 (a pytest
orchestrator dual-leg) is explicitly deferred. The constraint is test-only,
cumulative on infra-001, no fork, no new scaffold.

The reusable primitives the test needs already exist as **shell** idioms in the
infra-001 harness, not in Python:
- `vol()` busybox sidecar for distroless-volume inspection (`docker exec` is
  impossible — no shell in the runtime image).
- the cert/token pull, `HASH_DIR`/`SLUG_DIR` conventions, boot/register/restart,
  and deadline-poll idioms (`docker-http-posture-smoke.sh`).
- the `vol cat` db+`-wal`+`-shm` → host-side `sqlite3 -json` content-read pattern
  with sqlite3-absent hard-INFRA discipline (`cloud-bundle-lib.sh`).

The infra-001 pytest harness drives MCP over **stdio**, not HTTP-over-TLS against
a running container; it has no `vol`/cert/two-store-read machinery. A pytest host
would re-implement all of the above, which is a new scaffold — the rejected H2
shape — and would couple the property to a heavier orchestrator than it needs.

SR-12 warns that grafting onto the posture-smoke Gates 1–4 flow risks an upstream
change silently altering this gate.

### Decision

Host infra-003 as a **new, standalone shell gate**:
`product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh` — a separate
top-level script alongside `docker-http-posture-smoke.sh`, **not** a new
subdirectory scaffold and **not** an appended Gate inside the posture smoke.

- It **reuses** infra-001 primitives by sourcing only the
  define-on-source libraries (the `cloud-bundle-lib.sh` content-read helpers
  follow the "only DEFINES functions on source" contract), and by replicating the
  thin `vol()`/cert-pull boot idiom where those live in the executable smoke.
- Its assertions are **self-contained**: it boots its own container/volume, does
  its own two-slug registration, and owns its own verdict. It does not depend on
  the posture-smoke Gates 1–4 having run, so an upstream posture-smoke change
  surfaces here as an explicit failure, never a silent skip (SR-12).
- It follows the established exit contract: `fail()` → exit 1 for RED, exit 3 for
  Docker-absent SKIP (matching posture-smoke), and a distinct hard-INFRA `fail`
  for an absent read dependency (sqlite3) — INFRA is never rounded to GREEN.
- Pytest hosting (H2) is **rejected** for this feature: it would re-implement the
  `vol`/cert/sqlite3 primitives (new scaffold) for no property gain.

### Consequences

- **Easier:** direct reuse of the exact `vol`/cert/sqlite3/boot idioms with no
  re-implementation; the smallest faithful, cumulative change (ass-084 H1); the
  gate reads naturally next to the posture smoke it mirrors.
- **Easier:** self-containment means the gate's RED/INFRA/GREEN discrimination is
  not entangled with the posture-smoke truth table (SR-12).
- **Harder:** shell offers weaker structured assertions than pytest; mitigated by
  the disciplined `fail()` contract and explicit count comparisons (ADR-002).
- **Harder:** the streamable-HTTP MCP handshake (ADR-003) is more awkward in
  shell+curl than in a Python client; accepted as the cost of not building a new
  scaffold, and bounded to the C4 probe only.
- If a future feature needs a richer dual-leg orchestrator, H2/pytest can be
  revisited then; this ADR does not foreclose it, it defers it (SR-09).

Related: ADR-002 (content-read primitive this gate hosts), ADR-003 (the MCP probe
construction), ADR-004 (registration ordering).
