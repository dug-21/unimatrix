# C1 — HTTPS-leg standup (shell)

**Extends:** `scripts/docker-http-posture-smoke.sh` Gates 1–7 (`emit_bundle()`, `consume_bundle()`,
`vol()`, `store_size()`, store-delta gates, sourceable guard, `[783-smoke]` marker convention).
**Net-new code:** NONE. C1 is the existing standup REUSED VERBATIM. C2 (separate file) appends the new
`cloud_cycle_gates` gate function after Gate 7. Re-authoring any spawn/cert/bundle path here is a FORK
SMELL to flag (SR-04 / AC-07 / R-10).

## Purpose

Bring up the shipped HTTPS image HTTP-on, register a slug pre-serve, restart, read the leaf cert + bearer
off the data volume, emit a v:2 bundle, and `init --bundle` into the hermetic `$SANDBOX/home` — so the
credstore + token-free `.mcp.json` exist for the bridge (C2). Satisfies FR-1 / AC-01. Each step is an
EVENT-DRIVEN readiness gate (no `sleep`, SR-01).

## Reused standup sequence (verbatim — these gates already exist)

```
GATE 1  acquire image      docker pull "$IMAGE" || docker image inspect "$IMAGE" >/dev/null 2>&1 || exit 4   # ADR-005/#5208 — pull-then-inspect, never inspect-only
        HTTP-on assert      docker image inspect "$IMAGE" --format '{{json .Config.Env}}' | grep -q 'UNIMATRIX_HTTP_ENABLED=true'
        boot HTTP-on        docker run ... -e UNIMATRIX_PUBLIC_URL="https://localhost:18443" ...
        READINESS          poll daemon log for "HTTP transport active"   # NOT a sleep
GATE 2-4 register slug     docker run --rm -v "$VOL:/data" "$IMAGE" --project-dir /data project register "$SLUG"
        restart            docker restart "$CNAME" ; READINESS: listener bound (poll)
        store gates        per-slug store file exists; store grew (store_size / du -s over the slug DIR)
GATE 5  emit bundle        emit_bundle()   # UNIMATRIX_PUBLIC_URL set; Gate-5 placeholder guard rejects "<EDIT-ME>"
        ── stderr of THIS child stays SUPPRESSED (blob carries the bearer; R-13/security exception) ──
GATE 6  init --bundle      consume_bundle() → HOME="$SANDBOX/home"  init --bundle <blob> --project-dir "$SANDBOX/proj"
                           writes credstore ~/.unimatrix/<projectHash>/remote.json (mode 0600) + token-free .mcp.json
        READINESS          remote.json present (mode 0600) before any bridge spawn
GATE 7  read cert+bearer   vol cat "$HASH_DIR/tls/cert.pem" > "$TMP/cert.pem" ; vol cat "$HASH_DIR/token" > "$TMP/token"
                           READINESS: cert.pem present + non-empty BEFORE any pinned client runs
```
`HASH_DIR` is discovered by the existing smoke logic (lists `/data/.unimatrix/*/` for the dir holding a
`token`). C1 stops at Gate 7's verified credstore + cert-on-host; C2 picks up from the credstore.

## C1 → C2 handoff (data crossing the boundary)

C1 leaves these ready under `$SANDBOX` for C2:
- `$SANDBOX/home/.unimatrix/<projectHash>/remote.json` (credstore, mode 0600) — C2 reads `projectHash`
  back from here (single dir under `$SANDBOX/home/.unimatrix/`), NEVER recomputes it (OQ1/R-11).
- `$TMP/cert.pem`, `$TMP/token` — leaf cert + bearer for any pinned `/observe` curl in C2.
- `$SLUG`, `$CNAME`, `$VOL`, HTTPS port `18443`, per-slug store DIR — already in smoke scope.

## Initialization / config

- All standup runs behind the sourceable guard:
  `if [ "${BASH_SOURCE[0]}" != "${0}" ]; then return 0 ...; fi` — so stub-drive (C5/R-12) can source the
  file for gate-fns without executing standup.
- Hermetic `$SANDBOX` + `$SANDBOX/home` are the existing smoke's per-run temp; C2 reuses them.

## Error handling

- Image unacquirable → `exit 4` (distinct from Docker-absent `exit 3`, owned by C5) — already wired.
- Missing/empty cert, missing remote.json, placeholder bundle URL → `fail "<cause>"` (exit 1) with the
  existing `[783-smoke] FAIL:` prefix; child stderr (except emit) tail-dumped from `$SANDBOX` (ADR-005).
- `emit_bundle` stderr deliberately suppressed — assert this is intentional, not an oversight (R-13).

## Key test scenarios (hints for tester)

- AC-01: assert HTTP-on path (registered `[[projects]]` slug, NOT `serve --stdio`); assert NO third
  server-spawn/cert/bundle path was added (diff review — C1 is verbatim reuse) (R-10).
- Readiness gates are event-driven (log line / file present), zero fixed `sleep` between links (SR-01).
- `emit_bundle` child stays suppressed; all OTHER children capture stderr to `$SANDBOX` (R-13).
- credstore written mode 0600 ONLY under `$SANDBOX/home`; no real `~/.unimatrix` touched (R-14).
