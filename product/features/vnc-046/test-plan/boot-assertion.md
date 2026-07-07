# Test Plan — boot-assertion (`assert_per_slug_isolation` + field census)

Source: `main.rs`. Real runtime boot assertion returning `Result<(), ServerError>` (aborts boot
loud, NOT a `debug_assert`) + compile-time **exhaustive field census** (no `..`). Closes the
whole "constructor-default never overwritten" class. Risks: R-03 (census false-passes on
threading), R-05 (pairing), R-07 (latent field ships global). AC-08.

## Unit Test Expectations

1. **`test_assert_per_slug_isolation_unwired_registry_returns_err`** (R-03 / AC-08 — Critical) —
   build a slug server with `session_registry` deliberately left as the constructor default;
   assert `assert_per_slug_isolation` returns `Err(ServerError)` and aborts boot. Must be a real
   `Result`, NOT a `debug_assert` (compiled out of release — NFR-2, zero release coverage).
2. **`test_assert_per_slug_isolation_unpaired_hold_returns_err`** (R-05) — build a slug whose
   `session_registry.has_transcript_hold()` is false; assert boot `Err`. Prevents the purge-gate
   split (registry-alone → held buffers never purge → unbounded memory).
3. **`test_assert_per_slug_isolation_unset_config_sentinels_return_err`** (R-04 P3) — where a
   config-snapshot field has a checkable sentinel (still the `new` default), assert the boot
   assertion catches it. Covers `store_config` / `inference_config` at boot as a complement to
   the wiring-pins (project-provisioner.md).
4. **`test_assert_per_slug_isolation_fully_wired_returns_ok`** — a correctly-built slug (all
   handles + config wired) returns `Ok(())`.

## Wiring-Pins (R-03 — closes set-but-not-threaded for handle fields)

5. **`test_registry_for_ptr_eq_slug_server_registry`** — against the **production** resolver
   (not a double): `Arc::ptr_eq(resolver.registry_for(&slug), slug_server.session_registry)`.
   Same for `pending_for`. Proves the resolver hands back the INSTANCE the server holds. These
   live in the behavioral crate's white-box section, NOT the behavioral suite proper (AC-06 —
   the behavioral suite has no `ptr_eq`).

## Field Census (R-07 / R-03 §4 — Critical)

6. **`test_field_census_is_exhaustive_no_rest`** (compile-fail) — the census destructures
   `UnimatrixServer` exhaustively with **no `..`**. Add a throwaway field to `UnimatrixServer`;
   assert the destructure **fails to compile** until the field is classified. Implement as a
   documented compile-fail expectation (e.g. a `trybuild`-style or a commented canary + review
   note) so a new field cannot ship unclassified.
7. **Classification review** — every `UnimatrixServer::new` field is enumerated and classified
   PER-SLUG / CORRECTLY-GLOBAL / CORRECTLY-PER-INSTANCE. PER-SLUG fields route into the boot
   assertion. Spot-check `ExtractionContext` / neural enhancer (#5170 — already a per-slug field
   outside the ADR-003 bundle) and `client_type_map` (correctly-per-instance) are explicitly
   classified, not silently left global.

## The census is necessary but NOT sufficient (R-03 / #5427)

The census + boot assertion are **source-assertions** — they prove a field is *classified and
set*, blind to whether the resolved per-slug handle is actually *used* on the write path. A field
set on the server yet read from a global in `dispatch_request` passes the census and ships green.
The **behavioral back-stop** (isolation-suite.md, INV-T1/T3 through `route_observe` →
`McpAdapter`) is the real enforcement. State this dependency in the coverage-enumeration.

## Failure Modes
- Unwired field at boot → `Err(ServerError)`, boot aborts loud (never silent read-zero).
- New unclassified field → compile error at the census.
- Un-probed isolation direction has NO release coverage from a `debug_assert` — behavioral suite
  covers it (NFR-2).

## Coverage Trace
| Risk / AC | Test |
|-----------|------|
| R-03 | #1, #5 (pin), #6 (census), behavioral back-stop |
| R-05 | #2 |
| R-04 (config sentinels) | #3 |
| R-07 | #6, #7 |
| AC-08 | #1, #5, #6 |
