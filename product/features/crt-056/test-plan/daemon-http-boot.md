# Test Plan: Daemon HTTP boot (thread Arcs, collect contexts)

> Component: `crates/unimatrix-server/src/main.rs:1077-1107` (per-slug loop `1084`, call site
> `1085-1092`; daemon's resolved Arcs at `880-898`; pre-crt-056 singletons `957-961` threaded into
> `spawn_background_tick` `968-991`).
> Change (ADR-002 + ADR-004): pass the in-scope config `Arc`s (`Arc::clone` from `880-898`) into
> `build_project_server`; daemon's own path switches to `Some(services)`; **retire** the five global
> singleton handles from the multi-project path; build a `Vec<PerSlugTickContext>` and drive the new
> serial loop.
> Risks: **R-05** (threading), **R-06** (cloud-only branch / same-path), **R-02** (residual global
> handle write path). ACs: **AC-1**, **AC-6** (same-path proof), feeds AC-4. FR-1, FR-7.

This is where the parity *funnel* is wired and where the pre-crt-056 corruption root (the global
singletons handed to `spawn_background_tick`) MUST be removed — not supplemented (#4974 checklist 3).

---

## Unit / structural test expectations

### AC-6 same-path proof (R-06, one isolation seam)
- `test_daemon_and_per_slug_both_use_some_path`
  - **Assert:** the single-project daemon constructs its `UnimatrixServer` via `Some(config-driven
    ServiceLayer)` — the SAME construction path as per-slug servers. There is **no** `if cloud { ...
    } else { ... }` parity branch. The `None` arm is unreachable from the daemon boot.
- **Source audit:** grep `main.rs` boot path for any conditional that builds a different
  `ServiceLayer` shape for cloud vs local. MUST be absent.

### AC-1 propagation — every appended param is the daemon's resolved Arc
- `test_per_slug_call_site_threads_resolved_arcs`
  - **Assert (against the call site `1085-1092`):** each of the 8 appended `build_project_server`
    args is an `Arc::clone` of the daemon's resolved value (`880-898`), not a freshly-synthesized or
    defaulted value. (Couples with AC-1 in `build-project-server.md` — this is the *call-site half*
    of the #2398 propagation boundary.)

### R-02 — global singleton write path retired (verify-the-funnel, paired with AC-wave2-gate)
- **Source audit (part of `wave2-gating-audit.md` Part B):** the five handles extracted at
  `main.rs:957-961` and threaded into `spawn_background_tick` (`968-991`) are **removed** from the
  multi-project path. Grep confirms no surviving global/shared analytics-handle write path beside the
  per-slug `Vec<PerSlugTickContext>`. The per-slug handle set is the **sole** mutation route
  (#4974 checklist 1–3).

### AC-7 (context collection) — one context per registered slug
- `test_boot_collects_one_context_per_slug`
  - **Arrange:** boot with N registered slugs.
  - **Assert:** the boot produces exactly N `PerSlugTickContext`s (one per slug), each holding that
    slug's store + its `ServiceLayer` handle set + its own `TickMetadata`. The set is registry-
    derived (FR-12), not hardcoded.

---

## Edge cases / failure modes

- **N=0 registered slugs:** boot produces an empty `Vec<PerSlugTickContext>`; the serial loop is a
  no-op (covered in `per-slug-tick-loop.md`); boot does not panic.
- **Slug registered after first tick pass:** the iterated set is registry-derived; a later-registered
  slug is picked up on the next pass with no loop-body edit (FR-12; behavioral in
  `multi-slug-harness.md`).
- *Flag adjacent breakage:* removing the `spawn_background_tick` global wiring from the multi-project
  path must not break the single-project daemon's tick — confirm the daemon path still maintains its
  own slug's handles via the new per-slug loop (it is now one of the contexts). Flag any orphaned
  reference to the retired singletons.

## Coverage requirement

AC-1 call-site propagation (all 8 Arcs cloned from resolved config) + AC-6 same-path proof
(`Some` for both daemon and per-slug; no cloud branch) + the R-02 retired-singleton source audit
(paired into AC-wave2-gate, run as the first act of Wave 2).
