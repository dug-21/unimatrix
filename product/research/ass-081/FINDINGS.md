# FINDINGS: Is D6 isolation parity fixable over UDS, or an inherent measurability exception?

**Spike**: ass-081
**Date**: 2026-06-26
**Approach**: investigation (code-anchored) + throwaway two-daemon PoC
**Confidence**: directional, PoC-substantiated

---

## TL;DR

- A **single** in-process UDS daemon is **single-store by construction** — not by fixture
  wiring, but by an enforced architectural invariant (ADR-006 local-binding guard). The
  `feature=` parameter is provably **usage-tracking only, never a store selector**, so the
  current cross-slug probe can never return `false` on one store. PoC reproduces the
  artifact.
- A **genuine second store reachable over UDS is feasible test-only** via a *second daemon
  process* (second `--project-dir` -> second hash -> second store -> second MCP socket). PoC
  proves a marker written to A is **not** visible from B's own store, while B reads its own
  store fine. **No production code change.**
- The **"directly analogous to D5" claim is FALSE.** D5's gap is host-side (Claude-Code
  compaction — undrivable by any harness). D6's gap is harness-internal and drivable
  test-only. D6 is materially **more fixable** than D5.
- **New finding that also challenges #845's premise:** the HTTPS leg does **not** perform a
  live cross-slug read either. It registers **one** slug (`arch-research`) and runs a
  one-sided **directory-confinement** check (no *other* per-slug dir grew), which is
  trivially `visible_to_b=false`. The "real two-slug container" framing is overstated.
- **Disposition: keep #845 as a `bug`** with the second-daemon fix path. It is a real
  harness defect (a guaranteed false signal) AND it is fixable test-only — not a
  D5-style documented exception.

---

## Findings

### Q1: Can the in-process UDS leg host a genuine second slug/store such that a write to slug A is read back from slug B's *own* store — i.e. does the in-process daemon support multi-slug routing, or is it single-store by construction as driven?

**Answer (two-part):**

**(a) A single daemon: NO — single-store by construction, and by an *enforced architectural
invariant*, not merely fixture wiring.**

- The `daemon_server` fixture spawns exactly one daemon with one `--project-dir`
  (`harness/conftest.py:343-344`: `[binary, "--project-dir", str(project_dir), "serve",
  "--foreground"]`) and discovers a single store dir / single MCP socket
  (`_find_daemon_data_dir`, `conftest.py:422-438`).
- The local daemon opens **exactly one** store at boot:
  `unimatrix-server/src/main.rs:714` -> `let store = open_store_with_retry(&paths.db_path)`.
  That `Arc<Store>` is threaded straight into the UDS listener — there is no resolver.
- MCP tool calls over the local socket use that single store directly:
  `unimatrix-server/src/mcp/tools.rs:601` -> `let store_clone = Arc::clone(&self.store)`
  (context_search); `context_store` likewise uses `self.store` with no slug resolution.
- The `feature=` parameter is **usage-tracking only**, never routing
  (`tools.rs:896-937` -> `store.record_feature_entries(...)`). This is the exact #845
  root cause.
- Multi-slug routing is **HTTP-only** and is **forbidden** in the local boot path by a
  compile-time architectural guard (ADR-006): `local_binding_guard_tests.rs:30-48` lists
  `parse_project_key`, `MultiProjectRouter`, `StoreResolver`, `ProjectKey` as
  `FORBIDDEN_IN_LOCAL`. Per-request slug->store resolution exists only on the HTTP path
  (`http/router/seam.rs:199-215` parse + `project_resolver.rs:204-210` resolve).

  So making the *single* local daemon multi-slug is **not a test-infra fix** — it requires
  changing shipped server routing and defeating an architectural guard. That violates the
  nan-022 test-only Hard constraint -> out of bounds.

**(b) A genuine second store reachable over UDS: YES — feasible test-only via a second
daemon process.** A second `--project-dir` yields a second project hash -> a second
`~/.unimatrix/<hash>/unimatrix.db` -> a second `unimatrix-mcp.sock`. The
`compute_project_hash`/`ensure_data_directory` seam
(`unimatrix-engine/src/project.rs:146-186`) makes two project dirs deterministically map to
two independent stores. No production change; reuses the existing daemon spawn machinery.

**Evidence (PoC, throwaway, two real daemons over UDS):**

```
daemon A store=/.../563d52b4ad2c520b   daemon B store=/.../d18aae668a512e72
[ARTIFACT] single daemon A, feature='slug-b' hint -> marker visible? True
[GENUINE ] separate daemon B store     -> marker visible to B?      False
[SANITY  ] daemon B can read its OWN store?                          True
```

- `[ARTIFACT]` reproduces #845 exactly: on one store, a `feature="...-slug-b"`-hinted
  search returns slug A's own marker -> `visible_to_b=true` (a false leak signal).
- `[GENUINE]` shows the second daemon's store does **not** contain A's marker -> a real
  cross-store read returns `false`, the correct isolation result.

