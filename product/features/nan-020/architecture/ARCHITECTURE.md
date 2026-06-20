# nan-020 Architecture — Product Documentation Currency (Doc-Test Enforcement)

## System Overview

nan-020 adds a *documentation-currency* mechanism to Unimatrix's release process. It does
not add application code; it extends two existing process surfaces and rewrites two doc
files:

1. **Detection** — the nan-019 release smoke (`docker-http-posture-smoke.sh`) is extended
   **in place** (D-2) to additionally exercise the canonical operator attach path the docs
   tell users to run: emit a connection bundle from the booted container, consume it with
   `init --bundle`, and prove a `POST /v1/{slug}/observe` round-trip lands in the per-slug
   store. When that documented path stops working, the release gate fails.
2. **Authorship** — the `uni-docs` agent's remit is widened (one `.claude/` edit, C-5) from
   "README.md only" to "all of `docs/`, blast-radius-scoped".
3. **Content** — `docs/client-setup.md` and the README bundle example are rewritten to the
   current bundle/observe model (delivery-phase work; not this architecture's concern beyond
   the executable-claim contract below).

The doc-test is N5's regression guard extended from "deployable-as-released" to
"usable-as-documented" (Goal 4 / AC-08). No new NFR, no new CI job, no new script.

### How this fits the larger system

```
release.yml (smoke-amd64 / smoke-arm64 jobs)
      │  sources
      ▼
release-gate-lib.sh :: run_smoke_gate IMAGE docker-http-posture-smoke.sh
      │  invokes once, IMAGE exported, captures rc, discriminates exit code,
      │  asserts anchored terminal run-marker            (UNCHANGED wrapper)
      ▼
docker-http-posture-smoke.sh   ← EXTENDED IN PLACE (this feature)
      ├─ Gates 1–4 (nan-019): boot HTTP-on, register slug, per-slug observe 204,
      │                        store-grew/hash-unchanged          (MUST still pass)
      └─ Gates 5–7 (nan-020): emit bundle in-container, init --bundle on host,
                              hook-client observe round-trip lands in per-slug store
```

The two runtimes the documented attach path uses live in **two different places**, exactly
as a real operator experiences them:

| Step | Runtime | Where it runs | Why |
|------|---------|---------------|-----|
| `unimatrix client-bundle <slug>` | Rust binary | **inside a throwaway container** off the shipped image | The bundle-emit command is a server-side operation; the Rust binary is the only executable in the distroless image. |
| `init --bundle <blob>` | JS (npm pkg) | **on the CI host** (`packages/unimatrix`, Node present) | An operator runs `init` on their *client* machine, not the server. The distroless image ships no `node` and no JS — putting `init` "in the container" would test an environment no operator has (SR-03). |

This host/container split is the load-bearing architectural decision (ADR-002). It is not a
compromise — it is a faithful reproduction of the documented topology.

## Component Breakdown

| Component | Responsibility | Change |
|-----------|----------------|--------|
| `docker-http-posture-smoke.sh` | Boot shipped image, register slug, prove per-slug observe routing **and now** prove the documented bundle attach path | EXTENDED in place (Gates 5–7) |
| `release-gate-lib.sh::run_smoke_gate` | Exit-code discrimination + anchored run-marker assertion | **UNCHANGED** — new failure modes fold into existing exit codes (see truth table) |
| `crates/unimatrix-server` `ClientBundle { slug }` | Emit `unimatrix-bundle:` blob from data volume | Consumed as-is (verified present in image) |
| `packages/unimatrix` `init --bundle` + hook-client | Consume bundle, pin cert, POST observe | Consumed as-is from the repo checkout on the host |
| `docs/client-setup.md`, `README.md` (bundle example) | Operator-facing attach docs | Rewritten (delivery work) |
| `.claude/agents/uni/uni-docs.md` | Doc authorship remit | Widened (ADR-004) |

### Verified facts (design-time confirmation — SR-01/SR-03/A1/A3)

