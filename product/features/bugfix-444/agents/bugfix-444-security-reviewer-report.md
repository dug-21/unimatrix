# Security Review: bugfix-444-security-reviewer

## Risk Level: low

## Summary

PR #446 adds four targeted maintenance-tick passes (prune, heal, graph filter, metric) plus a restore path re-insertion to enforce the VECTOR_MAP/HNSW/active-entry invariant broken by GH #444. All SQL queries use parameterized placeholders with no user-controlled interpolation. No new external inputs, no new deserialization surface, no secrets introduced, no new dependencies. One finding is flagged: `heal_pass_batch_size` documents a valid range of `[1, 1000]` but the `validate()` function does not enforce it — a user who sets `heal_pass_batch_size = 0` will hit a `LIMIT 0` SQL clause that silently disables the heal pass, contrary to the documented semantics. This is not exploitable but is an operator misconfiguration trap. It is non-blocking because the failure mode is benign (heal pass skips silently) and the default is safe.

## Findings

### F-1: `heal_pass_batch_size` lacks range enforcement in `validate()`

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/config.rs` lines 408–410, 702–972
- **Description**: The field comment documents "Valid range: [1, 1000]" but `InferenceConfig::validate()` contains checks for all other bounded fields (e.g., `max_graph_inference_per_tick` [1, 1000], `graph_inference_k` [1, 100], `query_log_lookback_days` [1, 3650]) and conspicuously omits `heal_pass_batch_size`. If an operator writes `heal_pass_batch_size = 0` in their config TOML, the value passes validation, `heal_batch as i64` binds `0` to `LIMIT ?1`, and the SQL returns zero rows — silently disabling the heal pass. An operator who then reports missing embeddings after an adapter outage will have no diagnostic signal that the config was the cause.
- **Recommendation**: Add a validation check inside `InferenceConfig::validate()` consistent with the documented range: reject values of `0` (minimum must be `>= 1`) and optionally cap at `1000`. A single bounds check after the `ppr_max_expand` check on line ~963 is sufficient.
- **Blocking**: no

### F-2: Status discriminants hardcoded as integer literals in new SQL queries

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/status.rs` lines 699, 937, 986
- **Description**: The new SQL queries use `status = 0` (Active) and `status = 3` (Quarantined) as bare integer literals. The `Status` enum in `crates/unimatrix-store/src/schema.rs` carries `#[repr(u8)]` with stable discriminants (`Active = 0`, `Quarantined = 3`), so there is no current correctness risk. However, the existing codebase pattern elsewhere binds via `.bind(Status::Active as u8 as i64)` (e.g., `read.rs:1168`, `write.rs:179`) to make the relationship explicit and catch future enum changes at compile time. The new queries diverge from this pattern.
- **Recommendation**: Replace bare integer literals with bound parameters or constants where feasible. This is a code hygiene observation, not a security vulnerability — the status values are not user-controlled.
- **Blocking**: no

### F-3: `remove_entry()` does not call `stale_count.fetch_add`

- **Severity**: low
- **Location**: `crates/unimatrix-vector/src/index.rs` lines 439–444
- **Description**: The new `remove_entry()` method removes the entry from the `IdMap` but the HNSW point remains in the hnsw_rs graph with no IdMap entry pointing to it. The HNSW graph's internal stale-point counter (used by `stale_count()` and the compaction trigger threshold) is NOT incremented by the IdMap removal alone — it relies on the `next_data_id` counter and `IdMap` size delta via `stale_count()` which computes `next_data_id - id_map.len()`. Testing (T-444-remove-03: `test_remove_entry_increments_stale_count`) asserts that `stale_count()` increases by 1 after `remove_entry()`, which is satisfied by the computation. This is verified correct. No security issue.
- **Recommendation**: No action required — documented for completeness. The stale-count formula works correctly through the IdMap size change.
- **Blocking**: no

## No Findings (Confirmed Clean)

| Check | Status |
|-------|--------|
| SQL injection | CLEAN — all new SQL uses parameterized placeholders (`?1`, `?2`); no user-controlled string interpolation |
| Path traversal | N/A — no new file path operations |
| Command injection | N/A — no new shell or process invocations |
| Hardcoded secrets | CLEAN — no credentials, API keys, or tokens |
| New dependencies | CLEAN — no new Cargo dependencies introduced |
| Unsafe code | CLEAN — `#![forbid(unsafe_code)]` confirmed; no `unsafe` keyword in any changed file |
| Deserialization of untrusted data | CLEAN — `heal_pass_batch_size` is deserialized from operator config (TOML), not from MCP tool input |
| Privilege / access control bypass | CLEAN — `restore_with_audit` is only called from paths that already passed the existing audit check |
| Error leakage | CLEAN — all failure paths log via `tracing::warn!` with structured fields; no internal state returned to callers |
| Panic paths | CLEAN — all RwLock acquisitions use `.unwrap_or_else(|e| e.into_inner())` (poison recovery), consistent with existing codebase pattern |
| Integer overflow | CLEAN — `heal_batch as i64` binds a `usize` to SQLite i64; max `usize` on 64-bit exceeds i64::MAX but the documented cap is 1000, well within range. The missing validation (F-1) is the relevant concern, not overflow |
| Information disclosure via `unembedded_active_count` field | ASSESSED — the new status report field exposes a count, not content. This is consistent with all other existing diagnostic count fields and does not disclose entry data |

## Blast Radius Assessment

Worst case scenario: the prune pass deletes a VECTOR_MAP row for an entry that is concurrently being re-inserted (race between a store operation and the background tick). The result would be the entry's vector absent from the index until the next heal pass re-embeds it. The failure mode is: reduced search recall for one entry for one maintenance tick interval. No data loss, no corruption, no privilege escalation. The heal pass's idempotency design (write `embedding_dim` last) contains the blast radius to temporary search degradation.

Second worst case: `heal_pass_batch_size = 0` disables the heal pass silently (see F-1). An operator experiencing an embed-adapter outage recovers manually but the config trap prevents automatic recovery. No security impact; operational nuisance only.

## Regression Risk

**Low.** The changes are confined to:
- Background maintenance tick (not on the request-handling hot path)
- `restore_with_audit` (best-effort re-insertion; falls back gracefully to heal pass)
- `TypedGraphState::rebuild` filter (additive filter — only quarantined entries are excluded, and these were already semantically incorrect to include in PPR)
- New store methods (`delete_vector_mapping`, `update_embedding_dim`) — both are write-pool operations consistent with existing patterns
- New `StatusReport` field — `serde` will default-deserialize to `0` for any clients not yet sending it; non-breaking

The tick ordering (prune → heal → compact) is an additive constraint on the existing tick sequence, not a restructuring of it.

Existing test count: 3951 passed, 0 failed (gate report). 10 new bug-specific tests directly exercise each new code path.

## PR Comments

- Posted 1 comment on PR #446 with findings summary
- Blocking findings: no

## Knowledge Stewardship

- Stored: nothing novel to store -- F-1 (missing validate() for new config field) is a recurring pattern documented by procedure entry #3759 "How to add new fields to InferenceConfig". A future lesson could note "always add validate() bounds when the procedure says 'valid range'", but this is too marginal and the existing procedure already implies it. No new generalizable anti-pattern emerged beyond what #3759 captures.
