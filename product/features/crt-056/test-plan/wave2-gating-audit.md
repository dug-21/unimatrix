# crt-056 — Wave-2 Gating Audit (A1 per-op + verify-the-funnel)

> **This is a Wave-2-GATING precondition, executed as the FIRST ACT of Wave 2 — before any
> Wave 2 code is written.** It is NOT an end-gate check. (RISK R-01.2 / R-02.2; AC-wave2-gate.)
>
> Rationale (from the brief and #4974): if even one tick op carries a hidden global-handle write
> the probes missed, AC-4 can pass for the clean ops while the missed op corrupts B's state. The
> per-slug funnel can only be proven *sole* once every one of the 9 ops is confirmed
> store-parameterized **before** code is written. The whole "MODERATE not HARD" verdict and the
> per-slug funnel rest on this audit passing first.

Grounding evidence: Unimatrix **#4974** (verify-the-funnel 5-point checklist; vnc-034 `let _store`
ceremonial-seam precedent). This audit is the crt-056 application of that checklist.

---

## How this audit is run and recorded

- **Method:** source review + `Grep` (no behavioral test substitutes for it; it precedes the code).
- **Owner:** Wave 2 implementer, gated by the Stage-3a Gate (this file must exist and be complete)
  and re-confirmed at Gate 3c.
- **Output:** each checklist row below is filled with `PASS` + the source location/evidence, or
  `FAIL` + the offending location. Any FAIL **blocks Wave 2 code** until resolved.
- **Re-run trigger:** if any of the 9 ops or the job `run` path is edited during Wave 2, re-run the
  affected rows before the change lands.

---

## Part A — A1 per-op source audit (the 9 tick ops are store-parameterized)

For each of the 9 tick operations dispatched inside `run_single_tick`
(`crates/unimatrix-server/src/background.rs`, fn def ~`413-804`; call sites at
`463`/`552`/`566`/`629`/`706`/`794`), **source-confirm by closure-check** that the op:

1. takes `&Store` (the per-slug store) as its store input, AND
2. writes **only** the passed-in handle (the one supplied via `PerSlugTickContext`), AND
3. closes over **no** global/`static` analytics handle and **no** store singleton.

A single op reaching a global handle silently re-globalizes the funnel and would let AC-4 pass for
the clean ops while the missed op corrupts B's state (R-02).

> **AUDIT RESULT: PASS (all 9 ops).** Live-source line numbers reconciled to the current file
> (see "Live-source reconciliation" below); call sites match the architecture's expected set.

