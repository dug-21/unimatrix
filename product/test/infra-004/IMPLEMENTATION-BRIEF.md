# IMPLEMENTATION-BRIEF — infra-004: Enforce Cross-Tenant Isolation as a Blocking Release Gate

> Compiled for Session 2 delivery. Test/CI-only feature — **no `crates/` change**.
> DoD (outcome altitude): **a cross-tenant leak cannot ship a release.**

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/test/infra-004/SCOPE.md |
| Scope Risk Assessment | product/test/infra-004/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/test/infra-004/specification/SPECIFICATION.md |
| Architecture | product/test/infra-004/architecture/ARCHITECTURE.md |
| Risk / Test Strategy | product/test/infra-004/RISK-TEST-STRATEGY.md |
| Alignment Report | product/test/infra-004/ALIGNMENT-REPORT.md |
| ADR-001 (warmup barrier placement + bound) | product/test/infra-004/architecture/ADR-001-warmup-barrier-placement-and-bound.md |
| ADR-002 (exit-2/INFRA tri-state) | product/test/infra-004/architecture/ADR-002-exit2-infra-tristate-handling.md |
| ADR-003 (blocking blast-radius containment) | product/test/infra-004/architecture/ADR-003-blocking-flip-blast-radius-containment.md |
| ADR-004 (cold-model proof + post-merge tag strategy) | product/test/infra-004/architecture/ADR-004-cold-model-proof-and-post-merge-tag-strategy.md |

## Goal