**Recommendation:** Treat the question as "can a second *store* be hosted and read over
UDS," not "can the single daemon host a second *slug*." The former is **feasible test-only**
(second daemon); the latter is architecturally precluded and would need production change.
The Hypothesis-constraint assertion "a second slug cannot be hosted in the in-process
fixture" is **correct for one daemon but for a structural reason** (ADR-006 guard), and is
**refuted at the store level** — a second isolated store IS hostable and probe-able over UDS.

---

### Q2: If feasible, what is the smallest faithful change to the UDS fixture that makes the D6 probe genuinely two-store, and does it preserve the nan-022 constraints? Rank the candidates.

**Answer: feasible; ranked below. Recommended = a sibling second-daemon fixture doing a real
cross-store wire read.**

**Approach A1 — second daemon ("daemon B"), genuine cross-store wire read. RECOMMENDED.**
- Extend `daemon_server` into a paired fixture (e.g. `daemon_server_pair`) that spawns a
  second daemon with a distinct `--project-dir` reusing the *same* spawn/poll/teardown
  machinery already in `conftest.py:313-407`. Rewrite `capture_isolation`
  (`parity_legs_capture.py:400-442`) to: write the marker into A's store via A's MCP
  socket, then `context_search(marker)` over **B's** socket; `visible_to_b = bool(b_ids)`;
  `landed_only_in_a` from A's on-disk db (the existing `_marker_present_in_db`).
- Constraint compliance: test-only YES (no server/store-routing change); no production code
  YES; **extends infra-001 cumulatively** YES (a sibling daemon via the existing fixture
  family — no fork, no parallel scaffold); honors per-slug routing as ground truth YES (it
  measures the *same* on-disk store-isolation invariant — see faithfulness note below).