| # | Op (call site) | Takes `&Store`? | Writes only passed-in handle? | No global/static closure? | Verdict + evidence |
|---|----------------|-----------------|-------------------------------|---------------------------|--------------------|
| 1 | `maintenance_tick` (call `463`; def `814`) | **y** | **y** | **y** | **PASS.** Sig `background.rs:814-826` takes `store: &Arc<Store>` (`822`); writes only the passed-in `effectiveness_state: &EffectivenessStateHandle` (`819`, under write lock `861-943`). Quarantine SQL via passed `store` (`process_auto_quarantine`, `947-955`). No `static`/`OnceLock`/global handle in scope. |
| 2 | co-access promotion (call `552`) — runs AFTER orphaned-edge compaction `513-544`, BEFORE TypedGraph rebuild `562-599` | **y** | **y** (no analytics handle — SQL only) | **y** | **PASS.** `run_co_access_promotion_tick(store: &Store, config: &InferenceConfig, current_tick: u32)` — `co_access_promotion_tick.rs:204-208`. Borrows `&Store` directly; all writes via passed `store.write_pool_server()` (`:100/129/170/252`). No handle param, no module static. |
| 3 | `TypedGraphState::rebuild` (call `566`) | **y** | **y** | **y** | **PASS.** `tokio::spawn(async move { TypedGraphState::rebuild(&store_clone).await })` where `store_clone = Arc::clone(store)` (`563`). Result swapped into the passed-in `typed_graph_state` handle ONLY under write lock (`571-572`). The closure moves a clone of the passed store — closes over no global. |
| 4 | `PhaseFreqTable::rebuild` (call `629`) | **y** | **y** | **y** | **PASS.** `store_clone = Arc::clone(store)` (`622`); `PhaseFreqTable::rebuild(&store_clone, …)` in spawned task. Result swapped into the passed-in `phase_freq_table` handle ONLY under write lock (`636`). Closure captures only the per-call store clone + config scalars. |
| 5 | contradiction scan (call `703-712`, interval-gated `current_tick % CONTRADICTION_SCAN_INTERVAL_TICKS`, `682`) | **y** | **y** | **y** | **PASS.** Entries fetched via passed `store.query_by_status` (`687`); rayon closure captures only per-call `Arc::clone`s (`vi_for_scan` `696`, `adapter_for_scan` `697`). Result written ONLY to passed-in `contradiction_cache` under write lock (`717-720`). No global handle reached. |
| 6 | extraction tick (call `744`; def `1369`) | **y** | **y** (writes via passed store; no analytics handle) | **y** | **PASS.** Sig `1369-1377` takes `store: &Arc<Store>`. `store_clone`/`store_for_rules = Arc::clone(store)` (`1378/1392`); `spawn_blocking` closure captures only those per-call clones. Writes proposals via passed store. No analytics-handle param, no static. |
| 7 | NLI graph inference (call `780`) | **y** | **y** (writes graph_edges via passed store) | **y** | **PASS.** `run_graph_inference_tick(store: &Store, nli_handle: &NliServiceHandle, vector_index: &VectorIndex, rayon_pool: &RayonPool, config: &InferenceConfig)` — `nli_detection_tick.rs:156-162`. All borrowed refs (incl. the shared read-only `nli_handle`); writes via passed `store.write_pool_server()` (`:1112` etc.). No module static handle. |
| 8 | graph enrichment (call `794`) | **y** | **y** (writes graph_edges + COUNTERS via passed store) | **y** | **PASS.** `run_graph_enrichment_tick(store: &Store, config: &InferenceConfig, current_tick: u32)` — `graph_enrichment_tick.rs:60-64`; delegates to `run_s1/s2/s8_tick(store, …)` (`65-67`), each `(store: &Store, …)`. All writes via passed `store.write_pool_server()`. (One `let _ =` at `:395` discards a `set_counter` **Result** — error-handling discard, NOT a resolved per-slug handle; benign — see Part B item 1.) |
| 9 | effectiveness maintenance (the `EffectivenessStateHandle` write within `maintenance_tick`, `maintenance_tick` def `814`, write block `861-943`) | **y** | **y** | **y** | **PASS.** Same op as row 1's handle write, audited as its own analytics surface: the ONLY `effectiveness_state` mutation is under the write lock at `861-943` + `process_auto_quarantine` (`947`), both reached only via the passed-in `effectiveness_state` param + passed `store`. No second/global effectiveness handle exists in scope. |

### Live-source reconciliation (table line numbers vs the as-authored estimates)

