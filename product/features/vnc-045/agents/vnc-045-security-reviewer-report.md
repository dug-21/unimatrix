# Security Review: vnc-045-security-reviewer

## Risk Level: low

## Summary
`context_tag` is a clean, minimal, value-opaque tag-mutation op. All SQL uses bound parameters; the namespace-scoped `replace` DELETE is LIKE-escaped and double-guarded against over-broad deletes. No `entries`/hash/learning column is touched, `replace` is atomic with correct rollback, no new deps, no secrets, and no deferred `protected_tags` surface ships. No blocking findings.

## Findings

### F1 — SQL injection surface (R-08)
- **Severity**: low (verified safe)
- **Location**: `crates/unimatrix-store/src/write.rs` (`add_tag`, `remove_tag`, `replace_tag`, `like_escape`)
- **Description**: `tag`/`entry_id`/`namespace` all bound as parameters — no interpolation. The `replace` `DELETE ... tag LIKE ?2 ESCAPE '\'` escapes `\`, `%`, `_` (escape char first), so a derived namespace with LIKE metacharacters matches literally and cannot over-match siblings. `replace_tag` additionally guards `namespace.is_empty()` → pure insert, so no unscoped DELETE is reachable (double-guards the handler's leading-colon `":proven"` → `Some("")` path).
- **Recommendation**: none.
- **Blocking**: no

### F2 — Access control / privilege escalation
- **Severity**: low (verified safe)
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs` (`context_tag` step 2)
- **Description**: Gates on `Capability::Write` only; no `Capability::Tag` minted; add/remove/replace not split at capability layer. `agent_id` is audit-only (recorded via `audit_ctx.caller_id`, identical to `store_correct.rs:90`), never an authz input. No privilege beyond `context_correct` (SD-10).
- **Recommendation**: none.
- **Blocking**: no

### F3 — Blast radius / forbidden-surface mutation (R-01)
- **Severity**: low (verified safe)
- **Location**: `write.rs` primitives
- **Description**: Primitives touch only `entry_tags` — never any `entries` column, `content_hash`/`previous_hash`, learning columns, `update()`, or `context_correct`. Hash chain, learning vector, edges, and `id` invariant.
- **Recommendation**: none.
- **Blocking**: no

### F4 — Atomic replace (R-02)
- **Severity**: low (verified safe)
- **Location**: `write.rs::replace_tag`
- **Description**: DELETE(prior) + INSERT(new) in one SQLite transaction; `?` early-return on INSERT failure drops the txn uncommitted → rollback → prior `namespace:*` survives. No zero-value window. FK-cascade race surfaces as clean `StoreError::Database`, no partial write.
- **Recommendation**: none.
- **Blocking**: no

### F5 — Scope hygiene (deferred surface)
- **Severity**: informational
- **Location**: diff-wide
- **Description**: No `ProtectedTagsConfig`, validator, `min_trust_level`, `merge_configs` arm, per-slug threading, or cadence guard shipped. Both retrofit seams are comment-marked only (verified via `git diff | grep`).
- **Recommendation**: none.
- **Blocking**: no

### F6 — Dependencies & secrets
- **Severity**: informational
- **Description**: Zero Cargo.toml/Cargo.lock changes; no new deps; no hardcoded secrets/tokens/keys. No `.unwrap()` in non-test paths (`unwrap_or_default` used).
- **Blocking**: no

## Blast Radius Assessment
Worst case if a subtle bug existed: an over-broad `replace` DELETE could evict sibling tags on the **same entry**. This is prevented three ways — bound params, `ESCAPE '\'` metacharacter escaping via `like_escape`, and the `namespace.is_empty()` pure-insert guard. Reach is bounded to one entry's `entry_tags` rows; per-slug DB (vnc-034) gives structural cross-project isolation, so no cross-entry or cross-project blast. A partial `replace` cannot corrupt state — the single transaction rolls back atomically. No path touches hash-chain or learning columns, so integrity/self-learning corruption is not reachable.

## Regression Risk
Low. Only additive change to shared code is `'context_tag'` appended to the `audit_write_count_since` op-list (a latent, non-enforcing counter). `context_correct`, `validate_outcome_tags`, `update()`, hash, and embed paths are byte-unchanged. `ServiceLayer` reuses existing store/gateway/audit `Arc`s — no new shared state.

## PR Comments
- Posted 1 review comment on PR #929 (state: COMMENTED)
- Blocking findings: no

## Knowledge Stewardship
- Stored: nothing novel to store — the patterns here (LIKE-escape for derived prefixes, bound-param DELETE/INSERT, atomic single-txn replace, audit-only agent_id) are already-known conventions and the diff introduces no new cross-feature security anti-pattern. Feature-specific findings live in the PR comment.
