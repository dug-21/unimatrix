# Gate 3a Report: vnc-040

> Gate: 3a (Component Design Review)
> Date: 2026-06-19
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | 3 components match §2 decomposition; seam at call-site loop; ADR-004 registry data-only; both model invariants by construction |
| 2. Specification coverage | PASS | FR-01..FR-16 all traced into pseudocode; Cow fallthrough, post-merge validate, instructions thread-through, permissive global-locked all present; no scope additions |
| 3. Risk coverage | PASS | 14 risks / 32 scenarios all owned by a component test plan; R-14, R-01, R-03, R-04, R-12, R-06, R-10 each have concrete expectations |
| 4. Interface consistency | PASS | Shared types in OVERVIEW match component usage; owned-value merge_configs signature handled correctly via global.clone(); no merge rewrite |
| 5. Stewardship compliance | PASS | architect (active) has Stored:; pseudocode (read-only) has Queried:; risk/spec docs carry Queried+Stored blocks |

Minor non-blocking observations (WARN-level, recorded for Gate 3b) listed in Detailed Findings.

## Detailed Findings

### 1. Architecture alignment
**Status**: PASS
**Evidence**:
- Components map 1:1 to ARCHITECTURE §2: `slug_config_classification` (ADR-004 registry, `infra/config.rs`), `resolve_slug_config` (new helper), `per_slug_loop` (`main.rs:1089-1110` MODIFY). Matches the architecture's component table verbatim.
- The overlay seam is at the `build_project_server` call-site loop, NOT in `load_config` — confirmed in `per_slug_loop.md` (modifies `main.rs:1089-1110`) and OVERVIEW ("the overlay seam stays at `build_project_server`'s caller").
- ADR-004 data-only registry: `slug_config_classification.md` declares the const slice + enum + predicate with explicit "Anti-patterns guarded: DO NOT add merge logic here — data-only" and "Does NOT rewrite `merge_configs`". `merge_configs` is REUSED unchanged (OVERVIEW §"Reuse obligations").
- Model invariants by construction (§6a/§6b): `per_slug_loop.md` step (0) clones fields 0–2 unconditionally OUTSIDE any overlay branch, never read from `resolved`; embedding descriptor lock rides inside `merge_configs` global-wins (`inference.embedding_model_sha256`). N=2 `Arc::ptr_eq` proof carried into the test plan.