Convert the existing point-in-time cross-tenant isolation **proof**
(`multi-tenant-isolation-smoke.sh`, infra-003 / #855) into standing **enforcement**
so a genuine cross-tenant leak (RED) cannot ship a release. The full arc lands in
one feature as four risk-ordered deliverables: a bounded warmup barrier, an
additive tri-state runner + standing release lane, an in-feature cold-model
fresh-build GREEN proof, then the blocking flip via
`create-container-manifest.needs:`. On merge, capability **N3 (#5161)** moves
`partial → proven` (as-of the observe + MCP-write surfaces) and **N4** advances.

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| C-WB — warmup/readiness barrier (`multi-tenant-isolation-smoke.sh`) | pseudocode/C-WB-warmup-barrier.md | test-plan/C-WB-warmup-barrier.md |
| C-TS — `run_smoke_gate_tristate` (`release-gate-lib.sh`) | pseudocode/C-TS-tristate-runner.md | test-plan/C-TS-tristate-runner.md |
| C-LN — standing isolation lane (`release.yml`) | pseudocode/C-LN-standing-lane.md | test-plan/C-LN-standing-lane.md |
| C-FLIP — blocking flip into `create-container-manifest.needs:` (`release.yml`) | pseudocode/C-FLIP-blocking-flip.md | test-plan/C-FLIP-blocking-flip.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The
Component Map lists the four components from the architecture (C-WB/C-TS/C-LN/C-FLIP);
actual file paths are confirmed during delivery.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| D-1 — where exit-2/INFRA handling lands | **New additive** `run_smoke_gate_tristate` in the shared `release-gate-lib.sh`; `run_smoke_gate` untouched. Distinct `::warning::` + greppable marker + `return 0` on exit 2; never a silent return. | SCOPE D-1 / OQ-1 | architecture/ADR-002-exit2-infra-tristate-handling.md (Unimatrix #5350) |
| D-2 — fresh-build cold-model GREEN before the flip | **`workflow_dispatch` on the feature branch** (Option a). Test-only ⇒ byte-identical to `main`; exercises real `:latest-amd64` dispatch path + cold HF download. Fallback two-step merge only if branch dispatch can't push `:latest-amd64`. Pre-release dry-run tag rejected. | SCOPE D-2 / OQ-2 | architecture/ADR-004-cold-model-proof-and-post-merge-tag-strategy.md (Unimatrix #5352) |
| D-3 — arch coverage | **amd64-only blocking; no arm64 lane this round.** Routing is architecture-independent Rust; human-signed trade-off. | SCOPE D-3 | architecture/ARCHITECTURE.md §2; ADR-003 |
| Warmup barrier placement + mechanism + bound | Insert after `assert_routes_live`, before `run_isolation_matrix`; reuse `write_then_barrier` (one throwaway warmup write); `WARMUP_DEADLINE_SECS` default **180s** derived from #767 `READY_TIMEOUT_SECS`. Timeout → INFRA, never RED/GREEN. | SCOPE Goal 1 / AC-01..05 | architecture/ADR-001-warmup-barrier-placement-and-bound.md (Unimatrix #5349) |
| Blocking blast-radius containment + sqlite3 provisioning | Only script-exit-2 → non-blocking; **all** harness-step failures fail closed (block). Self-contained `apt-get install sqlite3` step, no hard dep on #849. ARCH §5 table is the verified contract. | SCOPE-RISK SR-04/SR-03 / OQ-1 | architecture/ADR-003-blocking-flip-blast-radius-containment.md (Unimatrix #5351) |

## Files to Create/Modify

| File | Change |
|------|--------|
| `product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh` | **C-WB** — add bounded warmup barrier between `assert_routes_live` and `run_isolation_matrix` (one throwaway `write_then_barrier` on `WARMUP_DEADLINE_SECS`). **The only permitted gate-script change.** |
| `product/test/infra-001/scripts/release-gate-lib.sh` | **C-TS** — add new additive `run_smoke_gate_tristate` function; `run_smoke_gate` byte-unchanged. |
| `.github/workflows/release.yml` | **C-LN** — new isolation lane job (node + sqlite3, GHCR login, `resolve_image`, invoke via `run_smoke_gate_tristate`); **C-FLIP** — add lane id to `create-container-manifest.needs:`. |

No other files change (plus feature docs). Verified by `git diff --stat` (AC-15).

## Data Structures / Key Constants

- **Exit-code tri-state** (gate verdict, unchanged): `GREEN=0`, `RED=1` (`fail`),
  `INFRA=2` (`infra_fail`), `SKIP=3` (Docker absent). Dominance:
  **RED > INFRA > GREEN**; no non-GREEN rounds to 0.
- **`WARMUP_DEADLINE_SECS`** — NEW env-overridable warmup deadline, default `180`
  (derived from #767 `READY_TIMEOUT_SECS=180`, ~2.5× over the ~70s embed
  retry/backoff floor; barrier's only delta over #767 is model-load, since per-slug
  store liveness is pre-established by `assert_routes_live`).
- **Warmup throwaway marker** — `infra003-warmup-${RUN}`, charset `[a-z0-9-]`,
  asserted **pairwise non-substring** of the four cell markers (runtime assertion).
- **Canonical INFRA marker (PIN THIS — see WARN below)** —
  `[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN`.
- **Verify-by-name GREEN marker** — `[infra003-smoke] ALL GATES PASSED`, credited
  via `grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'` against the **runtime**
  `log()`-prefixed line (never the source literal).

## Function Signatures / Integration Surface

| Symbol | Signature / behavior | Source (reuse / new) |
|--------|----------------------|----------------------|
| `resolve_image OWNER EVENT REF ARCH` | push → `ghcr.io/<owner>/unimatrix:v<version>-<arch>` (UN-stripped); dispatch → `:latest-<arch>` | `release-gate-lib.sh:26` — reuse verbatim |
| `run_smoke_gate IMAGE CMD…` | existing 4-lane runner; cases 0/3/4/1/*; **no exit-2 case** | `release-gate-lib.sh:44` — **untouched** |
| **NEW** `run_smoke_gate_tristate IMAGE CMD…` | rc0+marker→return 0; rc0-no-marker→`::error::`+return 1; rc1→`::error::`+return 1; rc2→`::warning::`+INFRA marker+return 0; rc3→`::error::`+return 1; `*`→`::error::`+return 1 | `release-gate-lib.sh` — **add (C-TS, ADR-002)** |
| `write_then_barrier surface slug store_dir marker` | bounded read-as-barrier; sets `WTB ∈ {PRESENT, INFRA}`; timeout→INFRA; stub seam via `SMOKE_*_CMD` | `multi-tenant-isolation-smoke.sh:271` — reuse for C-WB |
| `assert_routes_live` | C2 precondition: per-slug dbs exist + 4 routes non-404 | `multi-tenant-isolation-smoke.sh` — barrier inserted right after |
| Capture shape (R-05 PREREQUISITE) | `set +e; out="$(IMAGE="$image" "$@" 2>&1)"; rc=$?; set -e; echo "$out"` — **no pipe** between smoke and `$?`; `return`, never `exit` | ADR-002 / #5192 |

## Constraints

- **C-1** Test/CI-only — no `crates/` change; no gate-script logic change beyond the
  warmup barrier (verified by `git diff`).
- **C-2** Bounded, no-new-mechanism barrier; `WARMUP_DEADLINE_SECS` derived (with
  margin) from #767; past-deadline → INFRA.
- **C-3** Tri-state invariant and now enforced: RED blocks; INFRA non-blocking but
  visible; GREEN passes with marker; SKIP-on-Docker hard-fails. RED > INFRA > GREEN.
- **C-4** Pushed-bytes contract (#5180 / nan-019): `resolve_image`; UN-stripped push
  tag; `:latest-<arch>` on dispatch; **never** `${GITHUB_REF_NAME#v}`.
- **C-5** Single source of truth: exit-2 discrimination in `release-gate-lib.sh`,
  sourced by CI and the pre-merge stub test; no inline YAML logic.
- **C-6** sqlite3 provisioning self-contained in the lane (`apt-get install -y
  sqlite3`); coordinate with #849, do not block on it.
- **C-7** Stub-seam compatibility: warmup barrier must not break the off-Docker
  `SMOKE_*_CMD` gate-logic test; RED-blocks / INFRA-passes-visibly provable
  pre-merge via that seam + a `needs:`-graph assertion (no tag push required).
- **C-8** Additive shared-lib change (SR-08): purely additive; `run_smoke_gate`
  byte-unchanged; no existing blocking lane emits exit 2 today.
- **C-9** Blocking blast radius (SR-04): only the script's exit-2 maps to
  non-blocking; all harness/setup-step failures fail closed (block).
- **C-10** Never-green-on-a-tag tax (SR-05 / #5267 / ADR-004 #5184): AC-11's
  dispatch run proves warmup + verdict over `:latest-<arch>`, NOT tag-push
  resolution. Budget one post-merge tag round; lane is diagnostic-capture-first.
- **C-11** Byte-identical provenance (SR-06): AC-11 run requires the branch rebased
  on `main` (branch-point == `main` HEAD) so the build is current production bytes.
- **C-12** amd64-only blocking; no arm64 isolation lane this round (D-3).

## Dependencies

| Dependency | Relationship |
|-----------|--------------|
| #788 (closed) | The standing release gate this wires into and flips blocking (`release.yml`). |
| #855 / #853 / infra-003 | Delivers the gate this feature hardens, wires, and enforces (Unimatrix kernel #5347, lesson #5348). |
| #5180 | Verify-by-name / tri-state exit contract — exit-2 discrimination load-bearing for blocking. |
| #767 | Embed-readiness gate (`docker-embed-readiness-smoke.sh`) — provenance of the cold-first-boot warmup window (`READY_TIMEOUT_SECS=180`) and the cold-model proof reference. |
| #789 / crt-056 / C5 (#5190) | C5 per-slug surface proven (merged 2026-06-19) — removes N3's second `partial` caveat. |
| #849 | sqlite3 provisioning — coordinate; self-contained step means no hard ordering dependency. |
| #5161 (N3) | Capability `partial → proven` on this merge. |
| #5267 / #5184 (ADR-004) | Historical never-green-on-tag + dispatch-vs-tag resolution evidence (SR-05). |
| #5192 / #5258 / #4873 / #5345 | Sourceable-lib capture invariants (no-pipe / return-not-exit / set-e re-enable / runtime-marker). |
| N4 | Advanced (no false-alarm signals) via the warmup barrier + visible INFRA. |
| Existing scripts | `product/test/infra-001/scripts/{multi-tenant-isolation-smoke.sh, release-gate-lib.sh, isolation-probe-lib.sh}`. |

## Delivery Sequence (risk-ordered)

1. **Warmup barrier** in the gate script, #767-derived bound (Step 1 / AC-01..05).
2. **Visible exit-2/INFRA discrimination** in `release-gate-lib.sh` + the
   non-blocking lane in `release.yml` (Step 2 / AC-06..10).
3. **Cold-model fresh-build GREEN** via `workflow_dispatch` on the feature branch,
   demonstrated in-feature (Step 3 / AC-11) — *gates* step 4.
4. **Flip the lane** into `create-container-manifest.needs:` with
   RED-blocks / INFRA-passes-visibly semantics (Step 4 / AC-12..14).

## Gate 3c — Non-Negotiable Blockers (route to tester)

These two Critical risks MUST be proven before the blocking flip; they are
non-negotiable Gate 3c blockers:

- **R-01 — ceremonial warmup barrier / false-pass.** The throwaway
  `write_then_barrier` must be **load-bearing**: PRESENT must require an actual
  durable own-store write round-tripping the same `SMOKE_WRITE_CMD` /
  `SMOKE_READ_MARKER_CMD` a real write uses (not a liveness-only `store_size`
  poll), the PRESENT signal must gate proceed-to-matrix (consumed, not
  computed-and-discarded), and AC-11 must show zero warmup-attributable INFRA flap
  on the real cold path. N=1-green ≠ proven (#4974).
- **R-05 — swallowed-exit-code false-green in `run_smoke_gate_tristate`.** Must
  honor the #5192 PREREQUISITE: **no pipe** between the smoke invocation and `$?`;
  **`return`, never `exit`** (keep it unit-testable when sourced); re-enable
  `set +e; set -uo pipefail` after sourcing so an intentionally-RED truth-table row
  does not abort the suite (R-14); GREEN credited only via the **anchored** runtime
  marker grep. Full truth table proven by executing the real sourced lib against a
  stub smoke: (0+marker)→0, (0,no-marker)→1, (1)→1, (2)→0 with warning+marker,
  (3)→1, (other)→1.

## Delivery Notes / WARNINGs (from Alignment Report)

- **WARN — pin the canonical INFRA marker (delivery action, #3337 pattern).** The
  architecture shows the INFRA marker as illustrative (`e.g.
  [infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN`) while RISK-TEST R-09
  asserts that **exact literal** and the spec leaves it unpinned. Delivery MUST pin
  this exact string as the canonical marker in code so the tester's literal
  assertion matches — avoid a marker-divergence test failure.
- **WARN — diagnostic full-log echo is a workflow-command-injection surface
  (ARCH §6).** `run_smoke_gate_tristate` echoes the full smoke log on every path;
  container stdout (`::error::`/`::warning::`/`::set-output::` lines) is interpreted
  by the runner. Own-image, low blast radius, mitigated by the `-qxE` full-line
  marker anchor (a forged marker inside arbitrary output is not credited). Note it;
  do not loosen the anchor.
- **DELIVERY NOTE — AC-14 N3 (#5161) note rewrite.** The live N3 entry note still
  lists BOTH `partial` blockers as open, including the already-resolved C5/#5190
  caveat. Delivery's AC-14 update MUST **rewrite the full `partial` note** (record
  that this feature closed the only remaining blocker; the C5/#5190 caveat was
  already resolved by crt-056/#789), not merely flip the `status` field to `proven`.
  Use `context_correct` (never deprecate+store). Status enum literal is `proven`
  (enum: `missing | partial | proven | claimed`); "maintained / enforced" is prose
  only. N3 is proven **as-of the observe + MCP-write surfaces**.

## NOT in Scope

- No `crates/` change — production routing seam exercised as shipped.
- No gate-script change other than the warmup barrier (assertions, four-marker /
  non-substring scheme, read-as-barrier model, terminal run-marker, tri-state exit
  contract — untouched). No gate re-architecture.
- No new readiness mechanism for the barrier — reuse infra-001 idioms only.
- No arm64 isolation lane this round (D-3).
- No new smoke script (#815 invariant not in scope); no new local validation harness.
- No UDS behavioral probe / no parity-matrix shape.
- No automated chronic-INFRA escalation (SR-07) — visibility is `::warning::` +
  greppable marker only; escalation is a deferred follow-up (see Alignment Status).

## Alignment Status

Vision Alignment, Milestone Fit, Scope Gaps, Risk Completeness — **PASS**. Two WARN
items (INFRA-marker pinning #3337; diagnostic-echo injection surface) are captured
as delivery actions above. One **VARIANCE requires human approval before merge**:

> **VARIANCE (carry to human — do NOT resolve in delivery).** The headline DoD —
> "a cross-tenant leak cannot ship a release" — is qualified by chronic-INFRA
> (OQ-3 / SR-07 / R-15). Because INFRA does **not** block, a chronically-INFRA-dark
> lane (cold-HF throttle, wrong-tag pull-404, chronic warmup miss) lets a release
> ship with isolation **unverified**; the only mitigation is visibility
> (`::warning::` + greppable marker), unenforced. The human must either **ACCEPT**
> the residual as documented human-vigilance risk for infra-004 (consistent with
> "no new mechanism" discipline; the stable marker leaves a cheap escalation
> follow-up) **OR direct an escalation follow-up** (tracked INFRA count / threshold
> across N releases). **This gates the N3 `proven` claim (AC-14)** — it qualifies
> the DoD outcome and must be decided explicitly before merge.
