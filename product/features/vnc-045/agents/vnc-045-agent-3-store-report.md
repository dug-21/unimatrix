# Agent Report — vnc-045-agent-3-store (Wave 1, unimatrix-store)

## Scope
Direct tag-write primitives + audit op-list update (both `unimatrix-store`).

## Files Modified
- `crates/unimatrix-store/src/write.rs` — new `add_tag` / `remove_tag` / `replace_tag` primitives + module-private `like_escape` helper; `#[path]` test module wired.
- `crates/unimatrix-store/src/audit.rs` — added `'context_tag'` to the `audit_write_count_since` op-list and updated the doc-comment.
- `crates/unimatrix-store/src/write_tag_tests.rs` — NEW, primitive test suite (`#[path]` module of `write`).
- `crates/unimatrix-store/src/audit_count_tests.rs` — NEW, op-list test suite (`#[path]` module of `audit`).

## Implementation Notes
- Each primitive opens `self.write_pool.begin()`, writes `entry_tags` DIRECTLY via single-row INSERT/DELETE, commits once. No `entries` column touched — no `content_hash`/`previous_hash`, no learning columns, no `update()`, no `context_correct` (ADR-001, R-01).
- `add_tag` / replace's INSERT use `ON CONFLICT(entry_id, tag) DO NOTHING` (idempotent). `remove_tag` is a single-row `DELETE ... WHERE entry_id=?1 AND tag=?2` (absent = no-op).
- `replace_tag` (ADR-004, R-02): namespace-scoped `SELECT prior` + `DELETE` + `INSERT new` in ONE transaction; commit once. A failing INSERT → `?` early-return → txn dropped → full rollback (prior survives, no zero-`namespace:*` window). Returns evicted prior as `Option<String>`.
- R-08: DELETE/SELECT use `LIKE '<ns>:%' ESCAPE '\'` with the namespace passed through `like_escape` (`\`→`\\`, `%`→`\%`, `_`→`\_`). Empty namespace (colon-less degrade) skips DELETE entirely → pure insert, never an over-broad delete. All statements use bound params.
- Audit op-list: latent, non-enforcing counter only; no throttle path wired.

## Contract deviation flagged (non-blocking)
`remove_tag` implemented as `-> Result<()>` per the BRIEF "Function Signatures (use exactly; do not invent)" and the spawn prompt, NOT the pseudocode's `-> Result<Option<String>>`. Rationale: the service records `prior_value` from its own `tag` argument (the client named it exactly; absent-remove still records intent per FR-10), so the return value is unused downstream. Wave 2 (`StoreTagService`) can set `prior_value = tag` without the return. If Wave 2 expects `Option<String>`, this is a trivial widen.

## Tests
- New: 17 primitive tests (`write_tag_tests`) + 3 op-list tests (`audit_count_tests`) = 20, all passing.
- Full `cargo test -p unimatrix-store --lib`: 422 passed / 0 failed.
- Coverage per plan: R-01 invariance (learning cols + hash chain + id + edges + no supersession, for add/remove/replace) + read-freshness; R-02 evict-prior, rollback-on-insert-failure (trigger-injected), colon-less degrade, no-prior, concurrent last-writer-wins; R-08 `_` and `%` sibling-survival + SQL-metachar literal storage; edge cases (duplicate add idempotent, absent remove no-op, FK cascade clean error).
- `cargo fmt` + `cargo clippy -p unimatrix-store --tests`: clean.

## Issues / Blockers
None.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search(pattern) -- surfaced ADR-001 (#5599), ADR-009 (#5610), entry_tags FK CASCADE (#360), junction-table pattern (#373), and the "no tags column on entries" lesson (#4457). Applied: direct single-row write, ESCAPE clause, bound params, no entries-column touch.
- Stored: entry #5612 "Force a mid-transaction INSERT failure in tests with a BEFORE INSERT RAISE(ABORT) trigger" via /uni-store-pattern (topic unimatrix-store) — reusable technique for proving sqlx multi-statement transaction rollback.
