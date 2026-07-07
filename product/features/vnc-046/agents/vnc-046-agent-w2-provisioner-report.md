# Agent Report — vnc-046 Wave 2: project-provisioner

**Agent:** vnc-046-agent-w2-provisioner
**Component:** project-provisioner (`build_project_server`, `http_provision.rs`)
**ADR:** ADR-002 (per-slug construction parity) · **Constraint:** F1/SR-03, WARN-1

## What I did (additive construction — crate stays green)

`build_project_server` now reaches full per-slug construction parity with the daemon
path instead of leaving `UnimatrixServer::new` test-defaults live at runtime.

- **P1 — registry+hold+scanner TRIPLE + pending:** build this slug's `TranscriptHold`
  over its own audit, its `SessionRegistry` PAIRED with the hold + a per-slug
  `SignatureScanner`, and a fresh `PendingEntriesAnalysis`. Set them on the server
  INSIDE `build_project_server`, so they land before the `main.rs` tick loop clones
  them (F1/SR-03 — registry+hold never wired apart).
- **P3 — 5 config-snapshot fields set:** `observation_registry`, `inference_config`,
  `store_config`, `retention_config`, `transcript_signal_class_names`.
- **4 params-at-end** added to `build_project_server`: `store_config`,
  `retention_config`, `signal_class_names`, `signature_scanner` (a missing one is a
  compile error at the call site — crt-056 anti-Defect-1).
- **WARN-1 honored:** the real signature takes resolved config as explicit params, not
  `r`. The per-slug `SignatureScanner` is therefore compiled AT THE CALL SITE
  (`main.rs`, where `r` lives) from `r.transcript_signals.enabled_patterns()`,
  `.map_err(|e| ServerError::Config(e.to_string()))?` (no `From<ScannerError>`), and
  threaded in. No re-`validate` per-slug (`r` is already `resolve_slug_config`'s
  validated output).
- **File-cap split:** the construction block pushed `http_provision.rs` to 529 lines
  (>500). Extracted the cohesive per-slug config-overlay cluster (`resolve_slug_config`
  + helpers + its test module) into `http_provision/slug_config.rs`;
  `resolve_slug_config` is re-exported so `main.rs`'s call path is unchanged.
  `http_provision.rs` is now 348 lines.

## Files modified

- `crates/unimatrix-server/src/http_provision.rs` (M) — P1/P3 construction block, 4
  new params, imports; config-overlay cluster extracted; 348 lines.
- `crates/unimatrix-server/src/http_provision/slug_config.rs` (NEW) — extracted
  `resolve_slug_config` + `config_err`/`warn_locked_keys`/`flatten_present_keys`; 189 lines.
- `crates/unimatrix-server/src/http_provision/slug_config_tests.rs` → moved to
  `http_provision/slug_config/slug_config_tests.rs` (git rename, unchanged content).
- `crates/unimatrix-server/src/http_provision/construction_parity_tests.rs` (NEW) — 3
  unit tests (test plan #1/#2/#3); 200 lines.
- `crates/unimatrix-server/src/http_provision/boot_fallback_tests.rs` (M) — updated
  call site with the 4 new params (defaults + `SignatureScanner::empty()`).
- `crates/unimatrix-server/src/main.rs` (M) — per-slug loop call site: derive
  `slug_store_config`/`slug_retention_config`/`slug_signal_class_names` + compile the
  per-slug scanner from `r`, thread all 4 into `build_project_server`. **Minimal
  param-threading only** (WARN-1).

## Tests

- New (test plan): `test_build_project_server_sets_five_config_snapshot_fields`,
  `test_build_project_server_constructs_registry_hold_pair`,
  `test_pending_entries_analysis_constructed_per_slug` — all pass. Use
  `#[tokio::test(flavor = "multi_thread")]` (#5637).
- `cargo test -p unimatrix-server --lib` → **4512 passed, 0 failed, 1 ignored**.
- `cargo test -p unimatrix-server --bin unimatrix` → **127 passed, 0 failed** (incl. the
  3 new + moved `slug_config_tests` + `boot_fallback_tests`).
- `cargo build -p unimatrix-server` green; `cargo clippy --bin unimatrix` clean.
- Did NOT touch or run the integration `tests/` crate (per spawn instruction).

## Crate builds green: YES

## Issues / flags for later waves

1. **Wave 4 (boot-assertion) owns the rest of `main.rs`.** I made ONLY the minimal
   call-site param-threading + scanner-compile change. NOT added: `assert_per_slug_isolation`,
   `IsolationProbe`, the exhaustive field census, tick-loop reordering. The fields I set
   inside `build_project_server` already land before the existing tick loop (no reorder
   needed for P1), but the boot assertion / `Arc::ptr_eq` convergence guard is still owed.
2. **Documented AC-06 white-box wiring-pins (test plan #4/#5) NOT added here.** The test
   plan homes them in `tests/project_routing_integration.rs` (integration crate, which I
   was told not to touch) via `build_server_with_resolved_config`. My additive white-box
   coverage lives in the binary crate. Tester/Wave owner of the isolation-suite must add
   the bidirectional `store_config`/`inference_config` pins + the coverage-enumeration
   table (isolation-suite.md), and the OQ-2 non-zero `signal_class_counts` behavioral
   regression guard (needs a signal-bearing delta over the #800 fixture).
3. **Pre-existing >500-line test file:** `slug_config/slug_config_tests.rs` is 722 lines.
   I only moved it (git rename, unchanged); it was already over the cap before this wave.
   Not split — out of additive scope; flag for a future cleanup.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` + `context_get(5637)` + `context_get(5635)`
  — surfaced ADR-002 (#5635), ADR-001 (#5630), the multi_thread test-flavor pattern
  (#5637), and the vnc-040 resolve_slug_config lineage (#5209, #5212). Applied all.
- Stored: entry **#5638** "Per-slug fallible config artifacts compile at the
  build_project_server CALL SITE, not inside it" via `context_store` (pattern,
  topic unimatrix-server) — captures the WARN-1 call-site-compile rule, the
  no-`From<ScannerError>` map_err requirement, and the audit-moved-into-`new`
  `Arc::clone` gotcha (compile-invisible until you try to build the hold after
  construction).
