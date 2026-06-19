# Gate 3b Report: vnc-040

> Gate: 3b (Code Review)
> Date: 2026-06-19
> Result: PASS
> Branch: feature/vnc-040 @ fdc5d1ef (HEAD); validated against committed HEAD

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | `resolve_slug_config` matches probe→Borrowed/load→per-file-validate→merge(owned)→post-merge-validate→Owned exactly; loop derivations re-source daemon's own constructor exprs from `r`; ADR-004 registry data-only |
| 2. Architecture compliance | PASS | Overlay at call-site loop (not `load_config`); merge/load/validate REUSED unchanged (only `fn`→`pub fn` visibility); `build_project_server` signature UNCHANGED |
| 3. Interface implementation | PASS | Signatures match pseudocode; `Cow<'a>` fallthrough; `ServerError::Config` fail-loud naming slug; error handling per project pattern |
| 4. Test case alignment | PASS | 34 tests across 3 files cover every test-plan scenario incl. AC-11 drift-guard, AC-08b, AC-04 N=2, AC-10, R-06, R-10 |
| 5. Code quality | PASS | Builds clean; no todo!/unimplemented!/TODO/FIXME; no `.unwrap()` in new non-test code; clippy clean; no NEW source file >500 lines |
| 6. Security | PASS (1 WARN) | Path traversal structurally impossible (slug charset `[a-z0-9-]`); DoS/permission hardening reused+tested; no secrets; pre-existing unrelated `rsa` CVE (WARN, not introduced) |
| 7. Knowledge stewardship | PASS | All 3 rust-dev reports (agents 3/4/5) have `## Knowledge Stewardship` with `Queried:` + `Stored:` (#5211, #5212, #5213) |

## Detailed Findings

### 1. Pseudocode fidelity — PASS
`resolve_slug_config` (`http_provision.rs:308-360`) implements the validated body verbatim:
probe via `fs::metadata(...).map(is_file).unwrap_or(false)` → `Cow::Borrowed(global)` on no-file
(NO merge/re-derivation) → `load_single_config` → `validate_config(&slug_file)` →
`merge_configs(global.clone(), slug_file)` (owned-arg correction applied per pseudocode §3c) →
`validate_config(&merged)` (post-merge) → `Cow::Owned`. The per-slug loop (`main.rs:1089-1153`)
derives the 7 overlayable values from `r = &resolved`, re-using the daemon's OWN constructor
expressions (`resolve_confidence_params`, `CategoryAllowlist::from_categories_with_policy`,
`DomainPackRegistry::new`, `Arc::new(r.inference.clone())`, boosted-categories collect) — parity
by reuse, confirmed against the daemon's own derivations at `main.rs:674/681/878/1848/1862`.
ADR-004 registry is pure data + a total predicate, no merge logic.

### 2. Architecture compliance — PASS
The config.rs diff removes EXACTLY two lines (`fn load_single_config`→`pub fn`,
`fn merge_configs`→`pub fn`); everything else is additive (registry + types + docs). `merge_configs`
/`load_single_config`/`validate_config` bodies are byte-for-byte unchanged — confirmed by
`git diff main..feature/vnc-040` showing no removed body lines. `validate_config` was already `pub`.
Overlay lives at the `build_project_server` call-site loop, never in `load_config`.
`build_project_server` signature is UNCHANGED (only the values the caller derives changed).

### 3. Model invariants held BY CONSTRUCTION — PASS
Fields 0–2 (`embed`/`pool`/`nli`) are `Arc::clone`d UNCONDITIONALLY at `main.rs:1099-1101`,
textually OUTSIDE and AHEAD of the `resolve_slug_config` call, never read from `resolved` on any
path. `permissive` is passed from the global daemon flag unconditionally, never from `resolved`.
Machine-checked: `test_no_file_arm_ptr_eq_on_three_global_handles`,
`test_n2_exactly_one_nli_and_one_embed_handle_resident` (ptr_eq for both slugs), and
`test_fields_0_2_cloned_unconditionally_on_file_present_arm` (ptr_eq holds EVEN with a file
present — proving fields 0–2 never sourced from `resolved`).

### 4. AC-02 no-file fallthrough — PASS (machine-checked)
`test_no_file_arm_ptr_eq_on_three_global_handles` asserts `Arc::ptr_eq` on embed/pool/nli;
`test_resolve_no_file_returns_cow_borrowed_global_no_merge` asserts the returned `Cow::Borrowed`
ref is the SAME address as `&global` (no clone/merge ran); value-equality of overlayable inputs
covered by `test_no_file_arm_overlayable_values_equal_global`.

### 5. Test coverage vs Risk-Based Strategy (14 risks / 32 scenarios) — PASS
All required scenarios present and passing:
- **AC-11 drift-guard** (`test_classification_drift_guard_every_entry_matches_merge_configs`) +
  exhaustiveness closed-set guard (`test_classification_registry_exhaustive_vs_seam_field_set`).
- **AC-08b post-merge re-validation** (Critical, R-01): fusion-weight-sum violation rejected
  post-merge + the load-bearing `#3905` negative proving per-file validation alone is insufficient
  + construction proof the validate runs INSIDE the helper.
- **AC-04 N=2 model invariants**: one-NLI/one-embed at N≥2 with distinct configs.
- **AC-10 instructions**: N=2 per-slug isolation + absent-file global fallthrough.
- **R-06 forward guard**: `test_per_slug_vector_index_uses_vectorconfig_default_not_merged_dims`.
- **R-10 DoS/permission**: oversized (>64 KiB) + world/group-writable rejection on the per-slug path.
- Plus AC-01, AC-03, AC-05 (pin global-wins + warn), AC-06 (transport), R-02 (#4070 inference arm),
  R-08, R-11 (slug-named startup-fatal).

### 6. Compilation / stubs — PASS
`cargo build --workspace` clean (no errors). lib+bin suite: 98 passed, 0 failed (incl. all new
vnc-040 tests). No todo!()/unimplemented!()/TODO/FIXME/panic! in new non-test code. No `.unwrap()`
in new non-test code (the `unwrap_or(false)` metadata probe and `unwrap_or_else` confidence
fallback are total/non-panicking and mirror the daemon's own line 674). clippy lib+bins: no errors.

### Two implementation findings — both assessed SOUND

**Finding #1 (Pattern #5211 — GlobalLocked is two mechanisms): SOUND.** The drift-guard test
correctly splits GlobalLocked into merge-locked (`*_sha256` pins — asserted global-wins in
`merge_configs`) vs construction-locked (`rayon_pool_size`/`tls`/`http`/`permissive` —
`CONSTRUCTION_LOCKED_KEYS`, exempted from the merge-level "global wins" assertion because
`merge_configs` legitimately lets the project value win for these; their lock is carried by the
per-slug LOOP, not the merge). Asserting `merged==global` for them would be a FALSE assertion. The
construction-locked keys ARE actually locked in the loop (verified: never sourced from `resolved`
at `main.rs:1099-1167`; `test_permissive_passed_from_global_flag_never_from_resolved`,
`test_transport_keys_in_per_slug_file_do_not_affect_served_transport`, and the ptr_eq pool tests).
The split keeps the drift-guard HONEST rather than weakening it.

**Finding #2 (Pattern #5213): SOUND, does not weaken AC-05/AC-08b.**
(a) `embedding_model_sha256` global-wins being conditional on the global pin being `Some` is the
correct `.or()` absence semantics: AC-05 asserts a SET global pin wins over a differing per-slug pin
(`test_sha256_pins_global_wins_under_per_slug_pairing` confirms + warn emitted). When global is
`None`, `test_no_global_pin_plus_per_slug_pin_does_not_silently_lock` shows the per-slug pin falls
through but cannot describe a second model — the handle is global by construction (AC-04), so the
descriptor cannot diverge. AC-05 is about the pinned case; unweakened.
(b) post-merge `validate_config` rejecting `boosted_categories ⊄ categories` is exactly the AC-08b
cross-field re-validation guarantee — it STRENGTHENS, not weakens, the post-merge gate.

## Security — PASS (1 advisory WARN)
- **Path traversal**: structurally impossible. `ProjectSlug::try_from` (`http/router/seam.rs:88`)
  restricts the charset to lowercase `[a-z0-9-]`, 1..=63 chars, no `.`/`/`. `base_dir.join(slug)
  .join("config.toml")` cannot escape `{base_dir}/{slug}/`. The architecture claim is verified.
- **DoS / permission**: the reused `load_single_config` 64 KiB cap (#2395) and `#[cfg(unix)]`
  `mode()&0o022` check (`check_permissions`) are EXERCISED on the per-slug path (R-10 tests), not
  assumed.
- **Deserialization**: malformed TOML fails loud via `ServerError::Config` naming the slug; no panic.
- **Secrets**: none in the diff.
- **WARN — cargo audit**: 1 vulnerability (RUSTSEC-2023-0071, `rsa` 0.9.10 via `sqlx-mysql`,
  medium/5.9, no fix available). This is a PRE-EXISTING transitive dependency. vnc-040 changed NO
  Cargo.toml/Cargo.lock and added no dependencies — out of this feature's blast radius. Advisory
  only; not a Gate 3b blocker. (Recommend tracking daemon-wide, not in vnc-040.)

## Knowledge Stewardship — PASS
- Agent 3 (slug_config_classification): `Queried:` (#4070/#4655/#4044/#3771); `Stored:` #5211.
- Agent 4 (resolve_slug_config): `Queried:` (ADR-001/003/002, #3905/#4655/#4070/#5175); `Stored:` #5212.
- Agent 5 (per_slug_loop): `Queried:` (ADR-001/002, #5169/#5212/#5211); `Stored:` #5213.
All three have the `## Knowledge Stewardship` block with both `Queried:` and `Stored:` entries.

## Notes for Stage 3c
- Per the spawn environmental note, `cargo test -p unimatrix-server` building ALL integration-test
  binaries in parallel hit transient linker (`cc`) resource exhaustion (no undefined-reference /
  duplicate-symbol). The lib+bin suite passes (98 passed here; full workspace lib reported 4250+98).
  Stage 3c tester should run integration via the hardened sequential pattern.
- The `rsa` CVE (RUSTSEC-2023-0071) is pre-existing and daemon-wide; not a vnc-040 regression.
