# Gate 3b Report: vnc-045

> Gate: 3b (Code Review)
> Date: 2026-07-07
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | store primitives, `StoreTagService`, handler, seam fns all match validated pseudocode line-for-line |
| 2. Architecture compliance | PASS | ADR-001/002/004/008/009 followed; direct `entry_tags` write, no `update()`, no schema change |
| 3. Interface implementation | PASS | signatures match brief Function Signatures exactly (`add_tag`/`remove_tag`/`replace_tag`, `StoreTagService::tag`, `context_tag` `#[tool]`) |
| 4. Test-case alignment | PASS | 8 risks covered; R-01/R-02/R-03 High risks each have genuine behavioral tests |
| 5. Code quality | PASS | builds clean; no stubs/TODO/unwrap in new non-test code; new source files under 500 lines; canonical clippy clean |
| 6. Security | PASS (cargo audit WARN) | bound params throughout; LIKE-escape on replace DELETE; input validated; 2 audit CVEs are pre-existing transitive deps (no dep change in vnc-045) |
| 7. Knowledge stewardship | PASS | all 3 impl agents (store, service, handler) have `## Knowledge Stewardship` with Queried + Stored entries |

## Detailed Findings

### 1. Pseudocode fidelity — PASS
- `write.rs` `add_tag`/`remove_tag`/`replace_tag` mirror `store-tag-primitive.md`: single `write_pool.begin()`, `ON CONFLICT DO NOTHING` idempotent add, single-row scoped delete, atomic replace (Steps A–D) with one commit. `like_escape` helper present and ordered (escape char first).
- `store_tag.rs` matches `store-tag-service.md`: `check_write_rate` step 0 → per-action dispatch → session_id-before-spawn → metadata build → fire-and-forget `tokio::spawn`. Serialize-error path returns success and skips the event.
- `tools.rs` handler matches `context-tag-handler.md`: identity → `require_cap(Write)` → action parse → non-empty tag check → `derive_namespace` → `entry_store.get` → `check_tag_lifecycle` → marked seam comment → delegate → format. Both seams are comments only.

