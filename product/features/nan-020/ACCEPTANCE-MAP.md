# nan-020 Acceptance Criteria Map

> Regenerated 2026-06-20 (revision pass). Covers **AC-01..AC-09** — AC-01..AC-08 from SCOPE.md
> plus AC-09 (hermetic CI negative control), the spec-added proof obligation. Each AC maps to a
> verification method, the owning component, and its risk coverage. AC-09 is the new hard
> hermeticity proof obligation; the legacy `--remote` mode is documented-but-NOT-doc-tested by
> conscious design (AG-1 / C-7 / R-16) — its only owed coverage is the inspectable "legacy" marker
> under AC-02.

| AC-ID | Description | Verification Method | Verification Detail | Owning Component | Risk Coverage | Status |
|-------|-------------|---------------------|---------------------|------------------|---------------|--------|
| AC-01 | `docs/client-setup.md` has zero `501`, zero `W2-7`, zero hand-rolled `curl ... /observe` hook scripts; documents the `init`-wired hook client + `/v1/{slug}/observe`. | grep | `grep -c -E '501\|W2-7' docs/client-setup.md` → 0; no fenced block matches `curl .*/observe`; positive grep finds `init --bundle` and `/v1/{slug}/observe`; FR-3 obsolete-model sweep ("no local binary / curl shell hooks" prose gone). | `docs/client-setup.md` rewrite (uni-docs) | R-12, R-08 | PENDING |
| AC-02 | Both attach modes documented correctly in README + client-setup; legacy `--remote <url> --token <tok>` explicitly MARKED legacy; broken `init --remote unimatrix-bundle:<blob>` corrected to canonical `init --bundle <blob>`; no `--slug` paired with `--bundle`. | grep + manual | In BOTH files: `--remote <url> --token <tok>` AND `init --bundle <blob>` present; `--remote` annotated "legacy"; zero `init --remote unimatrix-bundle:` AND zero `init --remote <bundle>` bundle-fed forms anywhere (regex, multi-occurrence — not line-pinned, OQ-B); zero `--slug` paired with `--bundle` (OQ-A). | `docs/client-setup.md` rewrite + `README.md` bundle-example fix (uni-docs) | R-09, R-10, R-16 (legacy marker) | PENDING |
| AC-03 | Doc-test exercises canonical `--bundle` attach → successful `POST /v1/{slug}/observe` (204, lands in per-slug store) against a freshly built/booted image. | test | Run the extended `docker-http-posture-smoke.sh` in a Docker-capable lane: emits a bundle in-test via `client-bundle <slug>` (Gate 5), attaches via host hermetic `init --bundle` (Gate 6), fires a hook event → POST `/v1/<slug>/observe` (Gate 7), asserts HTTP 204 AND per-slug store grew while hash store unchanged; gate returns 0 with the terminal run-marker. **Two-leg done_when → delivers C15 (see explicit mapping below).** | `docker-http-posture-smoke.sh` (Gates 5–7) | R-01, R-02, R-04, R-05, R-06 | PENDING (pre-merge gate-logic provable now; live round-trip POST-TAG-CONFIRMABLE) |
| AC-04 | Doc-test is a SIBLING of nan-019 smoke (lives under `product/test/infra-001/scripts/`, reuses throwaway-container pattern, same release gate path); NOT a new bespoke CI job. | file-check + manual | No new script file added under `product/test/infra-001/scripts/` (D-2 extend-in-place; file-count delta 0) and no new CI job; the round-trip runs inside `docker-http-posture-smoke.sh`, invoked via `run_smoke_gate`. | `docker-http-posture-smoke.sh` (extension) | R-15, R-03 | PENDING |
| AC-05 | Docker unavailable ⇒ gate HARD-FAILs (no silent skip / false-green); each new skip path likewise. | test | No-Docker run: doc-test `exit 3`s with a SKIP reason; `run_smoke_gate` converts to `::error::smoke SKIPPED (exit 3) ... HARD failure` returning non-zero. Repeat per new skip path (FR-15): `node` absent, `client-bundle` absent/empty blob, observe non-204, store-no-grow each yield a distinct `fail()` exit 1 + a failing gate. | `docker-http-posture-smoke.sh` (preflights + `fail()` paths) | R-01, R-04, R-07 | PENDING |
| AC-06 | Doc-test asserts an anchored terminal run-marker so an early `exit 0` cannot pass. | test | Inject/simulate an early `exit 0` before all gates complete; assert `run_smoke_gate`'s `grep -qx '\[783-smoke\] ALL GATES PASSED.*'` fails the gate. Marker appears exactly once, last line; new gates print before it. | `docker-http-posture-smoke.sh` (marker) + `release-gate-lib.sh` (unchanged) | R-01, R-03 | PENDING |
| AC-07 | `.claude/agents/uni/uni-docs.md` remit widened README-only → all of `docs/`, blast-radius-scoped, with the full-tree-audit non-goal stated. | manual + grep | Inspection: scope/constraint/self-check admit `docs/`; "blast radius" defined as surfaces a change touches; explicit "does NOT audit all of docs/ every cycle"; source-read relaxation narrow (touched CLI surface only); prompt-injection defense + "document only what is shipped" retained; NO drift-checker/gate/Phase-4 trigger text added. | `.claude/agents/uni/uni-docs.md` remit widen | R-13 | PENDING |
| AC-08 | N5 described as extended to "usable-as-documented" (bundle chain only) with the doc-test named as its docs-layer regression guard; no new NFR minted; N5 status unchanged. | manual | Inspection of the artifact(s) referencing N5: framing reads "deployable-as-released → usable-as-documented", doc-test named as guard, claim bound to the `--bundle` chain (AG-1), N5 status unchanged, no new NFR/capability id introduced. (Capability-map update performed by human after scope lock.) | N5 framing (docs/artifacts; human-owned) | R-14 (human-owned), R-16 | PENDING |
| AC-09 | The doc-test is HERMETIC: Gates 6–7 run the host `init --bundle` attach + hook fire under an isolated `HOME`/credstore and a throwaway per-run `--project-dir`, so a prior run's `~/.unimatrix/<hash>/` credential/store cannot satisfy the gate. PROOF OBLIGATION, not a footnote. | test (REQUIRED pre-merge negative control) | **NEGATIVE CONTROL (#4977 / vnc-041 AC-06 / #5246):** pre-seed/poison a stale valid-looking credential where a non-isolated run would read it AND point Gate 6 at a deliberately BROKEN attach; assert Gate 7 STILL FAILS. Plus: assert HOME/credstore + throwaway `--project-dir` cleaned on ENTRY (not just exit); assert non-skip / fresh-write evidence (per-slug store grew by the NEW write, delta>0). Isolation MUST be at the process/shell boundary (`HOME="$SANDBOX/home" node … --project-dir "$SANDBOX/proj"`) — NOT in-process (Rust-2024 forbids it; vnc-041 AC-02 deferred for this). PRE-MERGE-PROVABLE against a stub broken-attach — PENDING IS a gap (#5189). | Hermeticity sandbox + negative control (in `docker-http-posture-smoke.sh` Gates 6–7) | **R-07 (CRITICAL)** | PENDING (negative control is REQUIRED pre-merge, not deferrable) |

