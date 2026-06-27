## ADR-003: Blocking-Flip Blast-Radius Containment + Self-Contained sqlite3 Provisioning

### Context
Step 4 adds the isolation lane to `create-container-manifest.needs:`, making a
**previously-non-existent** lane release-blocking. SCOPE frames blocking only in terms of the
script's tri-state (RED blocks / INFRA doesn't / GREEN passes / SKIP hard-fails). But once the
lane is in `needs:`, **any** job failure — runner outage, `checkout` fail, GHCR login expiry,
image-pull 404, the sqlite3 setup step itself — fails the `needs:` edge and blocks **all**
releases regardless of the isolation verdict (SR-04, the un-modeled release-wide block vector).

SR-03: the gate hard-fails INFRA without `sqlite3` (preflight C1); existing `smoke-*` lanes
provision only `node`. sqlite3 provisioning is coupled to in-flight #849; drift/ordering →
preflight INFRA on a blocking lane → vacuous pass, or a stranded feature.

### Decision
**(A) Only the script's exit-2 maps to non-blocking; everything else fails closed.** ADR-002's
`run_smoke_gate_tristate` maps **only** script-exit-2 → non-blocking-visible. It never makes a
harness-step failure non-blocking (unsafe) and never makes script-INFRA blocking (would block on
noise). All other script exits **and all harness-step failures block** the manifest (fail-closed).
This is intentional and matches the precedent: the four existing blocking lanes already carry the
identical harness exposure (checkout/login/setup-node failing the job blocks the manifest).
Fail-closed on harness breakage preserves the DoD (when we cannot run the check, we do not let the
manifest assemble) and adds no *new class* of exposure — only one extra provisioning step and a
heavier in-container smoke.

**(B) Image-pull failure is script-INFRA → non-blocking-visible (documented divergence).** The
isolation smoke classifies a failed `docker pull` as `infra_fail` (exit 2), unlike
`run_smoke_gate`'s exit-4-blocks. So a missing/404 pushed tag is **non-blocking but visible**,
consistent with "cannot verify → don't block, be loud." The risk that a chronically-wrong tag
makes enforcement vacuous is countered by ADR-004's cold-model proof + budgeted post-merge tag
round + visible INFRA, not by blocking every release on a resolve_image regression.

**(C) Self-contained sqlite3 provisioning — no hard dependency on #849.** The lane provisions
`sqlite3` itself (an explicit `apt-get update && apt-get install -y sqlite3` step, alongside the
existing `setup-node@v4`), so the feature is not stranded on #849's ordering (SR-03). Coordinate
with #849 to avoid duplication, but do not block on it. A provisioning **step** failure
fails-closed (blocks) per (A) — the loud, fixable signal — which is the correct guard against the
quieter failure mode of sqlite3 *missing at runtime* → preflight INFRA → vacuous pass.

**(D) Minimize + pre-exercise the harness.** The lane mirrors the proven `smoke-amd64` harness
(checkout, setup-node, GHCR login) plus the one sqlite3 step and the `resolve_image` + tri-state
invocation — no novel harness mechanism. ADR-004's AC-11 dispatch run exercises this **entire
harness** (including the sqlite3 step and login) on the dispatch path **before** the flip, so a
harness-step break is found pre-flip, not on a release tag.

**(E) SR-04 deliverable — explicit classification table.** ARCHITECTURE §5 enumerates every
failure source → layer → blocks?/rationale. That table is the contract the spec and tester
verify against; it is the explicit containment SR-04 asks for.

### Consequences
- **Easier:** the blocking lane has a fully enumerated, bounded blast radius; fail-closed on
  harness breakage is safe-by-default and precedented; sqlite3 cannot strand the feature on #849;
  harness breakage is caught pre-flip by AC-11.
- **Harder:** the lane *can* block all releases on infra flakiness (GHCR login blip, apt
  hiccup) — accepted as the same exposure the four existing blocking lanes already carry, and the
  signed cost of fail-closed enforcement; a wrong pushed tag degrades to *vacuous* (non-blocking)
  rather than loud-blocking — the deliberate INFRA-never-blocks trade (B), bounded by ADR-004.
- **Related:** ADR-002 (the tri-state mapping this constrains, incl. pull→INFRA); ADR-004
  (pre-flip harness exercise + post-merge tag round). Coordinate: #849 (sqlite3).
