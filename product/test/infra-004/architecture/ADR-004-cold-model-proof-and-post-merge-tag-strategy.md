## ADR-004: In-Feature Cold-Model Proof (D-2) + Post-Merge Tag Strategy (SR-05)

### Context
Before flipping the lane blocking, the gate must be shown GREEN against a **fresh, cold-model
build of current `main`** (AC-11) — front-loading the cold-model risk into this feature, not a
backlog item. D-2 settles the mechanism: a `workflow_dispatch` run on the feature branch. Because
infra-004 is test-only (no `crates/` change), a feature-branch build is a byte-identical
production image to `main`, so the dispatch run IS a fresh build of `main`'s production code with
the warmup barrier present.

SR-05 (never-green-on-tag, #5267): the exit-2/INFRA branch + the blocking `needs:` edge first run
on a **real tag** only post-merge. The dispatch path resolves `:latest-<arch>` while the tag-push
path resolves `:v<version>-<arch>` (ADR-004 #5184 — divergent resolution). Release-only gates,
reached for the first time on a real tag, fail in sequence (#5267). SR-06: the byte-identical
claim holds only if `main` has not advanced past the branch point. SR-09: dispatch-from-branch may
lack GHCR write for `:latest-<arch>` on a non-default branch.

### Decision
1. **AC-11 proves warmup + verdict + harness, NOT tag resolution.** The dispatch run on the
   feature branch exercises `run_smoke_gate_tristate`, the warmup barrier on the real cold
   first-boot HuggingFace download path, the verdict, and the **entire harness** (checkout,
   setup-node, the new sqlite3 step, GHCR login, `resolve_image` dispatch branch). It does **not**
   exercise the `:v<version>-<arch>` tag-push resolution. Record the run/log reference with the
   feature as AC-11 evidence.
2. **AC-11 GREEN gates the flip.** The blocking flip (Step 4) does not land until the dispatch
   cold-model run is demonstrably GREEN.
3. **Single-PR flip, with a budgeted post-merge tag round.** Land all four steps (incl. the
   `needs:` flip) in one PR — the DoD requires the arc to land in one feature. Explicitly **budget
   one post-merge tag round** as expected cost (#5267), not a regression: do not treat round-2's
   newly-revealed tag-path behavior as a regression of round-1. This is safe because INFRA does
   **not** block (ADR-002/003): a never-green-on-tag failure that surfaces as INFRA (e.g. a
   tag-resolution pull 404) degrades to *visible-vacuous*, not a release block. The **only**
   first-tag path that blocks a healthy release is a harness-step failure — already exercised by
   the AC-11 dispatch run (decision 1). The residual first-tag risk is therefore low and is the
   accepted never-green tax.
4. **Diagnostic-capture-first (#5267 mitigation 4).** `run_smoke_gate_tristate` echoes the full
   smoke log on every path; the warmup barrier and `verdict()` already log last-state on
   timeout/RED. So the first real tag yields a deterministic diagnosis, not a guess.
5. **SR-06 — rebase before the AC-11 run.** The dispatch must build from a feature-branch tip
   == `main` HEAD (rebase immediately before dispatch, or assert branch-point == `main` HEAD at
   run time) so the proof is against current production bytes. (Spec-level constraint.)
6. **SR-09 — verify GHCR write early; keep the two-step fallback.** Confirm the dispatch build
   can push `:latest-amd64` from the feature branch **before** building on D-2(a) — low risk
   (`build-container-x64` already runs on dispatch and pushes `:latest-amd64`; `nan-021` uses this
   path; workflow `permissions: packages: write`). If it cannot, fall back to the specified
   two-step merge (land non-blocking → dispatch to confirm GREEN → follow-up flip PR). A pre-release
   dry-run tag is **rejected** (it risks tripping the real manifest path).

### Consequences
- **Easier:** cold-model risk is discovered in-feature on a path byte-identical to `main`; the
  flip is gated on real evidence; the post-merge tag round is planned, deterministic (capture-first),
  and bounded — and largely *can't* block releases because INFRA degrades safely.
- **Harder:** AC-11 does not (cannot) prove tag-push resolution, so one post-merge tag round is an
  accepted, explicit cost; the byte-identical guarantee imposes a rebase discipline (SR-06); D-2(a)
  carries a residual SR-09 dependency on runner/token config, mitigated by early verification +
  the two-step fallback.
- **Related:** ADR-001 (the warmup bound this proves holds cold); ADR-002/003 (INFRA-never-blocks,
  which makes the single-PR flip safe). Evidence basis: lesson #5267 (never-green-on-tag), ADR-004
  #5184 (dispatch-vs-tag resolution).
