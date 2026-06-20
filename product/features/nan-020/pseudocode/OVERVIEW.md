# nan-020 Pseudocode Overview — Doc-Test Enforcement for Executable Claims

This feature adds NO application code. It extends one bash smoke script, adds one CI
provisioning step, rewrites two doc files, and edits one agent definition. The pseudocode
altitude varies per component (algorithm for the script; content-structure for prose; an
edit-plan for the agent def).

## Components & Why They Are Involved

| Component file | Touches | Why |
|----------------|---------|-----|
| `docker-http-posture-smoke.md` | `product/test/infra-001/scripts/docker-http-posture-smoke.sh` | Append Gates 5–7 (in-container bundle emit; host hermetic `init --bundle` consume; hook-fire observe round-trip + per-slug store-growth) after Gate 4. |
| `hermeticity-sandbox.md` | **same file** + a pre-merge stub gate-logic test | The process-boundary HOME/`--project-dir` sandbox lifecycle that Gates 6–7 run inside, AND the REQUIRED negative control. |
| `release-yml-setup-node.md` | `.github/workflows/release.yml` (smoke-amd64 + smoke-arm64) | Pin `actions/setup-node@v4` (node 24) on both smoke jobs so the host JS leg's hard-fail is intentional, not latent. |
| `docs-client-setup.md` | `docs/client-setup.md` | Rewrite to the current bundle/observe model; remove 501/W2-7/curl-observe; `--remote` marked legacy; verified-on footer. |
| `readme-bundle-example.md` | `README.md` | Converge every `init --remote …<bundle>` form onto `init --bundle <blob>`; mark `--remote <url> --token` legacy. |
| `uni-docs-remit.md` | `.claude/agents/uni/uni-docs.md` | Widen authorship remit README-only → all of `docs/`, blast-radius-scoped. |

## SAME-FILE COUPLING — Stage 3b MUST honor this

**`docker-http-posture-smoke.md` and `hermeticity-sandbox.md` describe edits to the SAME
file (`docker-http-posture-smoke.sh`). They are split here for readability ONLY. They are
INSEPARABLE: the sandbox lifecycle (`hermeticity-sandbox.md`) is the environment Gates 6–7
(`docker-http-posture-smoke.md`) run inside.**

> **Routing rule for Stage 3b:** assign BOTH `docker-http-posture-smoke.md` AND
> `hermeticity-sandbox.md` to ONE implementation agent editing
> `docker-http-posture-smoke.sh`. Do NOT route them to two agents — concurrent edits to the
> same file will conflict, and the gate ordering (sandbox setup → Gate 6 → Gate 7 → cleanup)
> must be authored as one coherent append. The pre-merge stub gate-logic test described in
> `hermeticity-sandbox.md` is a separate NEW test file and MAY be a distinct task, but its
> author must read the final shape of the script edit.

All other components touch distinct files and may be parallelized.

## Ordering Constraints (what must be built / true first)

1. The script extension (`docker-http-posture-smoke.md` + `hermeticity-sandbox.md`) is the
   load-bearing artifact; the pre-merge stub test depends on its final shape.
2. `release.yml` setup-node is independent but is the provisioning half whose enforcement
   half lives in the script (node-absent `fail()`); both should land in the same PR so the
   hard-fail is not armed before node is provisioned.
3. Doc rewrites (`docs-client-setup.md`, `readme-bundle-example.md`) are independent of the
   script and of each other, but BOTH must use the identical canonical form `init --bundle
   <blob>` (no `--slug`) and the identical "legacy" marking for `--remote` — shared contract
   below.
4. `uni-docs-remit.md` is fully independent.

## Shared Contracts (single source of truth — every component references these)

### A. Exit-code truth table (numerics UNCHANGED — ADR-001)

| Exit | Meaning | Who emits |
|------|---------|-----------|
| `0` | ran + ALL gates passed (incl. Gates 5–7); terminal marker printed | end of script |
| `1` | ran + a gate FAILED — **incl. EVERY new Gate 5–7 failure** | `fail()` |
| `3` | self-skipped: Docker absent | preflight `exit 3` (UNCHANGED) |
| `4` | `IMAGE=` tag unpullable/unfound | acquisition arm (UNCHANGED) |

