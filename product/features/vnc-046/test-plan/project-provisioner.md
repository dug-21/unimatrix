# Test Plan — project-provisioner (`build_project_server`)

Source: `http_provision.rs`. Constructs the per-slug registry+hold **pair** + pending, sets the
5 config-snapshot fields, threads 3 new params-at-end (`store_config`, `retention_config`,
`signal_class_names`). Risks: R-04 (store_config/inference_config white-box), R-05 (hold/registry
pairing → OOM), R-08 (derive over the wire, not seed).

## Unit Test Expectations

Vehicle: extend the config-parity section already in `project_routing_integration.rs`
(`build_server_with_resolved_config`, `test_per_slug_service_layer_config_parity_*`).

1. **`test_build_project_server_sets_five_config_snapshot_fields`** — build a slug server with a
   non-default resolved config; assert `observation_registry`, `inference_config`, `store_config`,
   `retention_config`, `transcript_signal_class_names` each equal the slug's resolved value and
   are **NOT** the `UnimatrixServer::new` test-default (the #930 silent-default symptom). Mirror
   `main.rs:978-990`.
2. **`test_build_project_server_constructs_registry_hold_pair`** (R-05 — pairing) — assert the
   built server's `session_registry.has_transcript_hold()` is true; the hold is constructed as a
   pair with the registry inside `build_project_server`. Build one with an unpaired hold →
   boot-assertion RED (boot-assertion.md).
3. **`test_pending_entries_analysis_constructed_per_slug`** — the built server's
   `pending_entries_analysis` is a fresh per-slug instance, not a shared global.

## White-Box Wiring-Pins — store_config + inference_config (R-04, documented AC-06 exception)

These two P3 fields have **no clean public observation surface** — they are the SR-05 coverage
hole. They get bidirectional wiring-pin units, **enumerated in the coverage list** (never
silently omitted). NOT value-only — pin to the slug's derived config:

4. **`test_store_config_wiring_pin_bidirectional`** — build slugs A and B with **different**
   byte-limits via `resolve_slug_config`; assert `slug_A.store_config` == A's resolved value AND
   ≠ the `new` default AND ≠ B's; symmetrically for B. Prefer an instance/value pin derived from
   `resolve_slug_config`, not a hardcoded literal (R-08 — derive, don't seed).
5. **`test_inference_config_wiring_pin_bidirectional`** — same shape: A and B declare different
   inference/blend config; each server field == its own resolved config, ≠ default, ≠ the other's.
6. Both pins are named in the suite's coverage-enumeration table (isolation-suite.md) as
   **white-box-only, documented AC-06 exceptions**. Absence of the enumeration entry is a gate
   failure.

## Derive-Over-the-Wire (R-08)

7. INV-C behavioral tests (isolation-suite.md / #800 fixture) drive config through
   `resolve_slug_config` → `build_project_server`, then observe via public surfaces
   (`signal_class_counts`, `status`, purge). No test assigns `server.<config field>` directly —
   seeding produces a believable-but-fake green (#5285).

## Edge Cases
- Slug with **empty** `transcript_signal_class_names` → `signal_class_counts_json` legitimately
  `"{}"`. Distinguish this from the #930 default-fallback symptom so INV-C1 does not false-pass
  or false-fail (test both a slug that declares classes and one that declares empty).
- Slug with **no declared config** → deliberate default fallback (fidelity); a slug that DID
  declare config must NOT fall back (test both declared and not-declared slugs).

## Coverage Trace
| Risk / AC | Test |
|-----------|------|
| R-04 (store_config/inference_config) | #4, #5, #6 (enumeration) |
| R-05 (pairing) | #2 + boot-assertion.md |
| R-08 (derive over wire) | #7 |
| INV-C fidelity | #1, edge cases |
