# Gate 3b Report: vnc-049

> Gate: 3b (Code Review)
> Date: 2026-09-16
> Result: PASS (re-validated 2026-09-16, iteration 1)

## Re-Validation Outcome (iteration 1) — PASS

The sole prior blocker (Check 7 stewardship, four missing reports) is resolved.
Technical Checks 1–6 were not re-run — no code changed (verified below).

**Check 7 — now PASS.** All four missing agent reports exist with substantive
`## Knowledge Stewardship` blocks (Queried + Stored):
| Component | Report file | Stored |
|-----------|-------------|--------|
| C1 plugin shim | `agents/vnc-049-agent-3-c1-plugin-shim-report.md` | #5755 ✓ |
| C3 JS normalizer mirror | `agents/vnc-049-agent-3-c3-provider-arm-js-report.md` | #5754 ✓ |
| C5 ingest persistence | `agents/vnc-049-agent-3-c5-ingest-persistence-report.md` | #5752 ✓ |
| C6 read path | `agents/vnc-049-agent-3-c6-read-path-report.md` | #5756 ✓ |

Each block has `Queried:` entries (context_search/context_get/briefing before implementing)
and a `Stored:` entry at the expected ID. Stored IDs match the rework claim exactly.

**Documentation-only rework — CONFIRMED.** Commit `0a4bb8ef` `--stat` shows 5 files, all
`.md` (the four agent reports + this gate report), +389/-0 lines, zero code/fixture files.
No `.rs`/`.js` diff vs main changed (the reviewed code is intact). The specific fixture
`packages/unimatrix/test/fixtures/parity/sas-tail-multibyte-window-edge/expected-request.json`
is NOT modified vs main (empty diff). Adjudication item 3 (pre-existing drift) stands.

All 7 checks PASS. Original REWORKABLE FAIL body preserved below for audit.

---


Feature: OpenCode as the fourth observation harness (C18). Branch `feature/vnc-049`,
merge-base `a989e033`, 17 commits, 9 components (C1–C9) + C2b plumbing across 5 waves.

**All technical checks (1–6) PASS. The single blocker is Check 7 (knowledge stewardship):
four implemented components — C1, C3, C5, C6 — have no agent report at all, so no
`## Knowledge Stewardship` block exists for them.** The code itself is production-ready,
correct, tested, and secure; the rework is documentation-only (file the four missing
reports with stewardship blocks).

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | C1–C9 + C2b implement the validated pseudocode; both Gate-3a reconciliation items resolved. |
| 2. Architecture compliance | PASS | ADR-001..009 honored; schema 31→32 with intra-stamp; new-module-thin-wiring (ADR-005). |
| 3. Interface implementation | PASS | Signatures match Integration Surface; fail-open + fail-loud error handling as designed. |
| 4. Test case alignment | PASS | Component test plans mapped; AC-03/AC-06 authoritative non-proxy assertions present + green. |
| 5. Code quality | PASS | Build clean (exit 0); no stubs/TODO/unimplemented!/todo!; no prod `.unwrap()` in new modules. |
| 6. Security | PASS | Boundary validation (model_id charset), parameterized binds, observe-channel spoof rejection; cargo audit advisories all pre-existing (Cargo.lock untouched). |
| 7. Knowledge stewardship | **FAIL** | C1, C3, C5, C6 have NO agent report → no stewardship block. C2/C2b/C4/C7/C8/C9 compliant. |

Modularity (PL-10, NON-BLOCKING): new modules under cap; over-cap files grew via thin wiring only (WARN, expected per ADR-005). Details below.

## Detailed Findings

