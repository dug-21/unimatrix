## ADR-002: Exit-2/INFRA Tri-State Handling — Additive Function in Shared `release-gate-lib.sh`

### Context
For the isolation lane to block on RED **without** blocking on INFRA, CI must discriminate the
gate's exit 2 (INFRA) from exit 1 (RED). The existing `run_smoke_gate` (release-gate-lib.sh:44)
has cases for exit 0/3/4/1/* but **no case for exit 2** — the isolation gate's INFRA code falls
into `*) → return 1`, collapsing INFRA into a generic failure. On a blocking lane that would
block releases on warmup/dependency noise. D-1 settles that the exit-2 branch lands in the
**shared** `release-gate-lib.sh` (single source of truth, sourced by both CI and the off-Docker
stub test) and must emit a **distinct, greppable marker + `::warning::`** and return success
(non-failing), not a silent return (mitigation (c) / N4).

SR-08 warns: `release-gate-lib.sh` is sourced by all four **existing blocking** lanes
(`smoke-amd64/arm64`, `embed-amd64/arm64`) plus the stub test. Mutating `run_smoke_gate`'s
behavior on exit 2 would change semantics for every sibling lane — a latent footgun if a future
sibling smoke ever emits 2.

### Decision
**Add a new, additive function** `run_smoke_gate_tristate IMAGE CMD…` to `release-gate-lib.sh`;
**do not modify `run_smoke_gate`.** Only the isolation lane calls the new function. Behavior:

- Capture exactly as the proven spine does — `set +e; out="$(IMAGE="$image" "$@" 2>&1)"; rc=$?;
  set -e; echo "$out"` — **no pipe** between the smoke and `$?` (the #4873/R-02 swallow class),
  and `return`, never `exit` (keeps it unit-testable when sourced).
- Exit-code map: `0` → fall through to marker check; `1` → `::error::` RED + `return 1` (blocks);
  `2` → emit `::warning::` + a **stable greppable marker** (e.g.
  `[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN`) + `return 0` (non-blocking, visible);
  `3` → `::error::` mis-provisioned Docker-present lane + `return 1` (hard fail); `*` → `::error::`
  + `return 1`.
- Marker check (only after rc 0): `echo "$out" | grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES
  PASSED.*'` else `::error::` early-exit-0 + `return 1`. GREEN iff `rc==0 AND marker`.

Rationale for a new function over a flag on `run_smoke_gate`: a signature/flag change touches the
proven runner used by four blocking lanes (SR-08); a separate function is **purely additive** —
the four siblings keep byte-identical behavior and zero exit-2 exposure. The ~6-line capture
duplication is the deliberate cost of SR-08 containment; both functions' truth tables are proven
by the stub test so drift cannot silently pass.

The image-pull failure is **not** given an exit-4-blocks case here: the isolation smoke classifies
a pull failure as `infra_fail` (exit 2, not 4 — see setup_container), so a missing pushed tag maps
to non-blocking-visible INFRA, consistent with "cannot verify → don't block, be loud." This is a
deliberate divergence from `run_smoke_gate`'s exit-4-blocks and is documented in ADR-003's
blast-radius table.

The full truth table — (0+marker)→0, (0,no-marker)→1, (1)→1, (2)→0 **with** warning+marker
emitted, (3)→1, (other)→1 — is proven **pre-merge** by sourcing the real lib against a stub smoke
(the #5192/#5258 pattern), no Docker/tag required.

### Consequences
- **Easier:** the lane blocks on RED while INFRA stays visible-but-non-blocking; the four existing
  blocking lanes are provably unaffected (SR-08 contained — additive, no shared-codepath edit);
  any future tri-state gate can reuse `run_smoke_gate_tristate`; the exit-2 policy is single-source
  and unit-tested, not inline YAML.
- **Harder:** ~6 lines of capture logic exist in two functions (mitigated: both stub-tested for
  the no-pipe/return invariants); INFRA being non-blocking means a chronically-INFRA gate is
  silently-vacuous-if-unwatched — addressed only to *visible* (SR-07 residual, ADR-003/§9).
- **Related:** ADR-001 (the warmup INFRA this surfaces); ADR-003 (blast-radius classification incl.
  the pull→INFRA divergence); must reuse the #5192 capture/return/marker invariants.
