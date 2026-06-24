# C1 — HTTPS-Leg Standup (shell) — Test Plan

> **Component:** boot the shipped image HTTP-on, register a `[[projects]]` slug pre-serve, restart, read
> the leaf cert + bearer off the data volume (busybox sidecar), emit a bundle, `init --bundle` into a
> hermetic sandbox. **Extends** `docker-http-posture-smoke.sh` Gates 1–7 (`emit_bundle`, `consume_bundle`,
> `vol`, store-delta). NO third server-spawn/cert/bundle path.
> **ACs:** AC-01 (primary), AC-06/AC-07 (cumulative, no fork). **Risks:** R-10, R-11, R-13, R-14.

---

## What is exercised here (live, on the tag run)

The standup is a precondition for C2 — but its readiness gates and hermeticity are independently testable.
Most C1 assertions are **structural** (diff/grep/file-mode checks on the shell + emitted artifacts) plus
the **event-driven readiness gates** asserted inside the live smoke run.

---

## Test Expectations

### AC-01 — cloud path stood up cumulatively (live + file-check)

- **`test_c1_serves_http_not_stdio`** (live, in-smoke): assert the daemon is started HTTP-on (`serve` with
  `UNIMATRIX_HTTP_ENABLED=true`) — assert via `docker image inspect "$IMAGE" --format '{{json .Config.Env}}'
  | grep -q 'UNIMATRIX_HTTP_ENABLED=true'` (smoke L336–338) — **NOT `serve --stdio`**. Assert no `--stdio`
  arg appears in the C1/C2 spawn path.
- **`test_c1_slug_registered_pre_serve`** (live): `docker run ... --project-dir /data project register
  "$SLUG"` runs BEFORE the listener serves (register-pre-serve, #5098 trap); then `docker restart "$CNAME"`.
  Assert the `/v1/{slug}/...` route answers after restart (Gate 2 pinned POST → 204 precedent).
- **`test_c1_leaf_cert_pinning`** (live): the busybox sidecar reads `vol cat /data/.unimatrix/<hash>/tls/
  cert.pem` and `vol cat .../token`; a pinned `curl --cacert "$TMP/cert.pem"` against
  `https://localhost:18443/v1/${SLUG}/observe` returns 204 (Gate 2 contract). Leaf-fingerprint pin is the
  trust boundary — exercised by the real pinned handshake, NOT shape-asserted (#4970).
- **File-check (AC-07 fork audit):** the standup EXTENDS Gates 1–7 — assert no net-new `docker run` server
  spawn, no net-new cert read, no net-new bundle emit path beyond `emit_bundle`/`consume_bundle`.

### Readiness gates — event-driven, NEVER fixed sleeps (R-13/SR-01, NFR-3)

Assert each link is gated on an OBSERVABLE condition, not a `sleep`:

- **`test_c1_gate_http_active_log_poll`**: boot → poll daemon log for `"HTTP transport active"` (not sleep).
- **`test_c1_gate_listener_bound_after_restart`**: after `docker restart` → poll until listener bound.
- **`test_c1_gate_cert_present_before_pinned_client`**: assert `cert.pem` is present + NON-EMPTY before any
  pinned client runs.
- **`test_c1_gate_remote_json_present_mode_0600`**: after `init --bundle` → assert `remote.json` present at
  mode `0600` under the hermetic `$HOME` BEFORE the bridge spawns (this is also the C2 precondition).

### R-11 — `projectHash` read-back, never recomputed (structural grep — AC-07)

- **`test_c1_projecthash_read_back_not_recomputed`**: assert the `projectHash` later passed to
  `node mcp-bridge.js <projectHash>` is **READ BACK** from `init --bundle` output (stdout/log) OR by listing
  the single directory under `$SANDBOX/home/.unimatrix/` after consume — NOT recomputed in the fixture.
- **`test_c1_no_hashing_primitive_in_path`**: grep the C1 standup path — assert ZERO hashing primitive
  (sha256/hash invocation) is imported or invoked. Net-new hashing is a fork smell (SR-04).

### R-13 — capture-first child stderr (structural grep)

- **`test_c1_children_capture_stderr`**: assert every child (`init`, container, sidecar) writes stderr to a
  `$SANDBOX` file, tail-dumped on FAILURE only — never `2>/dev/null` on a token-free child.
- **`test_c1_emit_bundle_stays_suppressed`**: assert the `emit_bundle` child's stderr STAYS suppressed (its
  blob carries the bearer) — the ONE deliberate exception, asserted as intentional, never logging the bearer.

### R-14 — hermeticity (structural)

- **`test_c1_credstore_confined_to_sandbox_home`**: assert `init --bundle` writes the credstore ONLY under
  `$HOME=$SANDBOX/home` (nan-020 precedent #5258); the sandbox home is FRESH per run.
- **`test_c1_no_global_unimatrix_access`**: assert no real `~/.unimatrix` outside the sandbox is read or
  written (no bridge attaching to the wrong server).

---

## Edge cases

- Cross-runner image cache miss handled by C5's acquisition (covered there) — C1 assumes the image is
  acquired.
- Empty/absent `cert.pem` → the readiness gate HARD-fails before any pinned client runs (no race into an
  unpinned connection).

## Integration boundary

C1 hands C2 a live HTTPS endpoint + a credstore (mode 0600) under the hermetic `$HOME` and the read-back
`projectHash`. The boundary assertion is `test_c1_gate_remote_json_present_mode_0600` — the gate the bridge
spawn waits on.