### 1. Pseudocode fidelity — PASS
- **C4 (wire carriers)**: `model_id: Option<String>` added to `HookInput` (`#[serde(default)]`) and `ImplantEvent` (`#[serde(default, skip_serializing_if="Option::is_none")]`), mirroring `provider`. `is_valid_model_id` + `MODEL_ID_MAX_LEN=128` with the `^[a-z0-9._/-]{1,128}$` carrier charset — resolves the Gate-3a WARN (model_id charset must allow `/`; the narrower source_domain contract would have rejected `ollama/qwen3-coder`).
- **C5 (ingest persistence)**: `derive_source_domain` is opencode-only (returns `Some("opencode")` else `None`); `resolve_model_id` validates via `is_valid_model_id` and drops invalid to `None`+warn; fail-loud `debug_assert` canary present; both `insert_observation` (`:3383`) and `insert_observations_batch` (`:3416`) bind `?11`/`?12` identically. Matches C5 pseudocode exactly.
- **C6 (read path)**: prefer-stored fork (`stored_source_domain` idx 7, `stored_model_id` idx 8), legacy NULL rows fall back to `resolve_source_domain`/`DEFAULT_HOOK_SOURCE_DOMAIN`; `model_id` surfaced on `ObservationRecord`. Both fork branches + mixed-row no-cross-contamination tested.
- **C2 (Rust arm)**: `KNOWN_PROVIDERS` deduped to a module-level const including `"opencode"`; new `hook/opencode.rs` holds canonicalize/normalize/alias-guard (ADR-005 thin wiring: `hook.rs` gains a const + one delegating call).
- **C3 (JS mirror)**: `normalize.js` is a byte-parity mirror — `canonicalizeOpencode`/`normalizeOpencode`/`isOpencodeEventAlias`/`isValidModelId`, `$`-less regex avoiding trailing-newline leniency.
- **C7 (domain pack)**: opencode pack uses `OPENCODE_PACK_SENTINEL_EVENT` (non-empty, non-canonical) — resolves the Gate-3a EC-07 collision concern; loaded zero-config in both `new()` and `with_builtin_claude_code()`.
- **C8 (parity corpus)**: flat arm-level goldens (`parity_corpus_opencode.rs` generator + `opencode-arm-goldens.json` + JS consumer suite) instead of `Case::new` dirs — see adjudication item 2.
- **C9 (installer)**: new `opencode-install.js` module, additive/non-clobbering.