- **`client-bundle` exists and is pinned**: `crates/unimatrix-server/src/main.rs:293` —
  `Command::ClientBundle { slug }`, dispatched at `main.rs:437` **pre-tokio (sync)**.
  Invocation: `unimatrix --project-dir /data client-bundle <slug>`. stdout = opaque
  `unimatrix-bundle:` blob (pipeable); stderr = token-redacted URL/fingerprint echo. The
  binary is the image ENTRYPOINT (`Dockerfile:165`) — **present in the shipped image**. A1
  CONFIRMED.
- **No JS in the image**: runtime stage is `gcr.io/distroless/cc-debian12:nonroot`
  (`Dockerfile:110`); it copies only the Rust binary + `libonnxruntime.so` + `/data`,
  `/shared`. No `node`, no `packages/unimatrix`. A3 CORRECTED: the image ships the Rust
  binary but **not** the JS init client. The JS half must run on the host.
- **Node is provisioned on the smoke host (PINNED, not incidental)**: `packages/unimatrix/
  lib/init.js` and `hook-client/` are in the repo checkout the smoke runs from; the host is the
  operator surrogate. The nan-019 smoke jobs (`release.yml:406–446`) currently carry NO
  `setup-node` step — they `checkout` + GHCR-login + `run_smoke_gate` and rely on incidental
  runner-image `node`. Because the doc-test makes node-absence a HARD-fail (ADR-001), that
  incidental reliance would let an unrelated runner-image change silently arm a release-blocker.
  nan-020 therefore REQUIRES an explicit pinned `actions/setup-node@v4` (`node-version: '24'`,
  matching the `package-npm` job at `release.yml:215–218`) added to both smoke jobs — the host
  JS leg's hard-fail must be intentional, not latent (ADR-002; the #793 "pin your infra"
  discipline). See Integration Surface.
- **Bundle decode is real attach**: `init --bundle` decodes the blob, pins the leaf cert by
  the carried `sha256:` fingerprint, writes the out-of-tree credential, wires the hook
  client, and validates with a pinned `Ping` (`init.js:362–518`). The fingerprint-pinned
  HTTPS connection from the host to the container's published port (`https://localhost:PORT`)
  is the same posture an operator gets.
- **`--slug` is RETIRED on the bundle path**: `init.js:353` — the bundle URLs already encode
  the slug; the client appends nothing. See Open Question OQ-A: SCOPE/AC-02 phrases the
  bundle mode as `--bundle <blob>` (+ `--slug`), but the code takes `--bundle` alone. The
  doc-test and the rewritten docs MUST follow the code (`--bundle <blob>`, no `--slug`).

## Component Interactions / Data Flow (the extension)

```
[host] docker run shipped image  ──►  container boots HTTP-on (Gate 1, nan-019)
[host] project register <slug> + restart                    (nan-019)
[host] cert-pinned curl POST /v1/<slug>/observe → 204, store grew (Gates 2–4, nan-019)
                          │
                          ▼  (nan-020 extension begins — original path proven first)
[host] docker run --rm <image> --project-dir /data client-bundle <slug>
                          │  Rust binary, in throwaway container, reads same volume
                          ▼  stdout → BUNDLE blob captured on host  (Gate 5)
[host] HOME="$SANDBOX/home" node packages/unimatrix/bin/unimatrix.js \
            init --bundle "$BUNDLE" --project-dir "$SANDBOX/proj"
                          │  JS, on host (pinned node, setup-node@v4); isolated HOME → fresh
                          │  $SANDBOX/home/.unimatrix/<hash>/remote.json; decodes bundle, pins
                          │  cert by fingerprint, wires hook client,
                          ▼  validates with pinned Ping over HTTPS to localhost:PORT  (Gate 6)
[host] fire one hook event through the wired hook client (same isolated HOME)
                          │  JS hook-client → POST <observe_url from bundle> (reads THIS run's
                          │  credstore, never the runner's real ~/.unimatrix)
                          ▼  204; per-slug store grows by the NEW write (re-use store_size)  (Gate 7)
```

The bundle's `observe_url` already encodes `/v1/<slug>/observe` (server-composed), so Gate 7
proves the *documented* path end-to-end — not a hand-composed URL.

## Hermeticity Is a Proof Obligation (ADR-005 — the gate must measure the FRESH attach)

