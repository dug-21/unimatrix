# C2 — Two-slug registration + single restart + route-liveness precondition

> Source: ARCH C2, ADR-004, SPEC FR-01/AC-01, RISK R-07/R-11. SR-11/SR-08.

## Purpose

Boot the shipped multi-slug container, register **both** slugs A=`arch-research`
and B=`isolation-b` **before a single restart** (routing config read once at boot,
#5079), then assert all four routes respond non-404 as a **precondition only**.
Route-liveness is **not** the isolation verdict — a mis-resolved route still
responds non-404 (C-06). Liveness exists to fail loud on the unregistered-B trap;
the behavioral verdict is the C5/C6/C7 content read. The liveness probe must
**not** write a marker (it would pollute the stores the verdict reads).

`store_size()` is reused here for boot/liveness waits **only** — explicitly NOT
the durability barrier (ADR-004 §5; the barrier is C5's read-as-barrier).

## Initialization Sequence

```
setup_container():
    IMAGE := (IMAGE env if set, pulled-or-local-fallback as posture-smoke :363-378)
             else docker build -t unimatrix:infra003-smoke "$REPO_ROOT"
    assert image ENV carries UNIMATRIX_HTTP_ENABLED=true   # posture-smoke :381-383
    docker volume create "$VOL"

    # Boot 1: clean docker run, NO -e UNIMATRIX_HTTP_ENABLED (image ENV must enable).
    docker run -d --name "$CNAME" -v "$VOL:/data" \
        -e UNIMATRIX_PUBLIC_URL="https://localhost:${PORT}" \
        -p "${PORT}:8443" "$IMAGE"
    wait_for_http_active()                      # deadline-poll, posture-smoke :400-410

    HASH_DIR := discover_hash_dir()             # posture-smoke :414-417
```

### `wait_for_http_active()` — reused boot deadline-poll (posture-smoke :400-410)

```
wait_for_http_active():
    deadline := now + 90
    loop:
        if docker logs "$CNAME" contains "HTTP transport active": return
        if docker logs "$CNAME" contains "set [http] enabled":
            infra_fail "daemon logged HTTP-disabled hint => booted HTTP-OFF"
        if now > deadline:
            infra_fail "HTTP listener never became active (boot failed)"
        sleep 2
```

### `discover_hash_dir()` — reused (posture-smoke :414-417)

```
discover_hash_dir():
    d := vol sh -c 'ls -d /data/.unimatrix/*/ | while read d; do
                       [ -f "$d/token" ] && echo "$d"; done | head -1'
    d := strip_trailing_slash(d)
    if empty(d): infra_fail "could not locate path-hash data dir (no token found)"
    return d
```

## Registration + Single Restart (ADR-004 §1-3, C-05)

```
register_both_and_restart():
    # Both slugs registered BEFORE the one restart (routing read once at boot).
    # Slug literals come from the SLUG_A/SLUG_B script-globals — NOT re-typed
    # ADR-004 allowlist regex copies (SR-08 / AC-13). A reuses arch-research.
    for slug in [SLUG_A, SLUG_B]:
        docker run --rm -v "$VOL:/data" "$IMAGE" --project-dir /data \
            project register "$slug"
            on failure: infra_fail "project register <slug> failed"

    docker restart "$CNAME"
    wait_for_http_active()                       # second wait (ADR-004 consequence)
```

## Route-liveness PRECONDITION (ADR-004 §4, FR-01.2/3, NOT the verdict)

```
assert_routes_live():
    # Pull cert + ONE bearer token (slug is in the path; one token serves all 4).
    vol cat "$HASH_DIR/token"        > "$TMP/token"
    vol cat "$HASH_DIR/tls/cert.pem" > "$TMP/cert.pem"
    TOKEN := trim($TMP/token); assert non-empty (else infra_fail "empty token")
    assert "$TMP/cert.pem" non-empty (else infra_fail "empty TLS cert")

    # Assert per-slug store dbs EXIST before trusting any cell (R-07 sc.2/3):
    # a missing B db at read time must be INFRA, not a phantom 0-row. Register
    # creates the store; confirm both are real files now.
    SLUG_DIR_A := "/data/.unimatrix/${SLUG_A}";  SLUG_DIR_B := "/data/.unimatrix/${SLUG_B}"
    for db in ["$SLUG_DIR_A/unimatrix.db", "$SLUG_DIR_B/unimatrix.db"]:
        vol test -f db  or  infra_fail "per-slug store <db> missing post-restart
                                        (registration/route never built — INFRA)"

    # Probe all FOUR routes non-404 WITHOUT writing a marker (ADR-004 §4, C-06):
    #   observe: a HEAD/OPTIONS or a benign GET — any non-404 status proves the
    #     route exists. Do NOT POST a RecordEvent here (it would seed a marker).
    #   mcp: a benign request to /v1/{slug}/mcp; non-404 proves the route is built.
    #     (A full MCP handshake is C4; here we only need "route exists".)
    for route in [ /v1/A/observe, /v1/B/observe, /v1/A/mcp, /v1/B/mcp ]:
        code := curl --cacert "$TMP/cert.pem" -H "Authorization: Bearer $TOKEN" \
                     -o /dev/null -w '%{http_code}'  <non-writing method>  route
        if code == 404:
            infra_fail "route <route> is 404 after restart — slug never built a
                        route (unregistered-B trap); INFRA, not an isolation pass"
        record liveness(route)=non-404 as a PRECONDITION (log only, NOT a verdict)

    log "all 4 routes non-404 (PRECONDITION only — non-404 ≠ isolated). PASS C2"
```

> Implementation note for the liveness method: pick a method/path that the router
> answers non-404 for a live route but that does **not** persist a row. An
> unauthenticated/benign probe that returns 401/405/400 for a live route is still
> a valid non-404 liveness signal; only 404 means "route absent". The tester
> confirms the exact non-writing probe per surface (open detail, see OQ below).

## Data Flow

- **Inputs:** `$IMAGE`, `$VOL`, `$CNAME`, `$PORT`, `$SLUG_A`, `$SLUG_B`.
- **Outputs (to C3/C4/C5/C6):** `$HASH_DIR`, `$SLUG_DIR_A`, `$SLUG_DIR_B`,
  `$TMP/cert.pem`, `$TOKEN`, live container on `$PORT`, confirmed-present per-slug
  store dbs.
- No markers written. `store_size()` may be sampled for boot/liveness waits but is
  never compared as a durability barrier (ADR-004 §5).

## Error Handling

| Condition | Outcome |
|-----------|---------|
| image lacks `UNIMATRIX_HTTP_ENABLED=true` | INFRA |
| `project register` fails for A or B | INFRA |
| HTTP transport never active (boot or restart) | INFRA |
| any per-slug store db missing post-restart | INFRA (R-07; never a 0-row cell) |
| any of the 4 routes 404 | INFRA (unregistered-B trap; never an isolation pass) |
| empty token / empty cert | INFRA |
| all routes non-404, both dbs present | continue to writes |

## Key Test Scenarios

1. Both A and B registered before the single restart; both store dbs exist
   afterward (R-07 sc.3).
2. B registered but route 404 (e.g. config not applied) → INFRA at the
   precondition, never a vacuous GREEN (R-07/SR-11).
3. Missing B `unimatrix.db` at read time is INFRA, not a 0-row cell (R-07 sc.2).
4. Liveness probe writes NO marker — a fresh cross-store read after C2 (before any
   write) returns 0 rows for all four markers (baseline sanity; R-08 sc.3).
5. Non-404 is recorded as a precondition only; it never sets any `POS_*`/`NEG_*`
   verdict variable (C-06).
6. No pre-existing `isolation-b` store on the volume before the run (R-11 sc.1).

## Open Question (for tester / Stage 3c)

- The exact **non-writing** liveness probe per surface (method/path that yields
  non-404 for a live route without persisting a row). ADR-004 fixes the
  requirement (non-404, no marker); the literal request is a tester detail.