### 2. Architecture compliance — PASS
ADR-001 (persist source_domain at ingest, provider-first, opencode-only, fail-loud) — C5 `derive_source_domain` + C6 prefer-stored fork; pinned ingest derivation site (OQ-4). ADR-002 (model_id carrier + column) — C4/C5/C6. ADR-003 (AC-06 in-cycle) — C5 gating test present. ADR-004 (both arms + parity) — C2+C3+C8. ADR-005 (new-module-thin-wiring) — opencode.rs / opencode-install.js net-new; over-cap files receive wiring only. ADR-006 (non-clobber installer), ADR-007 (observe-channel subagent), ADR-009 (degraded legs) honored. Schema `CURRENT_SCHEMA_VERSION` bumped 31→32 with the intra-stamp-to-31 pattern (#5052) since the v31→v32 block is now last; idempotent multi-column pre-check (#4092) + table-existence guard.

### 3. Interface implementation — PASS
`hook::run(event, provider, model, project_dir)` signature extended; `main.rs` threads the new `--model` CLI arg (C2b). All `ImplantEvent` construction sites in `hook.rs` propagate `input.model_id.clone()`. Read/write `ObservationRow` structs both gain the two fields (OQ-A dual-struct disambiguated correctly). Error handling: fail-open (invalid model_id → `None`+warn at CLI ingress AND ingest bind) and fail-loud (`debug_assert` canary for opencode-with-no-source_domain).

### 4. Test case alignment — PASS
- **AC-03 (R-02)** `test_opencode_event_stored_source_domain_is_opencode` — positive + MANDATORY negative (NOT claude-code), asserted on the SELECTed row via the real `dispatch_request→insert_observation` path. GREEN.
- **AC-06 GATING (R-01)** `test_local_vs_cloud_model_distinct_on_queried_row` + `test_two_local_models_mutually_distinguishable` — distinctness asserted on the QUERIED stored row (via `poll_stored_attribution` SELECT), not in-memory `ImplantEvent`. Anti-tautology `test_opencode_model_id_survives_wire_to_insert` proves sensitivity to a `?12`-bind drop. GREEN.
- **R-05** `test_source_domain_stamp_fires_only_for_opencode` (per-provider write matrix) + `test_derive_source_domain_never_silently_defaults_to_claude_code`. **T-SEC-12/13** (`test_parse_rows_hook_path_always_claude_code`, `test_parse_rows_unknown_event_type_passthrough`) GREEN unchanged (adjudication item 5).
- **R-04** migration cascade: `migration_v31_to_v32` (6 tests: constant bump, fresh-create, ALTER, column parity, idempotent, data-intact) GREEN; `sqlite_parity` (59) GREEN.
- **R-10** serde back-compat (`test_hookinput/implantevent_deserializes_without_model_id`, `skip_serializing_if` byte-stability). **R-15** charset (`test_persist_rejects_model_id_violating_format`).
- **R-06/R-07** C1 plugin: subagent alignment + spoof rejection ("agent value inside tool args does NOT populate extra.agent_type") — 37 plugin tests GREEN.
- Test-run results: store lib 425; migration+parity 6+59; observe domain_pack 47; server lib 4592 pass (2 flakes, see Check 5); JS hook-client opencode arm all GREEN; installer 26; plugin 37.

### 5. Code quality — PASS
- `cargo build --workspace` exit 0.
- No `todo!()`/`unimplemented!()`/`TODO`/`FIXME`/placeholders in production. All `panic!`/`unwrap` hits are in `#[cfg(test)]` fixture assertions (wire.rs). New modules `hook/opencode.rs` and `domain/mod.rs` have no production `.unwrap()`.
- **Two server-lib test flakes** — `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` and `services::usage::usage_tests::test_record_access_mcp_feature_recording` — FAILED under full parallel `--lib` but PASS in isolation. The branch touches neither `eval/` nor `services/usage.rs`. This is the known shared-state parallelism flake class (`.claude/rules/rust-workspace.md`), NOT a vnc-049 regression. Non-blocking.

### 6. Security — PASS
- No hardcoded secrets.
- Input validation at boundaries: `model_id` validated at CLI ingress (`validate_cli_model_id`, C2b) AND at ingest bind (`resolve_model_id`, C5); invalid dropped to `None` before reaching SQL. `source_domain` charset enforced by `DomainPackRegistry::new`.
- All observation INSERTs use parameterized binds (`?11`/`?12`); no string interpolation.
- Path/injection: installer non-clobbering, byte-for-byte preservation, no untrusted-path following (C9 tests GREEN).
- R-07 spoof rejection: validated `session.agent` rides `extra.agent_type` observe channel; conflicting tool-args agent does not override (plugin test GREEN).
- **cargo audit**: 4 vulnerabilities (RUSTSEC-2026-0204 crossbeam-epoch, 2026-0258 h2, 2023-0071 rsa, 2026-0285 rustls) + unmaintained/yanked warnings. **None introduced by this branch** — `Cargo.lock`/`Cargo.toml` are untouched vs main. Pre-existing; out of scope for this gate.

### 7. Knowledge stewardship compliance — FAIL (REWORKABLE)
**Present + compliant** (6): C2, C2b, C4, C7, C8, C9 — each report has a `## Knowledge Stewardship` block with `Queried:` and `Stored:`/"nothing novel" entries. (C9 notes a `context_store` server-timeout on its Stored attempt with the pattern text preserved — acceptable, WARN at most.)

**Missing entirely** (4): **C1 (plugin shim), C3 (JS normalizer mirror), C5 (ingest persistence), C6 (read path)** have NO agent report file in `product/features/vnc-049/agents/`. Impl commits exist (c1 `14968de9`, c3 `261ce634`, c5 `12df44ac`, c6 `6a0add51`), and the C2 report explicitly confirms separate agents owned these boundaries ("Did not touch listener.rs (C5), normalize.js (C3)... per boundary"; references "the C5 agent's in-flight work"). No report ⇒ no `## Knowledge Stewardship` block ⇒ REWORKABLE FAIL per the Gate-3b rule.

This is the strongest form of the gap: it covers the feature's most load-bearing components — C5 is the AC-03/AC-06 attribution mechanism, C1 is the entire plugin shim, C3 the col-022 split-brain mirror, C6 the read path. There is no evidence these agents queried patterns before implementing or evaluated storing lessons.

## Modularity assessment (PL-10 / #693 — NON-BLOCKING)

| File | Status | Note |
|------|--------|------|
| `hook/opencode.rs` (new) | under cap | 180 raw lines incl. tests; well under. |
| `parity_corpus_opencode.rs` (new) | under cap | 128 lines. |
| `opencode-install.js` (new) | under cap | 362 lines. |
| `opencode-plugin/lib/*.js` (new) | under cap | largest ~237 lines (events.js). |
| `hook.rs`, `listener.rs`, `services/observation.rs`, `db.rs`, `migration.rs` | WARN (expected) | Known over-cap; each GREW, but only via ADR-005-sanctioned thin wiring (const entry / delegating call / INSERT-SELECT column pair / struct field / migration block). Verified edits are minimal wiring, not new monolith logic. Per PL-10 done_when(2) these belong to the scheduled-decomposition posture, not a blocking FAIL. |

No NEW module is over cap. No blocking modularity FAIL.

## Adjudicated items (dev-flagged)

1. **C6 core `ObservationRecord` boundary exception** — CONFIRMED mechanical-only. The `model_id: None` at `listener.rs:2408` is the core `ObservationRecord` read-fallback literal (`content_based_attribution_fallback`), NOT the C5 write-path `ObservationRow` struct (`listener.rs:3225`). The ~112 `model_id: None` fixes across crates are field-init completions for a no-`Default` all-required struct. C5's write path (derive/resolve/bind) is untouched by this.
2. **C8 flat arm-goldens** — CONFIRMED sound. The opencode arm is hint-path unreachable via the stdin→request pipeline (JS client has no `--provider`/`--model` flag; plugin shells to the Rust binary, #5754), so `Case::new` dirs would re-test claude-code. The flat golden regenerated from the shipped `hook/opencode.rs` oracle + the JS consumer suite makes the col-022 split-brain drift-fail: mutating the JS arm reddens the consumer suite (dev-verified by mutation), mutating the Rust arm reddens `check-parity-drift.sh` (zero-diff gate) and opencode.rs unit tests. `opencode-arm-goldens.json` is zero-diff.
3. **Pre-existing `check-parity-drift.sh` failure** — CONFIRMED pre-existing, independently. `git log merge-base..HEAD` on `transcript_block.rs`, `parity_corpus_transcripts.rs`, and the `sas-tail-multibyte-window-edge` fixture returns EMPTY — the branch never touched them. The drift (ContextSearch→RecordEvent on the SubagentStart multibyte-window-edge case) is latent at merge-base and surfaces in both the Rust drift script and the JS hook-client suite. OUT OF SCOPE for this gate; route to the SubagentStart transcript-extraction owner.
4. **AC-06 assembled path** — CONFIRMED closed CLI→HookInput→ImplantEvent→INSERT (C2b `main.rs` + `hook.rs` build sites), and C5's `test_local_vs_cloud_model_distinct_on_queried_row` asserts distinctness on the QUERIED stored row (`poll_stored_attribution` SELECT), not in-memory.
5. **R-05 / T-SEC-12/13** — CONFIRMED. Stamp is opencode-only (`derive_source_domain`); `test_parse_rows_hook_path_always_claude_code` and `test_parse_rows_unknown_event_type_passthrough` pass GREEN unchanged.

## Rework Required (REWORKABLE FAIL)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| C1 plugin shim has no agent report | uni-js-dev (C1) | File `agents/vnc-049-agent-3-c1-plugin-shim-report.md` with a `## Knowledge Stewardship` block (`Queried:` evidence of pattern query before implementing; `Stored:`/"nothing novel to store -- {reason}"). |
| C3 JS normalizer mirror has no agent report | uni-js-dev (C3) | File the C3 agent report with a `## Knowledge Stewardship` block. |
| C5 ingest persistence has no agent report | uni-rust-dev (C5) | File the C5 agent report with a `## Knowledge Stewardship` block. |
| C6 read path has no agent report | uni-rust-dev (C6) | File the C6 agent report with a `## Knowledge Stewardship` block. |

Note: this is documentation-only rework. No code change is required — technical Checks 1–6 all PASS. On re-spawn, this gate need only re-verify the four missing stewardship blocks (Validation Iteration Cap applies).

## Knowledge Stewardship
- Stored: nothing novel to store -- the recurring failure mode here (implementing agents shipping code without filing a report/stewardship block) is a process gap already covered by existing gate lessons; no new cross-feature validation pattern surfaced. Feature-specific findings live in this gate report.
