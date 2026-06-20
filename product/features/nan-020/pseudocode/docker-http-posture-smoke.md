# Component: docker-http-posture-smoke.sh — Gates 5–7 extension

> **SAME-FILE COUPLING:** this component and `hermeticity-sandbox.md` edit the SAME file and
> MUST be implemented by ONE agent (see OVERVIEW "SAME-FILE COUPLING"). This file owns the
> Gate 5/6/7 LOGIC; `hermeticity-sandbox.md` owns the SANDBOX lifecycle that Gates 6–7 run
> inside and the negative control. Read both before editing.

## Purpose

Append, after the existing Gate 4 and BEFORE the terminal `ALL GATES PASSED` marker, the
documented operator attach round-trip:
- **Gate 5** — emit a connection bundle from the SAME booted container (Rust `client-bundle`).
- **Gate 6** — consume it on the host with `init --bundle` inside the hermetic sandbox.
- **Gate 7** — fire one hook event through the wired client; assert observe 204 + per-slug store grew.

Strictly append-only. Gates 1–4, the `IMAGE=` acquisition arm, the exit-3 preflight, the
`fail()`/`log()` helpers, `store_size()`, `vol()`, and the terminal marker are UNCHANGED.

## Integration surface consumed (verified, no behavior change)

- `fail() { printf '[783-smoke] FAIL: %s\n' "$*" >&2; exit 1; }` — REUSE for every new failure.
- `log()` — REUSE for PASS lines.
- `store_size "$dir"` — REUSE for the Gate 7 delta (busybox `du -s`, WAL-robust).
- Existing vars in scope at append point: `IMAGE`, `VOL`, `SLUG`, `PORT`, `SLUG_DIR`,
  `CNAME` (booted container still running), `TMP` (cert/token already extracted).
- Bundle blob format, exit table, messages, marker: see OVERVIEW shared contracts A–G.

## Placement

Insert between existing line ~183 (`log "... PASS gate 4 (AC-05)"`) and line ~185
(`log "ALL GATES PASSED — ..."`). The sandbox setup + trap from `hermeticity-sandbox.md`
is established at the TOP of this inserted block (before Gate 6).

## Pseudocode

```
# ===================== nan-020 Gates 5–7: documented bundle attach =====================
# Append-only after Gate 4. Reuses the SAME container/volume/slug/port/cert as Gates 1–4.

# ---- Host preflight: node must be present (defense-in-depth behind setup-node@v4) ----
# Placed here (not at the top Docker preflight) is acceptable because it gates only the
# new path; but per RISK R-04 sc.2 it MUST run BEFORE Gate 6 can no-op. node-absence is a
# mis-provisioned lane => hard-fail exit 1 (NOT exit 3 — that code is Docker-only).
IF NOT command_exists("node"):
    fail "node not available — the documented init --bundle path cannot be exercised"

# ---- GATE 5: emit the connection bundle from the booted image (Rust, in-container) ----
# Capture STDOUT ONLY. stderr carries the token-redacted echo and MUST NOT be folded into
# the blob (R-05 sc.1 / security: never log a bearer token). Capture rc WITHOUT a pipe so
# set -e/pipefail cannot swallow it (R-03/#4873 class):
set +e
BUNDLE="$(docker run --rm -v "$VOL:/data" "$IMAGE" --project-dir /data client-bundle "$SLUG" 2>/dev/null)"
emit_rc=$?
set -e
IF emit_rc != 0:
    fail "client-bundle emit failed (rc=$emit_rc) — subcommand renamed/absent in shipped image?"

# Blob shape validation at the boundary (R-05 sc.2/sc.4): non-empty AND correct prefix.
# Use a prefix test, not a substring match, and quote $BUNDLE everywhere (R-05 sc.3 — no
# word-splitting, blob may contain shell-significant chars):
IF BUNDLE is empty OR BUNDLE does NOT start with "unimatrix-bundle:":
    fail "client-bundle produced no/invalid bundle blob"
log "client-bundle emitted a unimatrix-bundle: blob. PASS gate 5"

# ---- SANDBOX SETUP (owned by hermeticity-sandbox.md) ----
# SANDBOX="$(mktemp -d)"; clean-on-entry guard; HOME/proj subdirs; trap-extended cleanup.
# See hermeticity-sandbox.md. After this block $SANDBOX/home and $SANDBOX/proj exist fresh.

# ---- GATE 6: consume the bundle on the host, HERMETICALLY (JS, repo-checkout client) ----
# Process-boundary isolation: HOME + --project-dir set on the SPAWNED CHILD only. The
# harness never mutates its own HOME (Rust-2024 forbids in-process set_var — ADR-005).
# repo-checkout client (NFR-4): packages/unimatrix/bin/unimatrix.js, NOT a global npm install.
# NO --slug (retired on bundle path, init.js:353). Capture rc without a pipe.
set +e
HOME="$SANDBOX/home" \
  node "$REPO_ROOT/packages/unimatrix/bin/unimatrix.js" \
       init --bundle "$BUNDLE" --project-dir "$SANDBOX/proj" >/dev/null 2>&1
init_rc=$?
set -e
IF init_rc != 0:
    fail "init --bundle failed (rc=$init_rc) — bundle attach broken"
log "init --bundle attached against the booted image (hermetic HOME). PASS gate 6"

# ---- GATE 7: fire one hook event through the wired client; assert observe + store grow ----
# Fresh BEFORE sample of the per-slug store, taken NOW (after Gates 1–4 already wrote), so
# the delta attributable to THIS hook fire is isolated (R-07 sc.4 — fresh-write evidence):
BUNDLE_BEFORE="$(store_size "$SLUG_DIR")"

# Fire ONE hook event through the SAME isolated HOME so the hook client reads THIS run's
# credstore ($SANDBOX/home/.unimatrix/<hash>/remote.json), never the runner's real ~/.unimatrix
# (R-07 sc.1). The client reads observe_url from the bundle store and POSTs verbatim — the
# doc-test proves the SERVER-COMPOSED path, not a hand-built URL.
# Invocation shape: pipe a minimal hook stdin JSON to the repo-checkout hook client entry,
# under the isolated HOME, capturing the HTTP outcome. The hook client is fail-open (exit 0
# always), so DO NOT rely on its exit code for the 204 assertion — assert via the store delta
# AND, where the client surfaces it, the observe HTTP code. (Implementation note for the dev:
# the hook client logs/returns the observe status; capture it. If only the store delta is
# observable, the delta IS the load-bearing assertion and the HTTP-code message is best-effort.)
set +e
observe_code="$( printf '%s' '<minimal hook event JSON for one observable event>' \
                  | HOME="$SANDBOX/home" node "$REPO_ROOT/packages/unimatrix/.../hook-client/index.js" <EVENT> \
                  ; capture the observe HTTP code the client reports )"
set -e
# Distinguish doc-drift from route change (R-02 sc.2 / SR-09):
IF observe_code is known AND observe_code != "204":
    fail "documented bundle attach observe returned HTTP $observe_code (expected 204)"

# Load-bearing, un-retryable, non-skip assertion (R-07 sc.4 / #4977): the per-slug store
# grew by THIS run's write. A delta of 0 = the attach silently no-opped => RED.
BUNDLE_AFTER="$(store_size "$SLUG_DIR")"
IF NOT (BUNDLE_AFTER > BUNDLE_BEFORE):
    fail "bundle-path observe did not land in per-slug store"
log "bundle-path observe landed (store $BUNDLE_BEFORE -> $BUNDLE_AFTER). PASS gate 7"
# ===================== end nan-020 Gates 5–7 =====================
```

