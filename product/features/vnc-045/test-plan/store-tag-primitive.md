# Test Plan — Direct tag-write primitives (`unimatrix-store`, beside write.rs:161)

> New fns: `add_tag(entry_id, tag)`, `remove_tag(entry_id, tag)`, `replace_tag(entry_id, namespace, new_tag) -> Option<String>`. Each atomic, direct `entry_tags` write (INSERT/DELETE mirroring write.rs:78/161), touches NO `entries` column. Covers R-01 (invariance), R-02 (atomicity/rollback/degrade), R-08 (injection/LIKE), edge cases.
>
> Seam: `SqlxStore::open` over a temp DB (unit-constructible). Seed via existing `insert`/`TestEntry` helpers; inspect `entry_tags` via existing `load_tags_for_entries(pool, &[id])` (read.rs:111-150) and raw SELECT.

## R-01 — Invariance after mutation (High, comprehensive)

Seed one entry with **non-zero** `confidence`, `access_count`, `helpful_count`, `unhelpful_count`, and a set `last_accessed_at`; give it edges and a known `content_hash`/`previous_hash`.

1. `test_add_tag_preserves_learning_columns` — after `add_tag`, assert all five learning columns byte-identical pre/post (contrast: `context_correct` zeroes them, write_ext.rs:542-561). (FR-06, NFR-01, AC-02)
2. `test_add_tag_preserves_hash_chain` — `content_hash` and `previous_hash` identical pre/post; integrity oracle (`chain_verify.rs:152`) yields no `ContentHashMismatch`. (FR-04, NFR-02)
3. `test_add_tag_preserves_id_and_edges` — entry `id` unchanged, no supersession version minted, full edge set byte-identical pre/post — behavioral proof `update()`/`correct()` were NOT invoked. (FR-03/FR-04, NFR-03, AC-01)
4. `test_remove_tag_invariance` / `test_replace_tag_invariance` — repeat the 5-column + hash-chain + id/edge assertions for one `remove` and one `replace`. (Coverage: invariance proven for ≥1 add, 1 remove, 1 replace.)

## R-02 — `replace` atomicity, rollback, degrade (High)

1. `test_replace_tag_single_value_evicts_prior` — seed `delivery:partial`; `replace_tag(id, "delivery", "delivery:proven")`; assert exactly one `delivery:*` tag remains (`proven`), prior gone, and the fn **returns `Some("delivery:partial")`** (the evicted prior) in one observable step. (FR-05, AC-03)
2. `test_replace_tag_rollback_on_insert_failure` — **CORE.** `replace_tag` runs `DELETE ... WHERE tag LIKE 'namespace:%'` + `INSERT new` in ONE SQLite transaction (mirror `insert_in_txn`, #267). Inject a forced INSERT failure (e.g. violate a constraint / drop a poisoned connection mid-tx) and assert the DELETE **rolls back** — the prior `delivery:partial` survives; assert **0 observable zero-tag intermediate states**; never a `namespace:*`-empty entry. (FR-05, NFR-05, R-02, AC-03) — historical non-transactional posture #4420 is the temptation to guard against.
3. `test_replace_tag_colon_less_degrades_to_add` — `replace_tag` with a colon-less tag (null namespace) performs a **pure insert**: no prior removed, returns `None`, never hard-errors on a valid tag. Assert no pre-existing tags removed. (ADR-004 edge, ARCH §4.3, R-02)
4. `test_replace_tag_no_prior_in_namespace` — namespace has no existing tag → pure insert, returns `None`. (Edge case; feeds R-03 `prior_value:null`.)
5. `test_replace_tag_one_transaction_atomic` — two racing `replace_tag` on the same `(entry, namespace)` never leave two `namespace:*` tags (last-writer-wins, atomic tx). (Concurrency edge case.)

## R-08 — Injection / over-broad DELETE (Low-Med, security-critical)

1. `test_add_tag_sql_metachar_stored_literally` — a `tag` containing `'`, `;`, `--`, `%`, `_` is stored verbatim (bound params, no interpolation) and matched literally on read/remove — no injection, no unintended deletion. (Security §, R-08)
2. `test_replace_tag_like_metachar_namespace_no_over_match` — **CORE.** Seed sibling tags whose names would be caught by an unescaped `LIKE` (e.g. namespace `a_b` vs sibling `axb:...`, or `%` in the derived prefix). Call `replace_tag` with a derived namespace containing `%`/`_`; assert the DELETE either **rejects the malformed prefix** OR **LIKE-escapes** the metacharacters so ONLY true `namespace:` rows are removed — sibling tags survive. All INSERT/DELETE use bound parameters. (Security §, R-08) — implementer picks reject-vs-escape; test asserts the chosen behavior leaves siblings intact.

## Edge Cases (defined-behavior assertions)

1. `test_add_tag_duplicate` — adding a tag that already exists: assert the DEFINED behavior against the `entry_tags` PK/unique constraint (idempotent no-op vs duplicate-key error) — pick one, assert it deterministically.
2. `test_remove_tag_absent` — removing a tag not present: assert defined behavior (no-op vs error); note prior_value handling feeds R-03.
3. `test_tag_on_cascade_deleted_entry` — `entry_tags` FK `ON DELETE CASCADE` (nxs-008 #360): a primitive against a since-deleted entry surfaces a clean `CoreError::Store`, not a partial write.

## Out of Scope (do NOT test here)
- No value validation / allow-list — primitives are value-opaque (R-04 seam, service tests).
- No lifecycle guard at this layer — guards live above the primitive (service/handler; R-05 in store-tag-service.md).
