## ADR-001: Warmup Barrier — Placement, Mechanism (Reuse `write_then_barrier`), and #767-Derived Bound

### Context
The C3/C4 load-bearing writes in `multi-tenant-isolation-smoke.sh` are fire-and-forget under
`synchronous=NORMAL`. If the embedding-model warmup has not completed before them, an own-store
marker can miss the tight C5 read-as-barrier deadline (`READ_DEADLINE_SECS=10`) → classified
INFRA. The gate degrades correctly (never false RED/GREEN), but an INFRA-flapping gate on a
**blocking** lane is the feature's central risk: silently-vacuous enforcement (never RED, never
GREEN, isolation never checked, release ships anyway). AC-01/AC-04 require a bounded warmup
barrier that makes a healthy run — *including the cold first-boot HuggingFace download path* —
deterministically GREEN, while a genuine not-ready state past the deadline stays INFRA (AC-03).

Constraints: the warmup barrier is the **only** permitted gate-script change; **no new readiness
mechanism** (reuse infra-001 idioms); the bound must be **derived from #767's
empirically-validated cold-first-boot window**, not guessed; must stay compatible with the
off-Docker `SMOKE_*_CMD` stub seam (AC-05).

SR-01 warns #767's window was calibrated for an embed round trip (`context_store→context_search`),
not this gate's profile ("model loaded AND both per-slug stores live + registered"). SR-02 warns
the cold HuggingFace download is an external, variable dependency.

### Decision
1. **Placement:** insert the barrier in `main` **after `assert_routes_live` and before
   `run_isolation_matrix`** (between lines 429 and 430), i.e. before any C3/C4 write.
   "Both per-slug stores live + registered" is already established by `assert_routes_live`
   (per-slug dbs exist + all four routes non-404) and registration at C2 — so the barrier's
   distinct job is to establish "embedding model loaded / a store write becomes durable."
2. **Mechanism — reuse `write_then_barrier`, no new primitive:** the barrier issues **one
   throwaway warmup** `write_then_barrier` call (marker `infra003-warmup-${RUN}`, charset
   `[a-z0-9-]`, asserted pairwise non-substring of the four cell markers) on a **longer,
   #767-derived deadline**. PRESENT proves the embed path is warm and a store write is durable
   → proceed to the matrix on the existing tight `READ_DEADLINE_SECS`. Timeout → the existing
   `WTB="INFRA"` path → `infra_fail` (exit 2), **never RED, never GREEN** (AC-03 preserved). The
   warmup row is inert to the matrix: negative cells query specific foreign markers, positive
   cells query their own — none match the warmup marker. Because `write_then_barrier` already
   routes external probes through `SMOKE_WRITE_CMD`/`SMOKE_READ_MARKER_CMD`, the barrier is
   stub-seam compatible for free (AC-05).
3. **Bound (#767-derived, with margin):** introduce `WARMUP_DEADLINE_SECS`, default the **#767
   `READY_TIMEOUT_SECS` value (180s)** — empirically validated by `docker-embed-readiness-smoke.sh`
   which polls *past* the 10s/20s/40s (~70s) embed retry/backoff window under a **real cold
   HuggingFace download** (its preflight requires `huggingface.co` reachable; first boot
   downloads the model). 180s already carries ~2.5× margin over the ~70s backoff floor, which
   covers this gate because its *only* readiness delta over #767 (per-slug store liveness +
   registration) is established **before** the barrier by `assert_routes_live`/C2 — so the
   barrier need only cover model-load, which is exactly #767's measured profile. The deadline is
   env-overridable for arm/slow runners.
4. **SR-02 (external download variance) — classify, do not pre-pull:** download-time variance is
   in-bound up to `WARMUP_DEADLINE_SECS`; beyond it (throttled/unreachable HF, no warmup) → INFRA
   (never a false pass), surfaced visibly by ADR-002's tri-state. A pre-pull/pin is **not**
   added (out of scope — no `crates/`/topology change, no new mechanism). The barrier **logs
   diagnostic context** on timeout (reusing the existing `write_then_barrier` timeout log) so a
   slow-download INFRA is diagnosable (SR-05 diagnostic-capture-first). AC-11's in-feature
   cold-model run demonstrates the bound holds on the real cold path.

### Consequences
- **Easier:** healthy cold-model runs are deterministically GREEN (AC-04), making the blocking
  flip safe; the barrier is ~one function reusing a proven idiom, minimizing gate-script risk and
  honoring "no new mechanism"; stub-seam compatibility is automatic; the bound has documented
  empirical provenance (SR-01 answered: the barrier covers only model-load, which #767 measured).
- **Harder:** the warmup adds up to `WARMUP_DEADLINE_SECS` to a cold run's wall-clock (acceptable
  on a release lane, as #767 already establishes); the gate now writes one extra throwaway row
  (inert, but the warmup marker must be kept non-substring of the four cell markers — a
  runtime assertion guards this); the bound remains hostage to HF availability on release day —
  mitigated to *visible INFRA*, not eliminated (SR-02 residual, accepted).
- **Related:** ADR-002 (tri-state handling that surfaces the barrier's INFRA visibly without
  blocking); ADR-004 (AC-11 cold-model proof that the bound holds). Bound provenance:
  `docker-embed-readiness-smoke.sh` (#767).