- This is **stronger** than the HTTPS leg's current probe: it performs a real cross-store
  wire READ, where the HTTPS leg only counts dirs (see Out-of-Scope #1).

**Approach A2 — mirror the HTTPS leg's directory-confinement check on the single UDS daemon.
REJECT (vacuous).** Port `capture_isolation_probe` (counts *other* per-slug dirs;
`cloud-bundle-lib.sh:107-140`) to the UDS leg. But the local daemon writes one
project-hash dir and creates **no** slug-name dirs, so `other_count` is structurally always
0 -> `visible_to_b=false` **always**, with no negative control. It would turn the leg green
while proving nothing — a vacuous pass, which the harness's own K4 discipline ("never an
empty/vacuous pass") forbids in spirit. Not recommended.

**Approach A3 — make the single local daemon multi-slug. REJECT (production change).**
Requires HTTP-only routing in the local path and defeating the `local_binding_guard`
(ADR-006). Violates the nan-022 test-only charter -> disqualified.

**Faithfulness note (the Hard "same on-disk property" constraint):** The local daemon has
**no** slug-name routing at all; its project-hash store dir *is* the local equivalent of a
per-slug store dir. So A1 measures the same underlying on-disk invariant the per-slug design
asserts — *two distinct store directories do not bleed across a wire read* — differing only
in how the second store is named (project-hash dir vs slug-name dir). That is the faithful
local analog. The narrow thing A1 does **not** exercise is the HTTP `/v1/<slug>/` **funnel
routing**, which does not exist on the UDS transport by design (see Q3).

**Recommendation:** Adopt **A1**. It is the smallest faithful change that yields a genuine,
non-vacuous D6 measurement on the UDS leg while staying entirely test-side.

---

### Q3: If not feasible (or only via production change), is the correct disposition a human-signed documented UDS-measurability exception for D6 (the D5 pattern), consistent with the per-slug routing architecture (#783 / vnc-034)?

**Answer: A full D5-style documented exception is NOT warranted, because the security
invariant is measurable test-only (Q1b/Q2-A1). One narrow slice is genuinely N/A but does
not require an exception.**

- The **security property** AC-07 actually cares about — "a write to slug A is not visible to
  slug B; it lands only in A" — is reachable over UDS via A1 and must be measured, not
  excepted.
- The only genuinely UDS-N/A slice is the **HTTP per-slug funnel mechanism itself**
  (`parse_project_key` / `/v1/<slug>/mcp` / `MultiProjectRouter`). That routing construct
  does not exist on the UDS transport by architectural design (ADR-006 guard), so
  "the per-slug funnel behaves identically on both transports" is ill-posed for UDS. But this
  is a *routing-mechanism* slice, not the *isolation* property — and the isolation property is
  measurable, so no exception is needed to honor it.
- Consistency with #783 / vnc-034: per-slug store routing is an **HTTP-transport feature**;
  the local daemon is deliberately single-project. Measuring cross-store isolation over UDS
  via two project-hash stores is consistent with that architecture — it tests the same
  store-isolation guarantee through the local transport's native single-project model.

**Recommendation:** Do **not** file a D5-style "host-side undrivable" documented exception for
D6. If the C0 (#5304) flip session wants belt-and-suspenders language, the only honest
carve-out is a one-line note that the *HTTP slug-funnel routing mechanism* has no UDS analog
by design (ADR-006), while the *isolation security property* is measured on both legs.

---

### Q4: Disposition for #845 — keep as a `bug` (with the ranked fix path), or reclassify as a D6/UDS documented exception alongside D5?

**Answer: KEEP #845 as a `bug`.** Two independent reasons:

1. **It is a genuine harness defect, not just a measurability gap.** The current UDS probe
   uses `feature=` as if it routed a store; it provably never does (`tools.rs:896-937`).
   On a single store the probe can **never** return `false` — it is a guaranteed false leak
   signal, i.e. a broken measurement, regardless of any "exception" framing. That must be
   fixed or removed; it cannot be left shipping as a "cross-tenant leak detector" that
   detects nothing real.
2. **It is fixable test-only** (Q1b/Q2-A1, PoC-proven), so it fails the bar for a
   measurability exception. The "Fix: make the UDS isolation probe genuinely two-slug"
   section of #845 is the **correct** disposition; the contradictory "directly analogous to
   the D5 documented gap" framing in the same issue should be **withdrawn**.

**Reject the D5 analogy (Hypothesis-constraint challenge):** D5's unmeasurable component is
**host-side** (the Claude-Code compaction host — undrivable by any test harness, an external
dependency; `parity_legs_capture.py:341-350`). D6's unmeasurable-as-driven component is
**store routing in the harness's own daemon spawn** — drivable test-only with a second
daemon. The two are not analogous; D6 is materially **more fixable** than D5.

**Recommendation:** Keep #845 `bug`. Action it with **Approach A1**. Update the issue body
to (i) drop the "analogous to D5 / documented exception" disposition, and (ii) record that
the fix is a second-daemon cross-store read, test-only. Optionally split off the HTTPS-leg
probe-semantics gap (Out-of-Scope #1) as its own follow-up if true probe-for-probe parity is
wanted.

---

## Unanswered Questions

None of the Goal questions are unanswered. Two bounded items left to the implementing
session (out of this spike's directional scope):

- **Exact fixture shape of A1** (a `daemon_server_pair` fixture vs. parametrizing
  `daemon_server`, and teardown ordering for two daemons). This is fixture-implementation
  detail — explicitly OUT of scope per SCOPE ("a full fixture implementation is OUT of
  scope; that is the eventual fix").
- **Whether the C0 flip bar treats the HTTP slug-funnel routing mechanism as a required D6
  sub-property** (vs. the isolation security property alone). This is a human/product call
  for the #5304 flip session; the framing for either reading is given in Q3.

---

## Out-of-Scope Discoveries

1. **The HTTPS leg's D6 probe is also not a live cross-slug read — it is a one-sided
   directory-confinement check on a *single* registered slug.** The posture smoke registers
   exactly one slug, `arch-research` (`docker-http-posture-smoke.sh:27,419-421`), and
   `capture_isolation_probe` (`cloud-bundle-lib.sh:107-140`) sets
   `visible_to_b=false` whenever **no other** per-slug dir grew (`other_count==0`), which is
   trivially true with one slug. So `visible_to_b=false` on HTTPS is *trivially* true and
   `visible_to_b=true` on UDS is a *feature-hint artifact* — the PARITY_FAIL compares two
   different probe semantics that merely share a key name. **Why it matters:** even after
   fixing UDS with A1, the legs would still measure different things (HTTPS = dir
   confinement; UDS = genuine cross-store read). For true probe-for-probe parity the HTTPS
   leg should also register a slug B and attempt a cross-slug read. This is a larger nan-022
   change, beyond #845's minimal fix — flag as a candidate follow-up spike/issue. *Not
   pursued here.*

2. **`context_search`'s `feature=` parameter is silently non-routing and could mislead other
   tests/users into believing it scopes results.** It only records usage attribution
   (`tools.rs:896-937`). Any test that relies on `feature=` to "scope" a query is at risk of
   the same artifact. Possibly worth a docstring/parameter-doc clarification. *Not pursued
   here.*

---

## Recommendations Summary

- **Q1 (measurability):** Single UDS daemon = single-store by an *enforced* architectural
  invariant (ADR-006 guard; `feature=` never routes). A genuine second store **is** reachable
  over UDS via a **second daemon** (PoC-proven) — feasible test-only.
- **Q2 (fix path):** Adopt **A1 — a sibling second-daemon fixture doing a real cross-store
  wire read** (write via A's socket, read via B's). Reject A2 (vacuous) and A3 (production
  change). A1 is test-only, cumulative on infra-001, and faithful to the on-disk isolation
  invariant.
- **Q3 (exception?):** No D5-style documented exception for D6 — the isolation security
  property is measurable. Only the HTTP slug-funnel *routing mechanism* is UDS-N/A by design,
  and that needs no exception.
- **Q4 (#845 disposition):** **Keep as `bug`**, fix via A1. Withdraw the "analogous to D5 /
  documented exception" framing — D6 is harness-internal and drivable, materially more
  fixable than D5's host-side gap.