## Explicit AC-03 → C15 Two-Leg done_when Mapping (design-review clarity finding, human-approved)

C15 (runbook capability — "operators can attach a client by following the docs and have it work")
has a **two-leg** done_when. AC-03 evidences both legs; each leg is named to which test leg
evidences it, so C15's eventual `proven` claim is self-evidencing:

| C15 leg | What it asserts | Evidenced by (named test leg) |
|---------|-----------------|-------------------------------|
| **MCP-round-trip leg** | The bundle attach actually established a working, fingerprint-pinned client session against the live server (not just that a POST landed). | **Gate 6 — the pinned `Ping`** over fingerprint-pinned HTTPS to `https://localhost:PORT` inside `init --bundle` (`init.js:421–518`). A throw → exit 1; success is the MCP-round-trip evidence. |
| **observe-landing leg** | The wired hook client's telemetry reaches the documented route and persists. | **Gate 7 — the 204 + per-slug-store write**: hook-fire → `POST /v1/<slug>/observe` → HTTP 204 AND the per-slug store (`/data/.unimatrix/<slug>/unimatrix.db`) grew by the NEW write (delta>0, hermetic per AC-09). |

Both legs run hermetically (AC-09) so neither leg can be satisfied by residual state. C15 stays
**`partial`** until the post-tag live run confirms the round-trip on a real hosted runner — the
pre-merge gate-logic (stub truth tables + the AC-09 negative control) proves the gate is correct and
non-vacuous now; the live container round-trip is POST-TAG-CONFIRMABLE (#5189, accepted PENDING, not
asserted as executed before it runs). When the post-tag live run greens both legs above, C15
advances to `proven` on the attached behavioral evidence.

## Accepted Gap (named, not a coverage hole)

- **AG-1 / R-16 — legacy `--remote` documented-but-NOT-doc-tested (CONSCIOUSLY ACCEPTED):** the
  doc-test (AC-03) exercises ONLY the canonical `--bundle` chain. The legacy
  `init --remote <url> --token <tok>` mode is documented and MARKED legacy (AC-02) but carries NO
  round-trip coverage by deliberate scope decision (bundle is the only canonical mode; `--remote` is
  legacy/unused). N5's "usable-as-documented" (AC-08) is therefore bound to the `--bundle` chain
  ONLY and MUST NOT be read as covering both modes. The sole owed mitigation is the inspectable
  "legacy" marker (verified under AC-02). If `--remote` ever returns to active use, doc-testing it
  must be re-opened.

## Coverage Roll-Up

- **Critical risks:** R-01 (AC-03/05/06), R-03 (AC-04/06), **R-07 (AC-09 — REQUIRED negative control)**.
- **High risks:** R-02 (AC-03), R-04 (AC-03/05), R-05 (AC-03), R-08 (AC-01), R-09 (AC-02), R-10 (AC-02), R-11 (AC-03 pre-merge gate logic).
- **Medium/Low:** R-06 (AC-03), R-12 (AC-01), R-13 (AC-07), R-14 (AC-08, human-owned), R-15 (AC-04).
- **Accepted residual:** R-16 (AC-02 legacy marker; AC-08 scope statement) — no round-trip coverage owed.

Every AC-01..AC-09 is PENDING until delivery; pre-merge-provable items (AC-03 gate logic, AC-05,
AC-06, **AC-09 negative control**) must be green pre-merge — classifying any of them PENDING-without-
proof at merge is a gap (#5189). The live container round-trip portion of AC-03/AC-09 is
POST-TAG-CONFIRMABLE and labeled accordingly, never asserted as executed before it runs.
