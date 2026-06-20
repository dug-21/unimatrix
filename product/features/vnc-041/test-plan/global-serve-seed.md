# C4 — Global Serve-Time Seed Test Plan (inside `if config.http.enabled`)

> File: `main.rs` (`tokio_main_daemon`). ADR-004 (#5238). Risks: **R-01 (Critical)**,
> **R-02 (Critical)**, R-03 (end-to-end half), R-10, R-11. ACs: **AC-01** (container serve writes (a),
> gate is `http.enabled` not `base_dir`), **AC-06** (empirical zero-files sentinel + negative control),
> AC-05 (no-clobber on (a)). Tests extend `main_tests.rs` / `per_slug_loop_tests.rs` patterns.

## What this component is

Inside `tokio_main_daemon`'s `if config.http.enabled` block, before the per-slug loop:
```
write_default_config_if_absent(&paths.data_dir.join("config.toml"), /* force = */ false)
```
The `else` branch (local STDIO/UDS) has **NO seed call site**. "Container only" is a compile-time
branch fact, NOT a runtime flag and NOT keyed on `base_dir` (which is `None` on every live serve call).

## The two correction-driven proofs (do NOT skip — #4876 empirical-gate-integrity)

### R-01 / AC-01 — seed fires on the http.enabled path WITH `base_dir = None`
This is the load-bearing proof of the ADR-004 correction: the gate is `http.enabled`, NOT `base_dir`.
- `test_serve_seed_fires_with_http_enabled_and_base_dir_none`
  Arrange: empty temp data dir; the conditions of an http-enabled serve where `base_dir = None` (the live
  serve value — main.rs passes None at 599/1347/1779/529/546). Act: drive the seed call site (smallest
  reachable seam — see Harness note). Assert: (a) exists at `paths.data_dir.join("config.toml")` with
  `DEFAULT_CONFIG_TOML` knobs (parse it → equals compiled defaults). The fact that `base_dir = None` and
  the seed STILL fires is the assertion that the gate is `http.enabled`. (R-01 scenario 2 + 4.)
- `test_serve_seed_second_boot_does_not_overwrite`
  After the first seed, capture content + mtime. Act: run the seed again. Assert: content/mtime unchanged
  (skip-if-exists; AC-01 "subsequent boot does not overwrite"; couples R-11).

### R-01 / R-02 / AC-06 — EMPIRICAL zero-files sentinel + MANDATORY negative control
The single forcing function for SR-04. Count files, do not reason about the branch.
- `test_local_serve_writes_zero_new_config_files` (the sentinel)
  Arrange: `config.http.enabled == false` (local/STDIO); an empty home `.unimatrix` tree (temp). Count
  files in the tree BEFORE. Act: drive the local serve seam (the `else` branch). Count files AFTER.
  Assert: **delta == 0** — no new config file at the path-hash path, no per-slug dir, nowhere. (R-01
  scenario 1, R-02 scenario 1, AC-06.)
- `test_container_serve_writes_one_config_file_negative_control` (MANDATORY negative control)
  SAME sentinel harness on the `http.enabled == true` path with an empty data dir. Assert: **delta > 0**
  (the global (a) appears). This proves the sentinel actually detects writes and is not trivially passing
  (R-02 scenario 3, AC-06 negative control). Without this, the zero-files assertion is worthless.
- `test_local_serve_resolution_behavior_matches_pre_vnc041_baseline`
  Arrange: capture the local config-load + resolution result with the seed path disabled (or a captured
  pre-vnc-041 baseline). Act: run the local path with vnc-041 present. Assert: byte-for-byte / value-for-
  value identical — the local majority is unperturbed (R-02 scenario 2, AC-06 second half).

### Structural placement assertion (R-01 scenario 3 — 3c structural check)
- The seed call is lexically INSIDE the `if config.http.enabled` block; the `else` branch contains no
  seed call site. Verified structurally at Stage 3c (read the branch / SR-08 ripple audit confirms
  placement). Document as a code-shape invariant — the empirical sentinel above is the runtime proof, the
  structural check is the construction proof. BOTH are required (#4876: structure alone is insufficient).

## R-03 / AC-05 — no-clobber on (a) (operator-edited global survives)
- `test_container_serve_does_not_clobber_operator_edited_a`
  Arrange: pre-place `"# operator-edited global config\n"` at the (a) path. Act: run the http-enabled
  seed. Assert: (a) byte-for-byte unchanged (skip-if-exists). AC-05 / R-03 scenario 1.

## R-11 — dual (a) writers (`handle_version` + serve) are idempotent
- `test_init_then_container_serve_a_written_once`
  Act: simulate `handle_version`/init writing (a), then run the serve seed. Assert: (a) is the
  init-written file, serve no-ops (skip-if-exists). (R-11 scenario 1.)
- `test_serve_seed_then_version_second_caller_noops`
  Act: serve seeds (a) first, then a later `version`/`handle_version` runs. Assert: same file, second
  caller no-ops; whichever runs first wins (ADR-004; `create_new` makes order irrelevant). (R-11 scenario 2.)

## R-10 — best-effort: (a) seed failure does not abort serve startup
- `test_container_serve_seed_failure_does_not_abort_startup`
  Arrange: make the data dir non-writable (chmod `0o555`). Act: run the http-enabled seed. Assert: no
  panic, daemon startup proceeds; serve tolerates the absent (a) (loads from defaults). (R-10 scenario 2.)

## Harness note (delivery decision, flagged for 3a→3b)

The seed call is one line inside `tokio_main_daemon`'s `http.enabled` block. Preferred test depth, in
order:
1. **Function-level** — call `write_default_config_if_absent(&data_dir.join("config.toml"), false)`
   directly against a temp data dir, under each branch's *conditions* (the function is already the seed;
   the branch just gates whether it's called). This makes the file-count sentinel + base_dir=None proof
   cheap and deterministic. The structural placement check (above) confirms the gate wiring separately.
2. **Seam-level** — if a small helper wraps the seed call, test that helper with `http.enabled` true/false.
3. **Full daemon boot** — only if neither is reachable; boots `serve` with a minimal http-enabled vs
   local config under a temp HOME and counts files. Heavier and slower; use as a last resort.
The **file-count delta assertion (== 0 local, > 0 container) is MANDATORY regardless of chosen depth.**

## Coverage requirement (RISK-TEST-STRATEGY R-01, R-02)

Both `http.enabled` branches exercised with empty config dirs; local branch proven to write ZERO files
empirically (delta == 0), container branch shows delta > 0 (negative control); seed fires with
`base_dir = None` (gate is `http.enabled`); (a) no-clobber; dual-writer idempotency; failure swallowed.