No new codes 5/6/7. `run_smoke_gate` in `release-gate-lib.sh` stays **byte-identical** (diff-asserted).

### B. New-failure-mode → message (distinct, attributable — all via `fail()` exit 1)

| Failure mode | EXACT message |
|--------------|---------------|
| `client-bundle` rc≠0 | `client-bundle emit failed (rc=N) — subcommand renamed/absent in shipped image?` |
| empty / non-`unimatrix-bundle:` blob | `client-bundle produced no/invalid bundle blob` |
| `node` absent on host | `node not available — the documented init --bundle path cannot be exercised` |
| `init --bundle` rc≠0 | `init --bundle failed (rc=N) — bundle attach broken` |
| observe non-204 | `documented bundle attach observe returned HTTP C (expected 204)` |
| per-slug store did not grow | `bundle-path observe did not land in per-slug store` |

(`fail()` prefixes each with `[783-smoke] FAIL: `. The strings above are the unique tails.)

### C. Bundle blob format

- Opaque single line beginning literal `unimatrix-bundle:` (`v:2`).
- Emitted on `client-bundle` **stdout only**; stderr carries token-redacted URL/fingerprint
  echo that MUST NOT be captured into the blob or logged.
- Encodes server-composed `observe_url` (`/v1/<slug>/observe`, slug baked in), bearer token,
  `sha256:` cert fingerprint. Consumed verbatim — client appends no path, no `--slug`.

### D. Invocation signatures (consumed as-is — NO behavior change)

- Emit (Rust, in-container): `docker run --rm -v "$VOL:/data" "$IMAGE" --project-dir /data client-bundle "$SLUG"` → stdout blob.
- Consume (JS, on host, hermetic): `HOME="$SANDBOX/home" node packages/unimatrix/bin/unimatrix.js init --bundle "$BUNDLE" --project-dir "$SANDBOX/proj"` (NO `--slug`).
- Hook fire (JS, host, SAME isolated HOME): one event through the wired hook client → `POST <observe_url from bundle>` → 204.
- Gate wrapper (UNCHANGED): `run_smoke_gate IMAGE bash docker-http-posture-smoke.sh`.

### E. Isolated credstore path (per-run)

- `$SANDBOX/home/.unimatrix/<projectHash>/remote.json` — HOME-keyed (vnc-039 #5125); cannot
  pre-exist this run because `SANDBOX="$(mktemp -d)"`.
- `SANDBOX` cleaned **on ENTRY** (guard) + on exit (trap), so a crashed prior run cannot poison the next.

### F. Anchored terminal run-marker (UNCHANGED, stays last line)

- Literal: `[783-smoke] ALL GATES PASSED` (printed via `log "ALL GATES PASSED — …"`).
- `run_smoke_gate` asserts `grep -qx '\[783-smoke\] ALL GATES PASSED.*'`.
- Gates 5–7 print BETWEEN Gate 4 and this single terminal line. NO second marker.

### G. Per-slug store growth signal

- `/data/.unimatrix/<slug>/unimatrix.db` (+ `-wal`/`-shm`); measured by reusing nan-019's
  `store_size()` (busybox `du -s` over the dir, WAL-robust). Gate 7 asserts a positive DELTA
  from a fresh BEFORE sample taken just before the hook fire.

### H. Canonical doc form (binding on BOTH doc components)

- Canonical attach: `npx @dug-21/unimatrix init --bundle <blob>` — **no `--slug`**.
- Legacy form (documented, MARKED legacy, NOT doc-tested): `init --remote <url> --token <tok>`.
- Zero occurrences of `init --remote unimatrix-bundle:` or `init --remote <bundle>` may survive.

## Open Questions / Gaps

- None blocking. All former OQs (A `--slug` retirement, B README enumeration, C hermeticity)
  are resolved by the brief/ADR-005 and the enumeration in `readme-bundle-example.md`.
- One enumeration note surfaced during design: README has **four** bundle-via-`--remote`
  occurrences to converge (lines 123, 130, 585, 587), not the two the brief's OQ-B named.
  Enumerated in `readme-bundle-example.md`. Line 113 (`--remote <url> --token`) is the legacy
  form — marked legacy, NOT converged.