The existing terminal line `log "ALL GATES PASSED — ..."` follows UNCHANGED and stays last.

## Data flow

```
container (Rust) --stdout blob--> BUNDLE (host shell var, quoted)
BUNDLE --child env HOME + --project-dir--> init --bundle (host JS) --> $SANDBOX/home credstore
hook event stdin --same isolated HOME--> hook client --> POST observe_url (from bundle) --> 204
per-slug store /data/.unimatrix/<slug> --store_size delta--> Gate 7 assertion
```

## Error handling (every path is a hard `fail()` exit 1 — never exit 0, never skip)

See OVERVIEW table B for the exact messages. Each failure mode has exactly one distinct
message tail. rc is captured with `set +e; …; rc=$?; set -e` (no pipe) on every command
whose failure must be attributable, so `set -e`/`pipefail` cannot mask it (R-03/R-11).

## Hermeticity hooks (delegated)

The `SANDBOX` lifecycle, the entry-clean guard, and the cleanup-trap EXTENSION are specified
in `hermeticity-sandbox.md`. Gate 6/7 above merely CONSUME `$SANDBOX/home` and
`$SANDBOX/proj`. The existing `cleanup()`/`trap cleanup EXIT` must be extended to also
`rm -rf "$SANDBOX"` — see `hermeticity-sandbox.md` (do not author two conflicting traps).

## Key test scenarios (hints for the tester — full plan is test-plan/)

- Stub `client-bundle` rc≠0 → Gate 5 fails with the `client-bundle emit failed (rc=…)` tail; no marker.
- Stub `client-bundle` emitting empty / `not-a-bundle` stdout → `produced no/invalid bundle blob`.
- Stub `node` absent → `node not available …`, exit 1 (NOT 3).
- Stub `init --bundle` rc≠0 → `init --bundle failed (rc=…)`.
- Force observe non-204 → message names the HTTP code; distinct from emit failure.
- Force store delta 0 → `bundle-path observe did not land in per-slug store`.
- Happy path is the ONLY combination yielding exit 0 + single terminal marker.
- Diff-assert `release-gate-lib.sh::run_smoke_gate` byte-unchanged.
- Gate 4 forced-fail → script fails at Gate 4 before any Gate 5 code runs (append-only proof).
- stdout-only capture: token-redacted stderr echo never appears in BUNDLE or CI log.
- Blob with trailing newline / shell-significant chars survives the quoted handoff.