Gates 6–7 write a REAL out-of-tree credential to the HOME-keyed credstore
(`~/.unimatrix/<hash>/remote.json`, vnc-039) and a `--project-dir` tree. The decisive hazard
(R-07): a PRIOR run's leftover credential can satisfy Gate 7's observe round-trip WITHOUT a
fresh attach — so a broken `init --bundle` would still green. A doc-test that false-greens off
residue reproduces the EXACT blind spot (#768) nan-020 exists to close — self-defeating.
Hermeticity is therefore architected as a PROOF OBLIGATION, not best-effort cleanup:

- **Process-boundary HOME isolation + throwaway project-dir.** The JS consume leg runs inside a
  per-run `mktemp -d` sandbox; `HOME` and `--project-dir` are set in the **spawned child's
  environment** (`HOME="$SANDBOX/home" node … init --bundle … --project-dir "$SANDBOX/proj"`),
  so the credstore resolves to a path that cannot pre-exist. The SAME isolated env is reused for
  the Gate 7 hook fire so the hook client reads THIS run's credstore, not the runner's real one.
- **Why the process boundary (not in-process):** Rust-2024 forbids `std::env::set_var("HOME",…)`
  (`unsafe`/forbidden) — vnc-041's AC-02 had to DEFER an in-process round-trip for exactly this
  reason. Setting HOME on the spawned child via the shell sidesteps that constraint by
  construction; the harness never mutates its own HOME in process.
- **Clean-on-entry, not just on exit** — a crashed prior run must not poison the next.
- **Proven by a NEGATIVE CONTROL (the vnc-041 AC-06 / #5246 shape — load-bearing).** A
  pre-merge test pre-seeds a STALE credential where a non-isolated run would read it AND points
  Gate 6 at a deliberately BROKEN attach; Gate 7 MUST STILL FAIL. A non-isolated harness would
  PASS that scenario (residue satisfies observe) — that pass is the vacuous false-green the
  control catches. The happy path (fresh attach into the sandbox) is the ONLY green. This proves
  the gate measures the fresh attach, not residue (#4977 / #5189).

## The Extended Exit-Code Truth Table (SR-04 — DO NOT REGRESS)

`run_smoke_gate` is **not modified**. Every new failure mode maps onto the existing
contract so the wrapper's discrimination still holds. The original table (nan-019) and the
nan-020 additions:

| Exit | Meaning | Source | Status |
|------|---------|--------|--------|
| 0 | ran + ALL gates passed (incl. new bundle gates); terminal marker printed | end of script | nan-019, semantics widened |
| 1 | ran + a gate FAILED — **incl. every new bundle-attach failure** | `fail()` | nan-019, reused for Gates 5–7 |
| 3 | self-skipped: Docker absent | preflight `exit 3` | nan-019 UNCHANGED (run_smoke_gate turns it into HARD failure) |
| 4 | IMAGE= prebuilt tag could neither be pulled nor found locally | acquisition arm | nan-019 UNCHANGED |

**Key contract decision (ADR-001):** the new bundle gates fail through the existing
`fail()` (exit 1) with *distinct, diagnosable messages* — they do **not** introduce new
numeric exit codes. SR-08 asks every skip path to be a "distinct hard-fail exit code";
the architecture satisfies the *intent* (no silent green, attributable cause) via distinct
**fail messages** under the single fail-exit 1, because:

- `run_smoke_gate` already maps 1 → `::error::smoke FAILED (exit 1)` and 3 → HARD-fail and
  4 → HARD-fail. Adding bespoke exit codes (5, 6, 7) would force a `run_smoke_gate` edit,
  widening the blast radius into the load-bearing wrapper (SR-04) for zero gate-behavior
  gain — every one of those would also just `return 1`. That is the gold-plating C-3 forbids.
- The C-2/SR-08 requirement is **"never silent-green; cause must be attributable."** A
  `fail()` (exit 1) with a message that names the failing step (e.g. `"client-bundle emit
  produced empty blob"`) is attributable and hard-fails. The distinctness lives in the
  message, the single load-bearing numeric distinction (skip-vs-fail-vs-acquire = 3/1/4)
  is preserved exactly.

### New-failure-mode → exit mapping (SR-08 / C-2)

Each new skip/abort path is a `fail()` (exit 1) with a UNIQUE message prefix. None may
early-`exit 0` or silently continue:

| New failure mode | Detection | Outcome | Message (distinct, attributable) |
|------------------|-----------|---------|----------------------------------|
| `client-bundle` binary/subcommand absent or errors | non-zero rc from the `docker run ... client-bundle` invocation | `fail()` exit 1 | `"client-bundle emit failed (rc=N) — subcommand renamed/absent in shipped image?"` (names the command — SR-02) |
| `client-bundle` produced empty / non-`unimatrix-bundle:` stdout | grep blob prefix | `fail()` exit 1 | `"client-bundle produced no/invalid bundle blob"` |
| `node` absent on the host | `command -v node` preflight (added next to the Docker preflight) | `fail()` exit 1 | `"node not available — the documented init --bundle path cannot be exercised"` (NOT exit 3: node-absence is a mis-provisioned lane, same class as Docker-absent → hard-fail). With `setup-node@v4` pinned (ADR-002), this is now a SAFETY NET for a provisioning regression, not the acquisition path. |
| `init --bundle` failed (decode/pin/Ping) | non-zero rc from `node ... init --bundle` | `fail()` exit 1 | `"init --bundle failed (rc=N) — bundle attach broken"` |
| observe route absent / non-204 via hook client | http code check | `fail()` exit 1 | `"documented bundle attach observe returned HTTP C (expected 204)"` (distinguishes doc-drift from route change — SR-09) |
| per-slug store did not grow after bundle observe | `store_size` delta | `fail()` exit 1 | `"bundle-path observe did not land in per-slug store"` |

### Regression assertion (SR-04 — the original path still passes)

The extension is **append-only**: Gates 1–4 (boot HTTP-on, register, per-slug observe 204,
store-grew/hash-unchanged) run **unchanged and first**; Gates 5–7 run only after Gate 4
passes. The existing per-slug-observe assertion is therefore the literal precondition of the
new gates — if it regresses, the script fails at Gate 4 exactly as today, before any new
code executes. No nan-019 gate, message, the `IMAGE=` acquisition arm, the exit-3 preflight,
or the `[783-smoke] ALL GATES PASSED` terminal marker is altered. The marker stays the
single terminal run-marker (AC-06); the new gates print between Gate 4 and the existing
final marker line.

> Caveat honored (D-2 / A2): the bundle round-trip reuses the **same** booted container,
> volume, slug, port, token, and cert as Gates 1–4 — no second image build, no divergent
> boot config. The "split to a sibling only if boot config genuinely diverges" condition is
> NOT met; extend-in-place is correct.

## The Executable-Claim vs Narrative-Prose Boundary (SR-06 — operational contract)

This is the contract that keeps the doc-test minimal (C-3) and tells `uni-docs` what to
stamp vs. what is machine-guarded. It is an **operational definition**, not prose intent:

**A doc line is an *executable claim* (MUST be doc-tested) iff it instructs the operator to
RUN a specific command whose success is the claim.** Concretely, a line is an executable
claim when ALL hold:
1. It contains a runnable command an operator copy-pastes (`unimatrix ...`, `npx ... init
   ...`, a `curl` invocation that is itself the instruction).
2. Its correctness is *behavioral* — it either works against the shipped artifact or it does
   not (not a matter of phrasing).
3. It lies on the canonical attach path the doc-test already exercises, OR is reducible to it.

**Everything else is *narrative prose*** (manual rewrite + a single `verified on vX` footer
stamp per file, D-3, NOT machine-checked): explanations of *what* a mode is, *why* pinning
works, trust-model background, when-to-use guidance, port/security notes.

**The doc-test's tested set is exactly one canonical claim chain** (AC-03), not every command
in the docs:

> *"An operator emits a bundle with `unimatrix client-bundle <slug>` and attaches with
> `init --bundle <blob>`, producing a successful `POST /v1/{slug}/observe` round-trip."*

Worked example against `docs/client-setup.md` (post-rewrite target lines):

| Doc line | Classification | Guarded by |
|----------|---------------|-----------|
| `unimatrix client-bundle <slug>` (emit) | **Executable claim** | doc-test Gate 5 |
| `npx @dug-21/unimatrix init --bundle <blob>` (attach) | **Executable claim** | doc-test Gate 6–7 |
| the hook client POSTing to `/v1/<slug>/observe` | **Executable claim** | doc-test Gate 7 |
| "Cloud MCP requires a v:2 bundle / fingerprint pinning explained" | Narrative prose | manual + `verified on vX` stamp |
| "TLS-only port 8443, GET /health unauthenticated" | Narrative prose | manual + stamp |
| Token-rotation steps (operator runbook, no shipped-behavior round-trip) | Narrative prose | manual + stamp |

Boundary discipline (prevents both failure modes SR-06 names):
- **Over-broad → gold-plating:** Do NOT add a doc-test gate per command. The tested set is
  the single canonical chain above; additional commands are covered only if they fall on it.
- **Under-broad → drift persists:** Any *new* command added to the attach docs that is NOT
  reducible to the canonical chain is a signal the canonical chain itself is incomplete —
  raise it to design, do not leave it untested by default.

## The uni-docs Remit Widen (ADR-004 — data/ownership, not code)

The change is **authorship-remit text only** (SR-05 fence). It widens *who owns which files*
and *how much they audit* — it adds NO drift-checker, NO gate, NO Phase-4 trigger redesign
(all Feature 2). Specific edits to `.claude/agents/uni/uni-docs.md`:

- Scope line: "README.md only" → "README.md and all of `docs/`".
- Behavioral rule: authorship is **blast-radius-scoped** — uni-docs updates the doc surfaces
  a delivered change *touches* (the files whose executable claims or narrative the change
  affects), NOT a full-tree `docs/` audit every cycle (C-4 / SR-07).
- The "no source code reading" rule is RELAXED narrowly: to write/verify an executable claim
  for `docs/`, uni-docs may read the CLI surface it documents — bounded to the touched
  surface, still not a general code-audit license.
- State the full-tree-audit non-goal explicitly in the definition (AC-07).
- Detection stays the doc-test's job, authorship stays uni-docs's job — do not conflate.

"Blast radius" operational definition (SR-07): the set of doc files containing claims —
executable or narrative — about the behavior a feature changed. Determined from the feature's
SCOPE/SPEC + the diff's touched surfaces, not by scanning all of `docs/`.

## Integration Surface

| Integration Point | Type / Signature | Source (verified) |
|-------------------|------------------|-------------------|
| Bundle emit | `unimatrix --project-dir /data client-bundle <slug>` → stdout `unimatrix-bundle:<blob>` (sync, pre-tokio) | `crates/unimatrix-server/src/main.rs:293,437`; README:587 |
| Bundle blob prefix | line begins `unimatrix-bundle:` (`v:2`) | README:587; `init.js` decodeBundle |
| Bundle consume | `init --bundle <blob>` (NO `--slug` — retired on bundle path) | `packages/unimatrix/bin/unimatrix.js:23`; `lib/init.js:353,363` |
| init host entry | `HOME="$SANDBOX/home" node packages/unimatrix/bin/unimatrix.js init --bundle <blob> --project-dir "$SANDBOX/proj"` (HOME + project-dir isolated to a per-run `mktemp -d` sandbox, set on the child) | `bin/unimatrix.js:10–33`; ADR-005 |
| Host node provisioning | `release.yml` smoke jobs add a pinned `actions/setup-node@v4` (`node-version: '24'`) step after `checkout`, before `run_smoke_gate` | NEW (this feature); model: `release.yml:215–218` (`package-npm`) |
| Host credstore (isolated) | `$SANDBOX/home/.unimatrix/<projectHash>/remote.json` (HOME-keyed; cannot pre-exist this run) | ADR-005; vnc-039 `#5125` |
| init validation | pinned `Ping` over fingerprint-pinned HTTPS (throws → exit 1) | `lib/init.js:421–518` |
| Observe route | `POST https://localhost:PORT/v1/<slug>/observe` → `204` | smoke script:153–164; vnc-038 |
| Per-slug store | `/data/.unimatrix/<slug>/unimatrix.db` (+ -wal/-shm); size delta via busybox `du -s` | smoke script:54,139,167 |
| Image runtime | distroless `gcr.io/distroless/cc-debian12:nonroot`; ENTRYPOINT `unimatrix`; **no node/JS** | `Dockerfile:110,165` |
| Gate wrapper | `run_smoke_gate IMAGE docker-http-posture-smoke.sh` (UNCHANGED) | `release-gate-lib.sh:42` |
| Terminal run-marker | `[783-smoke] ALL GATES PASSED` (single, terminal) | smoke script:185; gate-lib:57 |
| Exit contract | `0` pass · `1` fail (`fail()`, incl. all new gates) · `3` Docker absent · `4` IMAGE acquire | gate-lib:11–15 |

## Key Design Decisions (ADR index)

- **ADR-001** — Extend in place; new bundle-attach failures fold into the existing `fail()`
  exit-1 with distinct messages; `run_smoke_gate` untouched; original Gates 1–4 are the
  precondition of the new gates (regression guard).
- **ADR-002** — In-test bundle emission with a **host/container runtime split**: emit
  (Rust) in-container, consume (`init --bundle`, JS) on the host, because the distroless
  image ships no JS and the host is the operator surrogate. Amended: `node` is EXPLICITLY
  provisioned on the smoke runner via a pinned `setup-node@v4` step in `release.yml` so the
  host JS leg's hard-fail is intentional, not latent (the #793 pin-your-infra discipline).
- **ADR-003** — The executable-claim vs narrative-prose boundary as an operational contract;
  the tested set is exactly the one canonical attach chain (AC-03).
- **ADR-004** — Widen uni-docs authorship remit to all of `docs/`, blast-radius-scoped;
  authorship only, no drift-checker (fences Feature 2).
- **ADR-005** — Host-side consume hermeticity is a PROOF OBLIGATION: per-run `mktemp -d`
  sandbox with HOME + `--project-dir` isolated **on the spawned child** (process boundary, so
  no Rust-2024 in-process HOME-mutation — vnc-041 AC-02), proven by a NEGATIVE CONTROL
  (poison a stale cred + break the attach; Gate 7 must still fail — vnc-041 AC-06 / #5246). No
  prior `~/.unimatrix/<hash>/` state can false-green the gate.

## Open Questions

- **OQ-A (for spec writer / uni-docs — load-bearing):** SCOPE AC-02 phrases bundle attach as
  `--bundle <blob>` (+ `--slug`), but `init.js:353` RETIRES `--slug` on the bundle path (the
  bundle URLs encode the slug). The code is authoritative: docs and the doc-test MUST use
  `--bundle <blob>` with **no** `--slug`. Confirm AC-02's "(+ `--slug`)" is dropped for the
  bundle mode in the rewritten docs. (`--slug` remains only as a server-side `project
  register`/`client-bundle` argument, not an `init` bundle argument.)
- **OQ-B (for spec writer):** README has TWO bundle phrasings — line 123 `init --remote
  unimatrix-bundle:<blob>` (the broken example, AC-02 target) and line 587/130 `init --remote
  <bundle>`. Both predate `--bundle`. The rewrite must converge all of them on the canonical
  `init --bundle <blob>`; enumerate every occurrence so AC-02's "corrected example" is
  exhaustive, not just line 123.
- **OQ-C — RESOLVED by ADR-005 (no longer open).** Hermeticity is now architected as a proof
  obligation, not deferred to the test plan: per-run `mktemp -d` sandbox with HOME +
  `--project-dir` isolated on the spawned child (process boundary — sidesteps the Rust-2024
  in-process HOME-mutation ban, vnc-041 AC-02), clean-on-entry, and a REQUIRED pre-merge
  negative control (stale cred + broken attach must still fail Gate 7 — vnc-041 AC-06 /
  #5246). The tester implements the negative control; WHETHER the gate measures the fresh
  attach is decided here (ADR-005), not left to the test plan.