The op set and boundaries match the architecture's expected 9 exactly; line numbers drifted from the
plan estimates and are reconciled here against live `crates/unimatrix-server/src/background.rs`:
- `run_single_tick` def is `413-804` (matches). Loop body invoking it: `349-396`.
- `maintenance_tick`: **called** at `463`, **defined** at `814-996` (the plan's "within `maintenance_tick` scope" rows 1/9 are the call+def).
- co-access promotion call `552`; orphaned-edge compaction block `513-544`; `TypedGraphState::rebuild` call `562-599`.
- `PhaseFreqTable::rebuild` call block `621-664`; contradiction scan block `682-739`; extraction `742-774`;
  NLI graph inference `780-787`; graph enrichment `794`.
- The **ORDERING INVARIANT** (compaction → co-access promotion → TypedGraph rebuild) is documented
  in-source at `background.rs:546-551` and is reproduced by registration order when wrapped as jobs.
- **No `static` / `lazy_static` / `OnceLock` / `OnceCell` / `thread_local` analytics-handle declaration
  exists** anywhere in `background.rs` or in the three external op modules (grep-confirmed). There is no
  global handle for any op to close over — the structural precondition ADR-003 relies on holds in live source.

> The op set and exact line ranges are confirmed against live source during the audit; the table
> above lists the architecture's expected set (ARCHITECTURE.md §2, brief Function Signatures). If
> the live op count/boundaries differ, the auditor reconciles the table to source and notes the
> delta — **the audit covers every op actually dispatched, not just these rows.**

**Ordering invariant to preserve when wrapping as jobs** (`background.rs:547-550`): co-access
promotion (`552`) runs AFTER orphaned-edge compaction (`~496-540`) and BEFORE
`TypedGraphState::rebuild` (`566`). Registry order IS the ordering invariant — the audit confirms
the registered job order reproduces this.

---

## Part B — verify-the-funnel source audit (the per-slug handle set is the SOLE mutation route)

Applies #4974's 5-point checklist to the crt-056 job `run` path and the `main.rs` multi-project boot.

> **AUDIT RESULT: PASS (funnel can be made sole; precondition satisfiable).** The global write path
> is a single, fully-enumerated threading site removable in one place; no hidden parallel handle
> writer exists. Items 1–5 are forward obligations on the Wave-2 implementer, none currently blocked.

| # | #4974 checklist item | crt-056 application | Verdict + evidence |
|---|----------------------|---------------------|--------------------|
| 1 | **No discarded resolved handle.** Grep the job `run` path for `let _`, `let _ =`, unused binding that drops a resolved per-slug handle. | A discarded resolved handle ⇒ ceremonial funnel (the vnc-034 `let _store` trap). | **PASS (no offending discard).** The Wave-2 job `run` path does not exist yet (first act of Wave 2). In the CURRENT tick path, the only `let _ =` in the op modules is `graph_enrichment_tick.rs:395` (discards a `set_counter` **Result**, error-handling — NOT a resolved handle). No handle is resolved-then-dropped anywhere in `run_single_tick` (`background.rs:413-804`) or the three external ops. Obligation: the Wave-2 `PerSlugTickContext` builder must not `let _ =` any `*_handle()` result. |
| 2 | **No parallel/global write path beside the seam.** Grep for any global/`static`/shared analytics-handle write path beside `PerSlugTickContext`. | The five pre-crt-056 singletons (`main.rs:957-961`, threaded into `spawn_background_tick` `968-991`) MUST be removed from the multi-project path — not supplemented. | **PASS (single enumerated site, removable).** The five global handles are extracted at **`main.rs:965-969`** (`confidence_state_handle`/`effectiveness_state_handle`/`typed_graph_handle`/`contradiction_cache_handle`/`phase_freq_table_handle`; +8 line drift from the ADR's `957-961`) and threaded into `spawn_background_tick` at **`main.rs:976-1000`**. This is the SOLE global-handle write path — one call site, fully enumerated. Obligation: Wave 2 removes these args from the multi-project path (per ADR-003), it does not add `PerSlugTickContext` beside them. No other global analytics-handle writer exists in source. |
| 3 | **Per-slug handle set is the SOLE route.** Confirm the only mutation route for analytics handles is via `PerSlugTickContext` accessors. | Eliminate the discard AND remove the parallel path entirely (do not merely add the new path beside it). | **PASS (achievable).** Given items 1+2, once the `main.rs:976-1000` global threading is removed and the ops are dispatched over `PerSlugTickContext` (which borrows the per-slug `ServiceLayer`'s `*_handle()` accessors, `services/mod.rs:274-316`), the per-slug handle set is the sole mutation route by construction — Part A proves every op writes only its passed-in handle, so no op can re-globalize. Obligation: remove, don't supplement. |
| 4 | **No trait-default `{ }` bypass.** `BackgroundJob::run` MUST have no trait-default impl that could reintroduce a no-op/bypass. | Mirrors vnc-034 giving `adapter_for` no default impl so a `{ None }` bypass cannot reappear. | **PASS (planned trait is clean).** `BackgroundJob` does not yet exist in source (grep: only a doc-comment mention in `server.rs:409-427`). ADR-004 declares `run` as a bare required method (`async fn run(&self, ctx, shared) -> Result<(), String>;`) with NO trait-default body. Obligation: Wave 2 must implement it exactly so — required method, no `{ }` default — so a no-op bypass cannot reappear. |
| 5 | **Resolve and dispatch tied to the same source.** The handles the tick mutates are the SAME `Arc` instances the slug's `ServiceLayer` accessors return (proved structurally by `Arc::ptr_eq`, AC-5). | Resolution (build) and mutation (tick) cannot diverge from the serving read. | **PASS (mechanism present).** The serving accessors (`*_handle()`, `services/mod.rs:274-316`) return `Arc::clone`s of the `ServiceLayer`-owned handles (ADR-003). Building `PerSlugTickContext` from those same accessors at boot makes build/tick/serve hold the identical `Arc<RwLock<_>>`. Obligation: AC-5 asserts `Arc::ptr_eq` between context handles and serving accessors; no fresh `*StateHandle::new()` on the per-slug path. |

---

## Gate outcome

- **All Part A rows PASS** AND **all Part B rows PASS** ⇒ Wave-2-gate **CLEARED**; Wave 2 code may
  begin.
- **Any FAIL** ⇒ Wave 2 code is **blocked**. Resolve the offending op/path, re-run the affected
  rows, then re-evaluate.

### Outcome (executed 2026-06-19, agent `crt-056-agent-4-wave2-gating-audit`)

**GATE: PASS — Wave 2 may proceed.**

- **Part A (A1 per-op):** all 9 ops PASS. Every op takes the per-slug `&Store`/`&Arc<Store>`
  explicitly, writes ONLY its passed-in analytics handle (under a write lock) or via the passed
  store's pool, and closes over no global/`static` handle or store singleton. No
  `static`/`OnceLock`/`OnceCell`/`thread_local`/`lazy_static` analytics-handle declaration exists in
  `background.rs` or the three external op modules — there is **no global handle for any op to reach.**
  The "MODERATE not HARD" premise (every op cleanly store-parameterized) is **confirmed against live
  source**, not assumed.
- **Part B (verify-the-funnel):** all 5 items PASS. The only global-handle write path is the single
  enumerated `main.rs:965-969` extraction → `spawn_background_tick` `976-1000` threading site,
  removable in one place (ADR-003 mandates removal, not supplementation). No discarded resolved
  handle, no hidden parallel analytics-handle writer, planned `BackgroundJob::run` is a required
  method with no trait-default body. The per-slug handle set **can be made the sole mutation route.**
- **Blockers:** NONE. No op closes over a global handle/store singleton; no global write path would
  survive once the Part B item-2 site is removed as designed.

**Forward obligations carried into Wave 2 code** (verified satisfiable, re-confirmed at Gate 3c):
1. Remove (not supplement) the `main.rs:976-1000` global-handle args from the multi-project path.
2. Build `PerSlugTickContext` from the slug `ServiceLayer`'s `*_handle()` accessors with NO `let _ =`
   discard and NO fresh `*StateHandle::new()`.
3. Declare `BackgroundJob::run` as a required method (no `{ }` default).
4. Preserve registration order to honor the `background.rs:546-551` ordering invariant.

This file is consumed at: Wave 2 start (precondition), Gate 3a (must exist + be complete as a plan),
and Gate 3c (must be filled with PASS evidence). AC-wave2-gate in RISK-COVERAGE-REPORT.md cites this
file.
