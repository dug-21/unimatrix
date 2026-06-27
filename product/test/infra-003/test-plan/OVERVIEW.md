# Test Plan OVERVIEW — infra-003

> Test-only feature. **The deliverable IS an integration test**: a standalone shell
> smoke gate `multi-tenant-isolation-smoke.sh` that proves bidirectional 2×2
> cross-tenant HTTP isolation in the shipped release container, as a cumulative
> extension of the infra-001 harness. This document therefore plans a
> **test-of-a-test**: how Stage 3c verifies the gate itself behaves correctly — its
> verdict logic, its INFRA-vs-RED discrimination, its fault-injection teeth — and
> how it slots into the infra-001 integration harness.

## Overall test strategy

A smoke gate's dominant failure is **false-GREEN / vacuous pass** (a gate that
passes while isolation is broken is worse than no gate — the mis-route corrupts the
wrong tenant's hash chain unrollbackably). The strategy is therefore two-tiered:

1. **Off-Docker gate-logic test (stub-driven) — the primary teeth.** Following the
   nan-019/nan-020 precedent (#5192 sourceable seam, #5258 stub-driven Docker-smoke
   gates), the gate routes its external probes (boot/route checks, the C5 read
   primitive, the C6 two-store read) through an injectable seam so the **verdict
   truth table** runs without Docker. This is where teeth are proven: a marker
   planted in the WRONG store must surface **RED**, and an own-store timeout must
   surface **INFRA (never RED, never GREEN)** — in both directions on both surfaces.
   A self-contained logic test sources the gate and drives these cases.
2. **Live run (Docker present) — the point-in-time property proof.** The gate boots
   the real shipped image, registers both slugs, drives four real writes, reads
   both stores on-disk, and emits GREEN only on the full 2×2 matrix. This is the
   actual isolation evidence; it cannot, by itself, prove the gate's teeth (a
   correct container never triggers RED), which is why tier 1 exists.

This mirrors #3624's lesson: a zero-regression/no-op gate validates only the
happy path — a positive integration test (here, the planted-leak fault injection)
is mandatory to prove the assertion can fail.

### Test-naming convention (shell logic test)

`test_{component}_{scenario}_{expected}`, e.g. `test_c7_planted_leak_is_red`,
`test_c5_own_timeout_is_infra_not_red`. Each maps to a component plan below.

## Component test plans (1:1 with pseudocode components)

| Component | Plan file | Primary risks |
|-----------|-----------|---------------|
| C1 — read-dependency preflight | `c1-preflight.md` | R-06, R-10, R-13 |
| C2 — registration + single restart + liveness precondition | `c2-registration.md` | R-07, R-11, R-08 |
| C3 — observe writes, both directions | `c3-observe-probe.md` | R-09, R-08, R-12, R-18 |
| C4 — MCP-write probe, both directions | `c4-mcp-probe.md` | **R-01, R-17**, R-02, R-09 |
| C5 — per-cell write + read-as-barrier | `c5-read-as-barrier.md` | **R-04, R-05**, R-06, R-08 |
| C6 — cross-store negative + two-store read | `c6-two-store-read.md` | **R-04, R-18**, R-07, R-12 |
| C7 — verdict gate (tri-state, pos-gates-neg) | `c7-verdict.md` | **R-02, R-03, R-10**, R-18, R-14 |

## Risk-to-test mapping (all 18 risks)

