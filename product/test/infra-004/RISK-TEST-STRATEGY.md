# Risk-Based Test Strategy: infra-004

> Mode: architecture-risk. Scope: the three touched files — `multi-tenant-isolation-smoke.sh`
> (warmup barrier C-WB), `release-gate-lib.sh` (additive `run_smoke_gate_tristate` C-TS),
> `.github/workflows/release.yml` (standing lane C-LN + `needs:` flip C-FLIP). DoD: **a
> cross-tenant leak cannot ship a release.** The dominant failure class is not "the gate is
> absent" but **silently-vacuous enforcement** — the gate is blocking yet never RED and never
> GREEN, so isolation is never actually verified and the release ships anyway. Risks below are
> design-specific; generic CI risks are omitted.
>
> Historical evidence applied: #5267 (never-green-on-tag → R-10), #5180 (self-skip must fail not
> pass → R-05/R-06), #5345/#5192/#4873 (sourceable-lib capture: no-pipe / return-not-exit /
> `set -e` re-enable / runtime-marker → R-05/R-06/R-14), #4974 (ceremonial seam — N=1 green ≠
> guarantee proven → R-01/R-13).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Warmup barrier is **ceremonial** — its throwaway `write_then_barrier` returns PRESENT without actually proving the embedding model is loaded, so C3/C4 still flap INFRA (or worse the barrier "passes" while the precondition it claims is unestablished). N=1-green-≠-proven (#4974). | High | Med | **Critical** |
| R-05 | **Swallowed-exit-code false-green** in `run_smoke_gate_tristate`: a pipe between the smoke and `$?`, or `exit` instead of `return`, or sourced `set -e` re-enabling, makes a RED (exit 1) round to 0 → a genuine leak ships. The #4873/#5345 class, reintroduced in the additive function. | High | Med | **Critical** |
| R-03 | The #767-derived 180s bound under-covers **this** gate's readiness (SR-01): if `assert_routes_live` does not actually establish "a store write becomes durable" (only routes non-404 + dbs exist), the barrier must cover model-load **plus** first-durable-write, which may exceed 180s cold → INFRA flap → vacuous. | High | Med | High |
| R-06 | **Anchored run-marker invariant break** (#5345 finding c): GREEN credited via `grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'` against the **runtime** `log()`-prefixed line, not the source literal. A timestamp/prefix change breaking the full-line `-x` match → real GREEN rounds to block (all releases blocked); a loosened anchor → early-exit-0 false-green. | High | Med | High |
| R-08 | **Blocking blast-radius / fail-closed inversion** (SR-04): only script-exit-2 may map to non-blocking. A regression mapping the `*)` catch-all or a harness-step failure to `return 0` lets a non-isolation failure silently pass; mapping script-INFRA to block reds every release on warmup noise. | High | Med | High |
| R-09 | **Pull-404 / wrong-tag → chronic visible-INFRA = vacuous** (ADR-003 B): the smoke classifies a failed pull as exit 2 (non-blocking), diverging from `run_smoke_gate`'s exit-4-blocks. A chronically-wrong `resolve_image` tag never pulls the right bytes → always INFRA → isolation never verified, release ships. Central risk via tag resolution. | High | Med | High |
| R-10 | **Never-green-on-a-tag** (SR-05 / #5267): AC-11's dispatch run proves `:latest-<arch>` resolution only; the `:v<version>-<arch>` tag-push path + the blocking `needs:` edge first execute on a real tag **post-merge**. A tag-resolution divergence surfaces only then — degrading to vacuous-INFRA (safe) or, if a harness step, blocking a healthy release. | High | Med | High |
| R-13 | **AC-11 cold-model proof is ceremonial** (#4974): if the dispatch run hits a warm model cache / stale `:783-smoke` artifact instead of the real first-boot HF download, it proves GREEN on a warm path while a real cold release container flaps INFRA. AC-04 is "proven by AC-11" — a non-cold AC-11 makes the entire central-risk defense vacuous. | High | Med | High |
| R-02 | **Warmup-marker collision**: `infra003-warmup-${RUN}` must be pairwise non-substring of the four cell markers and inert to the negative-cell foreign-marker greps. A substring overlap → the warmup write is read as a foreign marker in a negative cell → **false RED** (blocks all releases) or masks a real leak. | Med | Low | Med |
| R-07 | **Sibling-lane regression** (SR-08): `release-gate-lib.sh` is sourced by the four existing **blocking** lanes (`smoke/embed-amd64/arm64`). A non-additive edit (touching the `*) → return 1` catch-all or a shared helper) shifts sibling behavior → all release lanes affected. | High | Low | Med |
| R-14 | **Verification itself goes false-green** (#5345): the pre-merge truth-table test sources the real lib + smoke (which set `set -euo pipefail`); without `set +e; set -uo pipefail` after sourcing, the first intentionally-RED row aborts the suite — every prior row green, coverage silently dropped, the R-05/R-06 regressions ship unverified. | High | Low | Med |
| R-04 | **Cold HF download variance** (SR-02): runner bandwidth / HF availability on release day governs whether 180s holds. A throttled HF exceeds the bound → INFRA on the very release it guards. Degrades to vacuous-but-visible (accepted residual; not eliminated). | Med | Med | Med |
| R-11 | **Stale-image proof** (SR-06): AC-11 builds from the feature-branch tip; if `main` advances past the branch point, "byte-identical to `main`" is false → AC-11 proves a stale image and a real cold-model regression on `main` HEAD ships unproven. | Med | Med | Med |
| R-12 | **Dispatch-from-branch GHCR write** (SR-09): if the runner/token cannot push `:latest-amd64` from a non-default branch, AC-11 cannot run → Step 3 stranded → feature re-splits to the two-step fallback. | Med | Med | Med |
| R-15 | **Chronic-INFRA = human-vigilance only** (SR-07): the `::warning::` + greppable marker is visible but unenforced. A gate INFRA across N releases stays non-failing — silently-vacuous-if-unwatched, with no automated escalation in scope. | Med | Med | Med |

## Risk-to-Scenario Mapping

### R-01: Warmup barrier is ceremonial (false-pass)
**Severity**: High **Likelihood**: Med
**Impact**: The barrier "passes" but does not establish model-loaded; cold C3/C4 still flap INFRA (vacuous), or — worse — the barrier masks a not-ready condition into a proceed → the matrix runs on a half-warm server with non-deterministic verdicts.
**Test Scenarios**:
1. Stub seam: force the throwaway warmup `write_then_barrier` to PRESENT while the model is unloaded — assert the barrier does **not** proceed unless the PRESENT path actually exercised the embed/durable-write path (the warmup write must round-trip through the same `SMOKE_WRITE_CMD`/`SMOKE_READ_MARKER_CMD` that a real durable write uses, not a liveness-only `store_size` poll).
2. Verify-the-funnel: confirm the warmup PRESENT signal is consumed (gates proceed-to-matrix), not computed-and-discarded; grep the barrier for an unused result binding.
3. Cold-path AC-11: the dispatch run reaches `run_isolation_matrix` GREEN with **zero** INFRA flap attributable to warmup (NFR-1).
**Coverage Requirement**: The barrier's PRESENT outcome must be load-bearing — proven to require an actual durable own-store write (model warm), not merely route-liveness; demonstrated GREEN on the real cold path (AC-04 via AC-11).

### R-05: Swallowed-exit-code false-green in `run_smoke_gate_tristate`
**Severity**: High **Likelihood**: Med
**Impact**: A RED (exit 1) is captured as 0 → the lane passes → a genuine cross-tenant leak ships. Direct DoD violation.
**Test Scenarios**:
1. Stub-seam EXECUTION (not YAML reading) of the exact capture shape `set +e; out="$(IMAGE=… "$@" 2>&1)"; rc=$?; set -e` against a stub smoke exiting 1 → assert `rc==1` and the function returns 1 (#4873/#5345 RC-survives-capture).
2. Assert **no pipe** between the smoke invocation and `$?`; assert the function `return`s, never `exit`s (sourcing must remain unit-testable).
3. Full truth table through the **real** sourced lib: (0+marker)→0, (0,no-marker)→1, (1)→1, (2)→0, (3)→1, (other)→1.
**Coverage Requirement**: Every exit-code cell proven by executing the real lib against a stub smoke; capture-shape invariants (no-pipe, return-not-exit) asserted, not assumed.

### R-06: Anchored run-marker invariant break
**Severity**: High **Likelihood**: Med
**Impact**: A broken full-line `-x` anchor rounds real GREEN to block (every release blocked) or, if loosened, credits an early-exit-0 as GREEN (vacuous pass).
**Test Scenarios**:
1. Stub emits exit 0 **with** the runtime-prefixed line `[infra003-smoke] ALL GATES PASSED` → credited GREEN; exit 0 **without** the marker → `::error::` early-exit-0 + return 1.
2. Reconstruct the **runtime** marker (`log()` prepends `[<tag>-smoke] ` at runtime) for the grep cross-check — never grep the source literal (#5345 finding c).
3. Negative: a stub printing `ALL GATES PASSED` as a substring inside a longer line is **not** credited (the `-x` full-line anchor holds).
**Coverage Requirement**: Marker check proven against the runtime line shape, with both the credited and the substring-rejection cases in the truth table.

### R-03: #767 bound under-covers this gate's readiness
**Severity**: High **Likelihood**: Med
**Impact**: If the barrier must cover model-load + first-durable-write (not just model-load), 180s under-covers cold → INFRA flap → vacuous.
**Test Scenarios**:
1. Confirm `assert_routes_live` establishes per-slug store liveness/registration **before** the barrier (so the barrier's delta over #767 is model-load only — the ADR-001 claim); if it only checks routes-non-404, flag the bound as under-scoped.
2. AC-11 cold dispatch run completes GREEN with documented wall-clock headroom under `WARMUP_DEADLINE_SECS`.
3. Document the derivation (the #767 `READY_TIMEOUT_SECS=180`, ~2.5× over the ~70s backoff floor) and the headroom in the diff.
**Coverage Requirement**: The bound's provenance is validated against this gate's actual readiness delta, and the cold-path headroom is observed empirically in AC-11 — not asserted.

### R-08: Blocking blast-radius / fail-closed inversion
**Severity**: High **Likelihood**: Med
**Impact**: A non-isolation failure silently passes (DoD hole) or warmup noise blocks every release.
**Test Scenarios**:
1. Truth-table: assert **only** script-exit-2 → `return 0` (non-blocking-visible); 1/3/0-no-marker/other → return 1 (block).
2. Validate the ARCHITECTURE §5 fail-closed table cell-by-cell: harness-step failures (checkout, GHCR login, the sqlite3 setup step) fail the job (block); script-exit-2 (warmup/pull/dep) does not block but is visible.
3. `needs:`-graph assertion: the lane id ∈ `create-container-manifest.needs:` so exit-1 gates the manifest (AC-12).
**Coverage Requirement**: Every row of the §5 blast-radius table mapped to a test (truth-table cell or YAML/graph assertion); no harness failure path maps to non-blocking.

### R-09: Pull-404 / wrong-tag → chronic visible-INFRA = vacuous
**Severity**: High **Likelihood**: Med
**Impact**: A persistently-wrong `resolve_image` tag never pulls the right bytes → always INFRA → isolation never verified across N releases; the release ships. The central risk via tag resolution.
**Test Scenarios**:
1. Confirm a pull failure maps to script-exit-2 (INFRA, non-blocking-visible) and emits the `::warning::` + distinct marker (`[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN`).
2. Assert `resolve_image` is called with the correct event/ref (push → `:v<version>-<arch>` UN-stripped; dispatch → `:latest-<arch>`); assert **no** `${GITHUB_REF_NAME#v}` usage anywhere in the lane (the nan-019 swallow class).
3. R-15 linkage: the INFRA marker is stable and greppable so a "grep recent release runs for the INFRA marker" check is cheap.
**Coverage Requirement**: Pull-fail→INFRA-visible proven; tag-resolution call-shape asserted by YAML review; the divergence from exit-4-blocks documented and tied to the visible-INFRA escalation surface.

### R-10: Never-green-on-a-tag (tag-push path unproven pre-merge)
**Severity**: High **Likelihood**: Med
**Impact**: The `:v<version>-<arch>` tag-push resolution + the blocking `needs:` edge first run on a real tag post-merge; a divergence surfaces only then.
**Test Scenarios**:
1. AC-11 dispatch run is recorded as proof of **warmup + verdict + full harness on the dispatch path only** — explicitly **not** tag-push resolution (per ADR-004).
2. Diagnostic-capture-first: `run_smoke_gate_tristate` echoes the full smoke log on every path so the first real tag yields a diagnosis, not a guess.
3. Budget **one** post-merge tag round as expected cost; verify a tag-path INFRA (e.g. pull 404) degrades to non-blocking (does not block a healthy release).
**Coverage Requirement**: AC-11 evidence scoped to dispatch; post-merge tag round explicitly budgeted; the only first-tag path that can block a healthy release (a harness-step failure) is the one AC-11 already exercised.

### R-13: AC-11 cold-model proof is ceremonial (warm cache)
**Severity**: High **Likelihood**: Med
**Impact**: A warm-cache AC-11 proves GREEN on a path a real release container never takes; AC-04's defense against silently-vacuous enforcement is itself vacuous.
**Test Scenarios**:
1. AC-11 log must show the **real first-boot HuggingFace download** lines (not a warm cache, not `:783-smoke`); record the cold-path evidence in the feature folder.
2. Confirm the dispatch image is freshly built (`:latest-amd64` from the just-built bytes), not a reused/cached image with a pre-warmed model layer.
3. Cross-check: the observed cold wall-clock is consistent with a genuine download (within `WARMUP_DEADLINE_SECS` with headroom), not a sub-second warm load.
**Coverage Requirement**: AC-11 evidence explicitly demonstrates the cold first-boot download path was taken; a warm-cache run is rejected as proof.

### R-02: Warmup-marker collision → false RED / masked leak
**Severity**: Med **Likelihood**: Low
**Impact**: A substring overlap with a cell marker causes the warmup write to be read as a foreign marker in a negative cell → false RED (blocks all releases) or masks a real leak.
**Test Scenarios**:
1. Runtime assertion (ADR-001): `infra003-warmup-${RUN}` is pairwise non-substring of the four cell markers — assert it fails loudly if violated.
2. Negative-cell greps query specific foreign markers; confirm none match the warmup marker (the warmup row is inert to the matrix).
**Coverage Requirement**: The non-substring invariant is enforced at runtime and covered by a stub-seam case that would trip on a colliding marker.

### R-07: Sibling-lane regression (shared lib)
**Severity**: High **Likelihood**: Low
**Impact**: A non-additive edit shifts behavior for the four existing blocking lanes → all release lanes affected.
**Test Scenarios**:
1. `git diff` confirms `run_smoke_gate` is byte-unchanged; the change is a **new** `run_smoke_gate_tristate` function only.
2. Run the existing `run_smoke_gate` truth table (0/3/4/1/*) post-change → identical results (no sibling drift).
3. Confirm no existing lane emits exit 2 today (the new branch is purely additive surface).
**Coverage Requirement**: Both functions' full truth tables pass; `run_smoke_gate` proven unchanged by diff + execution.

### R-14: Verification harness goes false-green (`set -e` re-enable)
**Severity**: High **Likelihood**: Low
**Impact**: The pre-merge truth-table suite silently aborts on the first RED row — R-05/R-06 regressions ship unverified.
**Test Scenarios**:
1. After sourcing the real lib + smoke (which set `set -euo pipefail`), the harness explicitly `set +e; set -uo pipefail` before driving RED cells (#5345).
2. Assert the suite's **final summary line** prints (all rows ran), not just per-row oks.
3. Intentionally inject a RED row first and confirm subsequent rows + summary still execute.
**Coverage Requirement**: The test harness is proven to run all truth-table rows including intentionally-RED cells, with the summary line as the completeness witness.

### R-04: Cold HF download variance
**Severity**: Med **Likelihood**: Med
**Impact**: A throttled/unreachable HF on release day exceeds 180s → INFRA on the guarded release. Degrades to vacuous-but-visible (accepted residual).
**Test Scenarios**:
1. Past-deadline (timeout) → INFRA exit 2, never RED/GREEN (AC-03), with the visible `::warning::` + marker.
2. The barrier logs diagnostic last-state on timeout (slow-download INFRA is diagnosable).
**Coverage Requirement**: Timeout→INFRA-visible proven; residual accepted as documented (no pre-pull/pin added — out of scope), bounded by R-15 visibility.

### R-11: Stale-image proof (main drift)
**Severity**: Med **Likelihood**: Med
**Impact**: AC-11 proves a stale image; a cold-model regression on `main` HEAD ships unproven.
**Test Scenarios**:
1. Assert the feature branch is rebased on `main` (or branch-point == `main` HEAD) **immediately before** the AC-11 dispatch (FR-16 / SR-06).
2. Record the branch SHA and `main` HEAD SHA with the AC-11 evidence.
**Coverage Requirement**: Branch-point == `main` HEAD asserted at AC-11 run time, recorded as evidence.

### R-12: Dispatch-from-branch GHCR write strands Step 3
**Severity**: Med **Likelihood**: Med
**Impact**: AC-11 cannot run → feature re-splits to the two-step fallback.
**Test Scenarios**:
1. Verify GHCR `packages: write` + `:latest-amd64` push from the feature branch **early**, before building Step 3 on D-2(a) (SR-09 / OQ-2).
2. Keep the two-step fallback (land non-blocking → dispatch → follow-up flip) specified and ready.
**Coverage Requirement**: Dispatch-from-branch push capability confirmed before Step 3 work; fallback path documented.

### R-15: Chronic-INFRA = human-vigilance only
**Severity**: Med **Likelihood**: Med
**Impact**: Visible-but-unenforced INFRA across N releases = silently-vacuous-if-unwatched.
**Test Scenarios**:
1. The INFRA marker string is stable and greppable across runs (enables a future scheduled "grep recent release runs" alert).
2. Confirm the human has accepted chronic-INFRA as documented human-vigilance risk (OQ-3) — automated escalation explicitly out of scope.
**Coverage Requirement**: Marker stability proven; human acceptance of the residual recorded; escalation noted as a cheap follow-up.

## Integration Risks

- **C-TS ↔ C-WB exit-code fidelity**: the script's `verdict()` produces the exit code; C-TS only maps it. RED>INFRA>GREEN dominance must survive end-to-end — a warmup INFRA (exit 2) must not mask a downstream RED (it can't, because warmup precedes the matrix and timeout exits 2 immediately; but confirm the barrier returns rather than continuing into the matrix on timeout). (R-01, R-08)
- **C-LN ↔ `resolve_image`**: dispatch vs tag-push resolve different tags (ADR-004 #5184); the lane's correctness on tag-push is unproven until post-merge. (R-09, R-10)
- **C-FLIP ↔ existing `needs:` siblings**: the new lane joins four existing blocking lanes; its harness exposure must mirror, not exceed, theirs (one extra sqlite3 step). (R-07, R-08)
- **Shared `release-gate-lib.sh` ↔ stub test ↔ CI**: the lib is the single source of truth sourced by both; the no-pipe/return/`set -e` invariants must hold identically in both contexts. (R-05, R-14)

## Edge Cases

- Exit 0 with no marker (early-exit-0) → must block (R-06).
- Exit 2 with no `::warning::`/marker emitted → silent INFRA = defect (R-09, R-15, NFR-2).
- Warmup marker that is a substring of (or contains) a cell marker → false RED (R-02).
- `${GITHUB_REF_NAME#v}` anywhere in the lane → tag-resolution swallow (R-09).
- `main` HEAD advancing during the AC-11 window → stale proof (R-11).
- Unexpected exit code (`*`) → must block, never round to pass (R-05, R-08).
- SKIP (exit 3) on a Docker-present release runner → hard failure, never silent pass (R-08).

## Security Risks

This is a test/CI feature with **no `crates/` change**; the production routing seam is exercised as shipped. Untrusted/variable inputs and blast radius:

- **Untrusted input — the pulled container image and its echoed log.** `run_smoke_gate_tristate` captures `out="$(… 2>&1)"` and echoes the **full smoke log** on every path (diagnostic-capture-first). The container image is built from our own `main`, but its stdout is echoed verbatim into the workflow — a **workflow-command-injection surface** (`::error::`/`::warning::`/`::set-output::` lines in container output would be interpreted by the runner). Blast radius: limited because the image is our own build, but the lane runs with `packages: write` + GHCR login. Coverage: confirm echoed smoke output cannot forge the GREEN credit (the marker grep is `-qxE` full-line anchored — a forged marker inside arbitrary output is not credited, R-06) and note the echo as an accepted, own-image surface.
- **Tag resolution from `GITHUB_REF`.** A `${GITHUB_REF_NAME#v}` mis-resolution (forbidden by C-4) could pull an unintended tag. Coverage: assert the forbidden pattern is absent and `resolve_image` is the sole resolver (R-09).
- **`sqlite3` provisioning via `apt-get`.** Runtime supply-chain surface on the runner; self-contained step. Blast radius: a compromised/failed install fails closed (blocks the manifest, R-08) rather than silently passing — the safe direction.
- **Blast radius if compromised:** the lane sits in the release pipeline and can block the manifest; the worst outcome of a wiring defect is not data exposure but **vacuous enforcement** (a leak ships) — which the entire register above is structured to prevent.

## Failure Modes

| Condition | Expected behavior |
|-----------|-------------------|
| Genuine cross-tenant leak (RED, exit 1) | Job fails, `needs:` edge unmet, manifest does not assemble (DoD) |
| Warmup not ready past deadline (exit 2) | INFRA — `::warning::` + greppable marker, **return 0**, manifest proceeds, isolation flagged not-verified |
| Image pull 404 / wrong tag (exit 2) | INFRA — non-blocking-visible (deliberate divergence from exit-4-blocks); diagnosable via captured log |
| Early-exit-0 (exit 0, no marker) | `::error::`, return 1, blocks |
| SKIP (exit 3) on Docker-present lane | `::error::` mis-provisioned, return 1, hard fail |
| Harness-step failure (checkout/login/sqlite3 setup) | Job fails, blocks (fail-closed) — found pre-flip by AC-11 dispatch |
| Cold HF download throttled | INFRA-visible (vacuous-but-loud), accepted residual; never a false GREEN/RED |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (bound calibrated for embed round-trip, not this gate) | R-03 | ADR-001: barrier's only delta over #767 is model-load (store liveness pre-established by `assert_routes_live`); validate that claim + observe cold headroom in AC-11. |
| SR-02 (cold HF download external/variable) | R-04 | ADR-001: classify-don't-pre-pull; past-deadline→INFRA-visible; AC-11 demonstrates the bound holds cold. Residual accepted. |
| SR-03 (sqlite3 provisioning coupled to #849) | R-08 | ADR-003 C: self-contained `apt-get` step, no hard dep on #849; provisioning-step failure fails closed (loud), guarding the quieter runtime-missing→vacuous mode. |
| SR-04 (blocking blast radius beyond tri-state) | R-08 | ADR-003 A/E: only script-exit-2 → non-blocking; everything else fails closed; ARCHITECTURE §5 table is the verified contract. |
| SR-05 (never-green-on-a-tag) | R-10 | ADR-004: AC-11 proves dispatch only; budget one post-merge tag round; diagnostic-capture-first; tag-path INFRA degrades safely. |
| SR-06 (byte-identical / main drift) | R-11 | ADR-004 5 / FR-16: rebase on `main` immediately before AC-11; record branch SHA == `main` HEAD. |
| SR-07 (chronic-INFRA human-vigilance only) | R-15 | ADR-002/ARCHITECTURE §9: stable greppable marker + `::warning::`; automated escalation out of scope; human acceptance (OQ-3). |
| SR-08 (shared-lib edit affects 4 blocking lanes) | R-07 | ADR-002: **additive** new function; `run_smoke_gate` untouched; both truth tables proven via stub seam. |
| SR-09 (dispatch-from-branch GHCR write) | R-12 | ADR-004 6: verify `:latest-amd64` push from branch early; two-step fallback specified. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-05) | 6 scenarios — funnel/load-bearing barrier proof + AC-11 cold GREEN; full exit-code truth table via the real sourced lib with capture-shape invariants |
| High | 6 (R-03, R-06, R-08, R-09, R-10, R-13) | 18 scenarios — bound provenance, runtime-marker anchor, fail-closed table cell-by-cell, pull→INFRA + tag-call-shape, dispatch-only AC-11 + budgeted tag round, cold-path evidence |
| Medium | 7 (R-02, R-04, R-07, R-11, R-12, R-14, R-15) | 14 scenarios — non-substring assertion, timeout→INFRA, sibling truth table + diff, rebase assertion, early GHCR verify, set-e-safe harness with summary witness, marker stability + human acceptance |

## Knowledge Stewardship
- Queried: /uni-knowledge-search for tri-state / false-green / never-green-on-tag / cold-model risk patterns — surfaced #5267 (never-green-on-tag, budget N tag rounds → R-10), #5180 (self-skip must fail not pass → R-05/R-06), #5345/#5192/#4873 (sourceable-lib capture invariants: no-pipe / return-not-exit / set-e re-enable / runtime-marker → R-05/R-06/R-14), #4974 (ceremonial seam, N=1-green-≠-proven → R-01/R-13). All applied as evidence elevating likelihood/severity.
- Stored: nothing novel to store — the recurring patterns (release-gate false-green, never-green-on-tag, ceremonial-seam) are already captured as #5180/#5267/#5345/#4974; this feature instantiates them rather than revealing a new cross-feature pattern. Will revisit at retro if the cold-model-proof-ceremonial (R-13) recurs as a distinct pattern across a 2nd feature.
