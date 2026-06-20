# nan-020 Test Plan — OVERVIEW

> Stage 3a. Doc-test / release-smoke extension. Rooted in `RISK-TEST-STRATEGY.md`
> (16 risks; R-01/R-03/R-07 Critical, R-07 the REQUIRED hermeticity negative control,
> R-16 accepted residual), `ARCHITECTURE.md` (ADR-001..005), `ACCEPTANCE-MAP.md`
> (AC-01..AC-09), and the priors #5180 / #5183 / #5189 / #5192 / #4977 / vnc-041 AC-06/AC-02.

## 1. Strategy In One Paragraph

The new behavior (Gates 5–7 + hermeticity) executes **only post-tag** in `release.yml`,
against a real container. Per #5189 the highest-value coverage is therefore **PRE-MERGE-PROVABLE,
stub-driven gate-logic tests** — not the live round-trip. We extend the *exact same* shell-gate
test convention nan-019 shipped: `release-gate-logic-test.sh` sources the shipped bytes and drives
them against `fixtures/stub-smoke.sh` (#5192 sourceable-spine; copies drift and test nothing — R-01).
nan-020 adds stub fixtures for the three new external commands (`docker run client-bundle`,
`node … init --bundle`, the observe POST / store-`du`) and drives the new Gate 5–7 logic through
them so every new `fail()`/exit-1 path, the negative control, and the regression invariants are
proven without Docker. The real container round-trip is **POST-TAG-CONFIRMABLE**, labeled PENDING,
never asserted as executed fact pre-merge (#4796).

**Cardinal rule (#5189):** for the Critical/High *gate-logic* risks (R-01, R-03, R-07, R-11)
PENDING-without-proof at merge **IS a gap**. Only the live hosted-runner container round-trip
(R-04/R-05/R-07 in a live container) may be PENDING-post-tag.

## 2. Test Levels

| Level | What | Where | When |
|-------|------|-------|------|
| **Shell gate-logic (stub-driven)** — the load-bearing level | New Gate 5–7 truth table, distinct fail messages, hermeticity negative control, regression invariance | `scripts/release-gate-logic-test.sh` (extend) + new stubs under `scripts/fixtures/` | pre-merge HARD |
| **Static / diff assertions** | `run_smoke_gate` byte-unchanged, single terminal marker, append-only ordering, `setup-node@v4` present, doc greps, uni-docs remit diff | same test script (static-grep section) + a small README/docs/release.yml grep harness | pre-merge HARD |
| **Live container round-trip** | Real `client-bundle` emit → host hermetic `init --bundle` → observe 204 → per-slug store grew | `docker-http-posture-smoke.sh` run via `run_smoke_gate` on a Docker+node lane | POST-TAG-CONFIRMABLE (PENDING) |
| **Inspection (human gate)** | uni-docs remit text, N5 framing, `--remote` "legacy" marker | reviewer | pre-merge inspection |

There is **no Rust `cargo test` surface** for this feature (no app code changes) and the
infra-001 **Python suites do not apply** — the new behavior is shell glue in a release workflow
with no MCP-visible effect; adding a Python suite test would be parallel scaffolding (#5189).
`cargo test --workspace` is still run at Stage 3c as a no-regression baseline.

## 3. Risk → Test Mapping

| Risk | Pri | Owning test(s) | Pre-merge? |
|------|-----|----------------|-----------|
| **R-01** new bundle failure greens the gate | **Crit** | `docker-http-posture-smoke.md` truth table (each ADR-001 row → fail()/exit-1, no marker); no-early-exit-0 | **YES (gap if PENDING)** |
| R-02 mis-attributed failure | High | `docker-http-posture-smoke.md` message-prefix assertions (each row's unique prefix; doc-drift vs route vs rename distinct) | YES |
| **R-03** regress nan-019 Gates 1–4 / contract | **Crit** | `docker-http-posture-smoke.md` truth-table invariance {0,1,3,4}; `set -e` survival; append-only ordering; single terminal marker; `release-gate-lib.sh` diff-unchanged | **YES** |
| R-04 host node absent / drift | High | `docker-http-posture-smoke.md` node-absent→exit-1 (NOT 3) + repo-checkout client; `release-yml-setup-node.md` pinned step present | YES (live version-compat PENDING) |
| R-05 blob handoff corruption | High | `docker-http-posture-smoke.md` stdout-only capture, prefix+non-empty validation, quoting-safe handoff, empty-capture guard | YES |
| R-06 `client-bundle` rename/absent | Med | `docker-http-posture-smoke.md` pinned invocation + named `client-bundle` failure | YES (rename detection); live-image-presence PENDING |
| **R-07** stale-credstore false-green | **Crit** | `hermeticity-negative-control.md` REQUIRED negative control (poison cred + broken attach → Gate 7 STILL fails) + isolation/clean-on-entry + fresh-write delta | **YES — REQUIRED pre-merge (gap if PENDING)** |
| R-08 executable-claim classification rots | High | `docs-client-setup.md` worked-example conformance; canonical-chain-only; no per-command gate | YES (inspection + grep) |
| R-09 `--slug` on bundle path | High | `docs-client-setup.md` + `readme-bundle-example.md` zero `--slug` w/ `--bundle`; `docker-http-posture-smoke.md` Gate 6 invokes no `--slug` | YES |
| R-10 README multi-occurrence miss | High | `readme-bundle-example.md` regex multi-occurrence grep (not line-pinned) | YES |
| R-11 gate logic un-provable pre-merge | High | All of §2 level-1; sourceable-spine reuse; RC-survival by execution; un-retryable store-grew non-flaky+discriminating | YES |
| R-12 AC-01 grep passes while drift remains | Med | `docs-client-setup.md` literal greps + obsolete-model sweep + enumerated-defect confirmation | YES |
| R-13 uni-docs remit scope creep | Med | `uni-docs-remit.md` authorship-text-only diff; blast-radius + non-goal present; no checker/gate/trigger | YES (inspection) |
| R-14 N5/remit human-owned | Low | `uni-docs-remit.md` inspection (AC-07/AC-08); explicitly NOT machine-checked | inspection |
| R-15 silent second image build | Low | `docker-http-posture-smoke.md` reuse-single-boot assertion; zero-new-script file-count | YES |
| **R-16** legacy `--remote` not doc-tested | **Accepted residual** | NONE for the round-trip; `docs-client-setup.md` + `readme-bundle-example.md` assert the "legacy" marker is the ONLY owed mitigation | inspection (no round-trip owed) |

**R-16 is consciously accepted — no `--remote` round-trip scenario is owed.** The sole owed
mitigation (the inspectable "legacy" marker in both files) is covered under R-09/R-10's doc plans.

## 4. AC → Test Mapping (AC-01..AC-09)

| AC | Covered by | Method | Pre-merge? |
|----|-----------|--------|-----------|
| AC-01 | `docs-client-setup.md` | grep (zero 501/W2-7/curl-observe; positive `init --bundle` + `/v1/{slug}/observe`) | YES |
| AC-02 | `docs-client-setup.md` + `readme-bundle-example.md` | grep+manual (both modes; `--remote` "legacy"; zero broken bundle-via-`--remote`; zero `--slug`+`--bundle`) | YES |
| AC-03 | `docker-http-posture-smoke.md` | stub gate-logic pre-merge; live round-trip POST-TAG | gate-logic YES / live PENDING |
| AC-04 | `docker-http-posture-smoke.md` | file-count delta 0 new scripts; round-trip inside the one smoke; no new CI job | YES |
| AC-05 | `docker-http-posture-smoke.md` | Docker-absent exit 3 → HARD-fail; each new skip path → distinct `fail()` exit 1 | YES |
| AC-06 | `docker-http-posture-smoke.md` | anchored marker grep fails on injected early `exit 0`; single/last marker | YES |
| AC-07 | `uni-docs-remit.md` | inspection (scope widened, blast-radius + non-goal, bounded relaxation, no Feature-2 machinery) | inspection |
| AC-08 | `uni-docs-remit.md` | inspection (N5 "usable-as-documented", bundle-chain-bound, no new NFR) | inspection |
| **AC-09** | `hermeticity-negative-control.md` | **REQUIRED pre-merge negative control** + clean-on-entry + fresh-write delta | **YES (gap if PENDING)** |

## 5. Integration Harness Plan (infra-001)

### Which suites apply

**None of the Python suites apply.** Per #5189 and the harness suite-selection table, the
Python suites validate **MCP-visible behavior of the `unimatrix` binary**. nan-020 changes **no
application code, no tool, no route, no schema** (Spec "NOT in Scope"). The behavior under test is
shell glue inside `release.yml` invoking the container + host `init`. Adding a `suites/test_*.py`
case would be parallel scaffolding that tests nothing nan-020 owns.

- **Smoke (`pytest -m smoke`)** — RUN at Stage 3c as the mandatory minimum gate, purely to prove
  nan-020 did not regress the binary (it should be untouched). Result recorded in the coverage report.
- **`tools` / `lifecycle` / `confidence` / `security` / etc.** — NOT run; nan-020 touches none of
  their surfaces. State this explicitly in the report (not a gap).

### The actual integration surface for this feature: the shell gate-logic harness

The doc-test IS an integration test — across the container→host boundary, the Rust→JS runtime
split, and the Gate 4→Gate 5 ordering seam. That integration is exercised **pre-merge against
stubs** in the shell gate-logic test, and **live post-tag** in `docker-http-posture-smoke.sh`.

### New pre-merge-provable tests to write (Stage 3c)

Extend the **existing** `scripts/release-gate-logic-test.sh` (cumulative — do NOT create a parallel
framework, #5189) and add stub fixtures under `scripts/fixtures/`. The new Gate 5–7 internal logic
(blob validation, node preflight, observe-code check, store-delta check, hermeticity sandbox) must
be reachable by the test against controllable stubs for the three new external commands:

1. **`fixtures/stub-client-bundle.sh`** — stands in for `docker run … client-bundle <slug>`.
   Env-driven: emit a chosen stdout body (a valid `unimatrix-bundle:` blob, an empty blob, a
   wrong-prefix blob, a blob with a trailing diagnostic + shell-significant chars) and a chosen rc.
2. **`fixtures/stub-init-bundle.sh`** — stands in for `node … init --bundle`. Env-driven rc
   (success / broken-attach), and a switch to simulate "no fresh write" vs "fresh write lands".
3. **`fixtures/stub-observe.sh`** (or env hooks) — stands in for the observe POST + store `du`
   delta: chosen HTTP code (204/200/404/500/501) and chosen store before/after sizes.

These let the new Gate 5–7 logic be driven through its whole truth table, the per-row distinct
messages asserted, and the hermeticity negative control (poison stale cred + broken attach → STILL
red) proven — all without Docker. This requires the **delivery** to factor the new gate logic so it
is invocable with the external commands injected (an env-overridable command indirection, mirroring
how nan-019 made `run_smoke_gate` take `SMOKE_CMD…`). The pseudocode component
`docker-http-posture-smoke` / `hermeticity-sandbox` owns that seam; the test plan **requires** it.

### What is PENDING-post-tag (accepted, NOT a gap)

- Real `client-bundle` emit from the **actual shipped image** (R-06 image-presence).
- Live host `init --bundle` decode/pin/Ping against the **real container** (R-04 version-compat,
  R-05 real blob).
- The **live** hermetic round-trip landing a real write in the per-slug store (R-07 live half).

Phrase as: *"configured + verified locally against stubs; GH execution confirmed post-tag."*
Never assert these as executed before the tag run (#4796 / #5189).

### Un-retryable assertion discipline (#5189)

The store-grew assertion has **no `|| retry`**. Before merge it must be proven (a) **non-flaky**
locally ≥5 runs against the stub, and (b) **discriminating** — the negative control (broken attach
→ store delta 0) must actually flip it red. A tolerance band that hides flakiness hides the defect.

## 6. Cross-Component Test Dependencies

- `hermeticity-negative-control` and `docker-http-posture-smoke` share the Gate 6/7 seam and the
  same stub fixtures — the hermeticity component's negative control is a special-case row of the
  Gate 5–7 truth table (broken attach + poisoned cred). They are split for clarity but co-implemented.
- `release-yml-setup-node` (static grep on `release.yml`) is independent but its node-pinned step is
  the *first line of defense* for R-04; `docker-http-posture-smoke`'s node-absent preflight is the
  *backstop*. Both are required (defense in depth).
- `docs-client-setup` and `readme-bundle-example` jointly satisfy AC-02 (both files) and jointly
  own the R-16 "legacy" marker mitigation and the R-09 no-`--slug` invariant.
- `docs-client-setup`'s executable-claim classification (R-08) must agree, claim-for-claim, with the
  three executable claims `docker-http-posture-smoke` actually exercises (Gates 5/6/7).

## 7. Self-Check (Stage 3a)

- [x] OVERVIEW maps every RISK-TEST-STRATEGY risk to a test (R-01/R-03/R-07 Critical called out).
- [x] Integration harness section: Python suites N/A (with reason); the real integration is the
      stub-driven shell gate-logic harness; new stub fixtures enumerated; PENDING-post-tag listed.
- [x] Per-component plans match the IMPLEMENTATION-BRIEF Component Map 1:1.
- [x] Every high-priority risk has ≥1 concrete test expectation.
- [x] Integration tests defined at the container→host / Rust→JS / Gate-4→Gate-5 boundaries.
- [x] R-07 negative control classified REQUIRED pre-merge (PENDING = gap), not deferred.
- [x] R-16 recorded as accepted residual, no round-trip scenario owed.
- [x] All output under `product/features/nan-020/test-plan/`.
- [x] Knowledge Stewardship block in the returned report.