| Risk | Priority | Covered by | Key teeth test |
|------|----------|-----------|----------------|
| R-01 MCP handshake mis-built (×2 dir) | Critical | C4 | `test_c4_handshake_failure_is_infra` |
| R-02 load-bearing MCP probe vacuous | Critical | C4, C5, C6, C7 | `test_c7_planted_leak_is_red` (MCP cells) |
| R-03 positive-gates-negative inversion | Critical | C7 | `test_c7_positive_gates_negative_per_direction` |
| R-04 WAL pre-checkpoint false-empty (both stores) | Critical | C5, C6 | `test_c6_other_store_wal_copied` |
| R-05 durability-barrier soundness (read-as-barrier) | Med | C5, C7 | `test_c5_own_timeout_is_infra_not_red` |
| R-06 read-dependency absent → empty-pass | High | C1, C5 | `test_c1_sqlite3_absent_is_infra` |
| R-07 liveness-as-verdict / missing-B INFRA | High | C2, C6 | `test_c2_missing_b_store_is_infra` |
| R-08 stale store / non-unique markers (×4) | High | C2, C3, C5 | `test_c3_markers_per_run_unique` |
| R-09 marker round-trip into column | High | C3, C4 | `test_c3_marker_roundtrips_to_column` |
| R-10 INFRA/RED/GREEN tri-state collapse | High | C1, C7 | `test_c7_tristate_exit_codes` |
| R-11 slug-B collision (resolved by design) | Low | C2 | `test_c2_fresh_isolation_b` |
| R-12 marker SQL/LIKE metacharacters | Med | C3, C6 | `test_c3_marker_charset_safe` |
| R-13 cumulative coupling to posture-smoke libs | Med | C1 / self-containment | `test_c1_no_warn_continue` + SR-12 self-containment |
| R-14 overclaim / parity reintroduction | Low | C7 | `test_c7_no_overclaim_no_parity` |
| R-15 new-smoke-script invariant trip (#815) | High | **delivery action** | `test_no_new_smoke_script` (updated, see below) |
| R-16 standing-gate orphan (#788) | High | **delivery action** | #788 durable adoption linkage (see below) |
| R-17 crossed/reused `Mcp-Session-Id` | High | C4 | `test_c4_session_captured_per_route` |
| R-18 marker substring collision under `LIKE` | Med | C3, C6, C7 | `test_c7_four_markers_mutually_non_substring` |

All 18 risks have at least one concrete test expectation; every Critical/High risk
has a named teeth or INFRA-discrimination test. R-15/R-16 are
**delivery-coordination** obligations, not in-gate logic (tracked below).

## Cross-component test dependencies

- C7's verdict tests depend on the C5/C6 **stub seam** (injectable read results) —
  Stage 3b must expose this seam so the teeth tests can run without Docker.
- C5 (positive PRESENT) gates C6 (negative read) per direction; C7 enforces the
  ordering. The teeth tests exercise the join: planted leak → RED **even when** the
  own positive timed out INFRA.
- C3/C4 marker round-trip (R-09) is verified by the same read C5 uses as its
  positive control — one read, two purposes (mapping proof + barrier).

## Integration harness plan (infra-001)

This gate is itself a new member of the infra-001 **shell smoke** family
(`scripts/*smoke*.sh`), not the pytest suite family. Mapping to the suite-selection
question:

- **The pytest suites (`protocol`, `tools`, `lifecycle`, …) do not apply** — they
  drive MCP over **stdio** against an in-process binary; they have no `vol`/cert/
  two-store machinery and exercise no per-slug HTTP routing. infra-003 deliberately
  rejects pytest hosting (ADR-001, H2 deferred) for exactly this reason. No new or
  modified pytest test is planned.
- **The applicable harness leg is the shell smoke lane.** The gate reuses infra-001
  primitives by **sourcing only define-on-source libs** (`cloud-bundle-lib.sh`
  content-read helpers — `vol`/`sqlite3`/WAL-aware `vol cat`/sqlite3 hard-INFRA) and
  replicating the thin `vol()`/cert-pull boot idiom from `docker-http-posture-
  smoke.sh`. Per SR-12 it is a **separate top-level script with self-contained
  assertions** — it does NOT graft onto posture-smoke Gates 1–8, so an upstream
  posture-smoke change surfaces here as an explicit failure, not a silent skip
  (R-13).
- **How it is invoked.** Directly: `product/test/infra-001/scripts/multi-tenant-
  isolation-smoke.sh`. In the release/CI lane it is run via `run_smoke_gate`
  (`release-gate-lib.sh`), which enforces the verify-by-name / exit-code contract
  (#5180): exit 0 + a terminal `[<name>-smoke] ALL GATES PASSED` line, exit 3 =
  SKIP treated as hard failure (never green), exit 1 = RED. The gate's distinct
  INFRA exit is non-zero/non-green, consistent with that contract.
- **New integration tests to add (Stage 3c).** (1) the gate script itself; (2) a
  self-contained off-Docker **gate-logic test** that sources the gate and drives
  the C5/C6/C7 stub truth table (planted-leak → RED, own-timeout → INFRA, all-clean
  → GREEN-marker) — modelled on `release-gate-bundle-static-test.sh` /
  `release-gate-cloud-cycle-logic-test.sh`. No existing suite covers per-slug HTTP
  isolation through the release artifact, so this is net-new behavioral coverage.
- **Minimum gate.** The infra-001 pytest `-m smoke` set remains the unrelated
  baseline; Stage 3c still runs it as a regression check, but the infra-003
  property is proven by this shell gate + its logic test, not by pytest.

## R-15 — new-smoke-script invariant update (#815), IN-PR lockstep

The new top-level script **will trip** the existing invariant
`test_no_new_smoke_script()` in
`product/test/infra-001/scripts/release-gate-bundle-static-test.sh` (lines ~186–226):
it globs `scripts/*smoke*.sh` (excluding `stub-smoke.sh`) and asserts **exact set
equality** against a closed `KNOWN_SMOKE_SCRIPTS` allowlist (currently
`docker-http-posture-smoke.sh`, `docker-embed-readiness-smoke.sh`). Any unaccounted
script → RED "FORK smell".

**Plan (same delivery PR, cross-linked on #815):**
1. Add `multi-tenant-isolation-smoke.sh` to `KNOWN_SMOKE_SCRIPTS` in the **same PR**
   that introduces the script (not a follow-up).
2. **Verify the invariant still has teeth** after the update: a Stage-3c assertion
   that `test_no_new_smoke_script` (a) **passes** with the three expected scripts
   present, and (b) **still fails** if a synthetic unaccounted `*smoke*.sh` is added
   (the FORK-detection arm) and if a known script is removed (the missing arm). The
   guard must keep its closed-set discipline, not be loosened to a wildcard.
3. Confirm the new gate honors the verify-by-name / exit-code contract the invariant
   ecosystem enforces (#5180): the terminal `*-smoke ALL GATES PASSED` marker
   matches `\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*` and no path early-exits 0
   without it (covered by `test_c7_terminal_marker_matches_grep`).
4. The leader posts the cross-link comment on **#815**; #815's intent is closed in
   this change.

## R-16 — standing-gate adoption (#788), delivery-coordination note

This is a **point-in-time** gate; N3 (#5161) stays `partial`. The N5/#788 recurring
regression lane is **not wired here** (out of scope). The delivery action is a
**durable adoption comment on #788** requiring N5/#788 to adopt infra-003's gate
into the recurring lane (run on N5's cadence), advancing N3 from point-in-time
toward "maintained". Stage 3c/validation verifies: (a) the #788 linkage is durable
(tracked GitHub comment, not an informal note), and (b) capability evidence wording
is "advances, does not close N3" and names the #788 adoption as the path to
"maintained" — so no reader misreads a delivery pass as a standing guarantee (R-14).

## Acceptance-criteria coverage (AC-01…AC-15)

| AC | Component plan | AC | Component plan |
|----|----------------|----|----------------|
| AC-01 | C2 | AC-09 | C7 |
| AC-02 | C3 | AC-10 | C5, C7 |
| AC-03 | C5 | AC-11 | C5 (C1 sqlite3 presence) |
| AC-04 | C6 | AC-12 | C6 |
| AC-05 | C7 | AC-13 | C2 (+ `git diff` no `crates/`) |
| AC-06 | C4 | AC-14 | C7 |
| AC-07 | C5 | AC-15 | C4 |
| AC-08 | C6 | | |

All 15 ACs map to a component plan; AC-13's "no production change" is additionally
a Stage-3c `git diff` check (no `crates/` edit), AC-14 a grep (no UDS/parity).

## Knowledge Stewardship

- Queried: `context_briefing` + `context_search` — surfaced the infra-003 ADRs
  (#5335/#5342/#5343/#5344), the false-green/verify-by-name patterns
  (#5180/#5192/#5258), and the cumulative-test convention (#238). Applied: the
  stub-driven off-Docker logic-test tier and the verify-by-name marker contract are
  lifted directly from the nan-019/nan-020 precedent.
- Stored: nothing novel at plan stage — the patterns reused are already in
  Unimatrix (#5180/#5192/#5258); a Stage-3c retro may store an
  isolation-specific fault-injection (planted-leak) pattern if it proves reusable.