### 2. Architecture compliance — PASS
- **replace_tag atomicity (ADR-004):** ONE `txn`; namespace-scoped `DELETE ... LIKE ?2 ESCAPE '\'` + `INSERT ... ON CONFLICT DO NOTHING`; single `txn.commit()`. `?` early-return on failure drops txn → rollback. Returns evicted prior. Empty namespace degrades to pure insert with no over-broad DELETE. No `update()` reuse, no `content_hash`/`previous_hash` touch, no schema/migration. Verified against `write.rs:350-411`.
- **ADR-002:** no invalidation step; all reads live SQL (read-freshness test passes).
- **ADR-009 audit shape:** `operation="context_tag"`, `target_ids=[id]`, `capability_used="write"`, metadata `{action,namespace,tag,prior_value,new_value}` with explicit JSON nulls; `Outcome::Success` and `action.as_str()` serialized as variant strings (#4366); `session_id` captured before `tokio::spawn` (#4388/#4389); serialize error → `warn` + skip, never `"{}"` (#5468). One event per mutation.
- **ADR-008 authorization:** `Capability::Write` only via `require_cap`; `agent_id` audit-only; `TrustLevel` never consulted; no `Capability::Tag`; no add/remove/replace capability split. The two retrofit seams (gate location, value-opacity pre-write) are comments only.

### 3. Interface implementation — PASS
Signatures match the brief exactly. `TagParams` carries `id`(i64 tolerant)/`action`/`tag`/`agent_id`(audit-only)/`format`. `TagAction` and `TagResult` shared `pub(crate)`. `StoreTagService::new` reuses the same store/gateway/audit `Arc`s as `store_ops`, wired into `ServiceLayer`.

### 4. Test-case alignment — PASS
- **R-01 (invariance + read-freshness):** `test_replace_tag_invariance` compares a full pre/post `snapshot`; `test_tag_read_freshness` proves add→present, remove→absent with no invalidation.
- **R-02 (atomic replace):** `test_replace_tag_one_transaction_atomic`, `test_replace_tag_rollback_on_insert_failure` (real `BEFORE INSERT RAISE(ABORT)` trigger forces Step-C failure and asserts prior survives, no zero-tag window), `test_replace_tag_single_value_evicts_prior`, colon-less degrade, no-prior.
- **R-03 (audit):** 12 service-seam tests — prior_value mandatory on remove/replace, null on add, never-sentinel, single-event, variant-string action, session_id-before-spawn, field completeness, namespace derived-not-validated.
- **R-05/R-06 (seams):** 10 module-scope unit tests over `derive_namespace` (boundary table incl. leading/mid/multi-colon) and `check_tag_lifecycle` (quarantined/deprecated/active).
- **R-07:** `test_check_write_rate_throttles_before_write`, `test_uds_session_exempt_from_throttle`, `audit_write_count` includes/excludes/boundary tests.
- **R-08:** `test_replace_tag_like_underscore/percent_namespace_no_over_match`, injection literal-storage.
- **R-04:** `test_value_opaque_acceptance_table`; no validator/config shipped (grep-confirmed).
- Tests green: unimatrix-store 422 passed; unimatrix-server store_tag 17 passed; handler seam unit tests 10 passed.

### 5. Code quality — PASS
- `cargo build --workspace` clean.
- No `todo!()`/`unimplemented!()`/TODO/FIXME/`.unwrap()` in new non-test code (tools.rs unwraps at 1942/6157+ are pre-existing/test).
- New source files under 500 lines: `store_tag.rs` 225, `write.rs` 427, `audit.rs` 479. Test modules `store_tag_tests.rs` 677 / `write_tag_tests.rs` 524 exceed 500 but are test files (rule targets source). `tools.rs` (13651) is a pre-existing monolith the brief explicitly directs extending — not introduced by vnc-045.
- Canonical project clippy gate (`cargo clippy -p unimatrix-server -p unimatrix-store -- -D warnings`) clean.
- No module-wide `#![allow(dead_code)]` in new code.

### 6. Security — PASS (with pre-existing cargo audit WARN)
- All INSERT/DELETE/SELECT use bound params (`?1`/`?2`) — no string interpolation.
- Replace DELETE LIKE-escapes `%`/`_`/`\` via `like_escape` + `ESCAPE '\'`; over-match tests pass.
- Input validated at boundary: non-empty tag, action allow-listed, entry existence + lifecycle checked before write.
- No hardcoded secrets. Blast radius bounded to one entry's `entry_tags` rows (per-slug DB isolation).
- `cargo audit`: 2 vulnerabilities — RUSTSEC-2026-0204 (crossbeam-epoch, transitive) and RUSTSEC-2023-0071 (rsa Marvin, no fix available, transitive). **vnc-045 changed no `Cargo.toml`/`Cargo.lock`** — both are pre-existing transitive-dependency advisories, not introduced by this feature. Does not block; track separately.

### 7. Knowledge stewardship — PASS
`## Knowledge Stewardship` blocks present with Queried + Stored entries:
- store agent — Queried briefing/search (#5599/#5610/#360/#373/#4457); Stored #5612 (trigger-based rollback proof pattern).
- service agent — Queried briefing (#2147/#84/#302); Stored #5613 (audit read-back match-by-content pattern).
- handler agent — Queried search x2 (#4707/#3814/#5609/#5610); Stored entry noted.

## Non-blocking Observations (WARN)

| Item | Nature | Recommendation |
|------|--------|----------------|
| Stale `#[allow(dead_code)]` on `ServiceLayer.store_tag` (`services/mod.rs:250`) | Field is now live (handler calls `self.services.store_tag.tag(...)` in the same PR); the attribute's own comment says "Remove this allow when the handler read lands." Redundant, not module-wide, no compile/clippy impact. | Drop the attribute + comment in a cleanup edit (rust-dev). |
| `cargo audit` 2 CVEs | Pre-existing transitive deps (no dep change in vnc-045). rsa RUSTSEC-2023-0071 has no fix available. | Track outside vnc-045; not a delivery blocker. |
| `cargo clippy --all-targets -D warnings` fails on 2 `manual_repeat_n` lints in `mcp/response/verbosity.rs:192,208` | Pre-existing vnc-044 test code, surfaced by rust-1.95.0's newer lint under `--all-targets`. Canonical gate (no `--all-targets`) is clean. Flagged as adjacent breakage. | Fix in a maintenance pass; not attributable to vnc-045. |

## Rework Required

None (all WARNs are non-blocking; the stale-allow cleanup is optional).