### 2. Specification coverage
**Status**: PASS
**Evidence**:
- FR-01/FR-02 (overlay resolution, per-key merge) → `resolve_slug_config.md` step 3c (`merge_configs`), step (2) loop derivation.
- FR-04/FR-05/FR-06 (model+pool global, embedding lock, hash-pin global-wins) → `per_slug_loop.md` step (0); classification rows `inference.embedding_model_sha256`/`nli_model_sha256` GlobalLocked.
- FR-07 / Cow + post-merge `validate_config(&merged)` → `resolve_slug_config.md` step 3d explicitly "MANDATORY, after merge, before return".
- FR-08 (byte-for-byte fallthrough) → `Cow::Borrowed(global)` no-merge no-derive arm (step 2 in helper).
- FR-09 (transport never read) → `per_slug_loop.md` AC-06 test; loop derives only the 7 overlayable values.
- FR-11/FR-15 (closed verdict, permissive global-locked) → verdict table in `per_slug_loop.md` with every arg rowed; permissive passed unconditionally.
- FR-14 (instructions thread-through) → relocate `main.rs:687` source into loop, source `resolved.server.instructions`; correctly flags the grep-before-delete obligation for other uses of `server_instructions` (open question #4).
- FR-16 (single canonical classification) → component 1 is the sole owner; verdict tables render from it.
- No scope additions: no new config struct/section/field; `adapt_service` left default (FR-13). Confirmed against the "NOT in Scope" list.

### 3. Risk coverage
**Status**: PASS
**Evidence** (specifically the items called out in the spawn prompt):
- **R-14 (drift-guard + registry exhaustiveness, AC-11)** → `slug_config_classification.md`: `test_classification_drift_guard_every_entry_matches_merge_configs` + `test_classification_registry_exhaustive_vs_validate_config_fields`. Both present and mandatory.
- **R-01 (post-merge cross-field re-validation, AC-08b, Critical)** → `resolve_slug_config.md`: per-invariant merged-only violation tests + the load-bearing negative (`test_per_file_validation_alone_does_not_catch_merged_violation`) + in-helper ordering proof. OVERVIEW §4 mandates exhaustive cross-field enumeration in 3c.
- **R-03 (Arc::ptr_eq fallthrough, AC-02)** → split correctly: `Cow::Borrowed` return in `resolve_slug_config.md`; `Arc::ptr_eq` on the 3 handles in `per_slug_loop.md` (`test_no_file_arm_ptr_eq_on_three_global_handles`).
- **R-04 (N=2 model invariants, AC-04)** → `per_slug_loop.md`: `test_n2_exactly_one_nli_and_one_embed_handle_resident` + unconditional-clone construction proof.
- **R-12 (instructions overlay, AC-10)** → `per_slug_loop.md`: per-slug isolation + absent-file fallthrough tests, both arms.
- **R-06 (VectorConfig::default() forward guard)** → `per_slug_loop.md`: `test_per_slug_vector_index_uses_vectorconfig_default_not_merged_dims`, a standing guard that fails if dims become config-driven.
- **R-10 (DoS/permission hardening)** → `resolve_slug_config.md`: 64 KiB cap before parse + `#[cfg(unix)]` 0o022 rejection, exercised on the per-slug path not assumed.
- All 14 risks claimed by exactly one owning plan (OVERVIEW §2 mapping table); coverage summary 14/32 reconciles with RISK-TEST-STRATEGY.md.

### 4. Interface consistency
**Status**: PASS
**Evidence**:
- `merge_configs` LIVE signature verified against source: `config.rs:3825` reads `fn merge_configs(global: UnimatrixConfig, project: UnimatrixConfig) -> UnimatrixConfig` (OWNED values). The brief/ARCHITECTURE §9 stated a by-reference form; the pseudocode CORRECTLY flags this gap (OVERVIEW §"CRITICAL upstream-signature correction") and resolves it by `merge_configs(global.clone(), slug_file)` — one clone per slug-with-a-file, startup-only. This is NOT a design defect and NOT a merge rewrite; reuse stays intact. Confirmed correct.
- Shared types in OVERVIEW (`Cow<'a, UnimatrixConfig>`, `ProjectSlug`, `ServerError::Config`, the two ADR-004 types) match per-component usage; no contradictions between the three component files.
- Data flow across the helper↔loop boundary is coherent: helper returns `Cow`; loop clones fields 0–2 outside/ahead and derives 3–9+instructions from `&*resolved`.

### 5. Knowledge stewardship compliance
**Status**: PASS
**Evidence**:
- architect (active-storage): `## Knowledge Stewardship` with `Stored:` entries #5197/#5198/#5199 + edges. Compliant.
- pseudocode (read-only): `## Knowledge Stewardship` with `Queried:` entries (#2395/#4655/#5090 + the four ADRs) and explicit "Deviations: none". Compliant.
- risk-strategist + specification: stewardship blocks present in RISK-TEST-STRATEGY.md and SPECIFICATION.md (Queried: + "nothing novel to store -- {reason}"). Compliant.

## Non-Blocking Observations (carry to Gate 3b — not FAILs)

1. **Drift-guard test signature form inconsistency (WARN).** `slug_config_classification.md` pseudocode shows the drift-guard calling `merge_configs(global.clone(), slug.clone())` (owned, correct), but the matching test plan (`test-plan/slug_config_classification.md`) writes `merge_configs(&global, &per_slug)` (ref form). The pseudocode is correct against the live owned-value signature; the test-plan line must use the owned form at impl time. Cosmetic in a plan; flag so rust-dev/tester use owned args.

2. **Open question #1 (AC-08b cross-field enumeration) correctly deferred.** The cross-field invariant set is to be enumerated mechanically from `validate_config` at impl time (OVERVIEW §4 names it a recorded obligation in RISK-COVERAGE-REPORT.md). Appropriately a 3b/3c obligation, not a 3a blocker.

3. **Open question #2 (registry key-string exhaustiveness vs real field paths)** is explicitly carried as carry-item 9 with a binding drift-guard + exhaustiveness test. The literal key strings (e.g. inference weight/PPR names) are placeholders to be reconciled against real struct paths at impl — the drift-guard makes a mismatch fail the build. Appropriately deferred.

4. **Open question #3 (`is_per_slug_overlayable(unknown_key)` contract)** is RESOLVED in pseudocode: conservative `false` (treated as locked), total/panic-free, no `.unwrap()`. Test plan pins it. Good.

5. **Open question #4 (main.rs:687 `server_instructions` other uses)** is explicitly handled: `per_slug_loop.md` instructs rust-dev to grep `server_instructions` usages and relocate ONLY the per-slug fan-out, leaving any daemon-own-ServiceLayer use intact. Good.

6. **Module-home for `resolve_slug_config`** placed in existing `http_provision.rs` (avoids a second agent editing `main.rs` `mod` lines — the swarm-shared-worktree hazard). ≤500-line budget flagged for Gate 3b re-confirmation after the helper lands.

## Rework Required

None.

## Scope Concerns

None. Scope is sound: the design reuses `merge_configs`/`load_single_config`/`validate_config` unchanged, adds no config knobs, confines the change to the call-site loop + a small helper + a data-only registry, and holds both model invariants by construction. The registry is DATA-ONLY (merge_configs not rewritten) — confirmed.
