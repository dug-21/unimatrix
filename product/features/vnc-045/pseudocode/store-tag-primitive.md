# Component: Direct tag-write primitives

**Crate/file:** `unimatrix-store` — new `impl SqlxStore` fns beside `write.rs:161`.
**ADRs:** ADR-001 (direct single-row write, not `update()`), ADR-004 (atomic replace).
**Risks covered:** R-01 (forbidden-surface), R-02 (atomic replace), R-08 (LIKE over-match / injection).

## Purpose

Mutate the `entry_tags` junction lane in place — single-row `INSERT`/`DELETE` for `add`/`remove`,
and a namespace-scoped atomic `DELETE + INSERT` for `replace`. Each touches ONLY `entry_tags`;
none reads or writes any `entries` column (no `content_hash`, no learning columns, no counters —
contrast `write.rs:97 update()` which rewrites 24 columns and re-hashes at `:115`). No `entry_tags`
reader caches (ARCHITECTURE §3), so no invalidation step is required (ADR-002).

## Reference facts (HEAD)

- Table (`db.rs:574`): `entry_tags(entry_id INTEGER NOT NULL, tag TEXT NOT NULL, PRIMARY KEY(entry_id, tag), FOREIGN KEY(entry_id) REFERENCES entries(id) ON DELETE CASCADE)`.
- Existing single-row INSERT precedent: `write.rs:168` (`INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)`).
- Existing DELETE-ALL precedent (do NOT copy the ALL scope): `write.rs:161`.
- Txn open + commit precedent: `write.rs:20-24 / 89-91` (`self.write_pool.begin()` → map_pool_timeout; `txn.commit()`).
- Error type: `crate::error::Result<T>` / `StoreError` (`StoreError::Database(e.into())`, `map_pool_timeout(e, PoolKind::Write)`).
- All binds positional (`?1`, `?2`) — bound parameters only; `tag` is stored/matched literally.

## New functions

### `add_tag(entry_id, tag) -> Result<()>`

```
async fn add_tag(&self, entry_id: u64, tag: &str) -> Result<()>:
    txn = self.write_pool.begin() ? (map_pool_timeout Write)
    // Idempotent: re-adding an existing (entry_id, tag) is a no-op, not a PK-violation error.
    // Least-surprise for a value-opaque add (Edge Cases: "add a tag that already exists").
    sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)
                 ON CONFLICT(entry_id, tag) DO NOTHING")
        .bind(entry_id as i64)
        .bind(tag)                      // bound param — literal, no interpolation (R-08)
        .execute(&mut *txn) ? (StoreError::Database)
    txn.commit() ? (StoreError::Database)
    Ok(())
```

- Touches no `entries` row → learning vector, hash chain, edges all invariant (R-01, FR-04/FR-06).
- FK note: if `entry_id` was deleted between the handler's `get` and here (TOCTOU), the INSERT
  fails the FK constraint → surfaces as `StoreError::Database` → service maps to `CoreError::Store`
  (clean error, no partial write — RISK Integration Risks `ON DELETE CASCADE`).

### `remove_tag(entry_id, tag) -> Result<Option<String>>`

```
async fn remove_tag(&self, entry_id: u64, tag: &str) -> Result<Option<String>>:
    txn = self.write_pool.begin() ? (map_pool_timeout Write)
    result = sqlx::query("DELETE FROM entry_tags WHERE entry_id = ?1 AND tag = ?2")  // single-row, NOT delete-all
        .bind(entry_id as i64)
        .bind(tag)                      // bound param (R-08)
        .execute(&mut *txn) ? (StoreError::Database)
    txn.commit() ? (StoreError::Database)
    // Report what was removed so the service can satisfy ADR-009 "prior_value non-null on remove".
    if result.rows_affected() > 0: Ok(Some(tag.to_string()))
    else:                          Ok(None)      // tag was absent → no-op (Edge Cases: "remove absent tag")
```

- Returns the removed tag (the client named it exactly) → service records it as `prior_value`.
- Absent tag → `None`, no error; service still emits one audit event with `prior_value = tag`
  per FR-10 (the client's stated intent). See Open Questions on absent-remove `prior_value`.

### `replace_tag(entry_id, namespace, new_tag) -> Result<Option<String>>` — ATOMIC (ADR-004)

`namespace` is the derived prefix supplied by the caller (handler). Caller guarantees it is
non-empty AND LIKE-safe (see R-08 below); the colon-less case never reaches `replace_tag` with a
namespace — it is routed to `add_tag` by the service (degrade-to-add).

```
async fn replace_tag(&self, entry_id: u64, namespace: &str, new_tag: &str) -> Result<Option<String>>:
    txn = self.write_pool.begin() ? (map_pool_timeout Write)

    // Step A: read the evicted prior (for the audit prior_value) — inside the SAME txn.
    // ESCAPE the LIKE pattern: namespace may contain '%' or '_' (R-08). Escape them and
    // declare ESCAPE '\'. Build the pattern as  <escaped-namespace> || ':%'.
    like_pattern = like_escape(namespace) + ":%"        // like_escape: %→\%, _→\_, \→\\
    prior: Option<String> = sqlx::query_scalar(
        "SELECT tag FROM entry_tags
         WHERE entry_id = ?1 AND tag LIKE ?2 ESCAPE '\\'
         LIMIT 1")
        .bind(entry_id as i64)
        .bind(&like_pattern)
        .fetch_optional(&mut *txn) ? (StoreError::Database)

    // Step B: namespace-scoped DELETE (NOT delete-all) — same escaped pattern.
    sqlx::query("DELETE FROM entry_tags WHERE entry_id = ?1 AND tag LIKE ?2 ESCAPE '\\'")
        .bind(entry_id as i64)
        .bind(&like_pattern)
        .execute(&mut *txn) ? (StoreError::Database)

    // Step C: INSERT the new tag (idempotent guard; new_tag shares the namespace so B removed it if present).
    sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)
                 ON CONFLICT(entry_id, tag) DO NOTHING")
        .bind(entry_id as i64)
        .bind(new_tag)
        .execute(&mut *txn) ? (StoreError::Database)

    // Step D: ONE commit. If B or C errored above, `?` returns early → txn dropped → rollback:
    // the prior value survives; NEVER a zero-`namespace:*` window (R-02, AC-03, NFR-05).
    txn.commit() ? (StoreError::Database)

    Ok(prior)   // evicted prior value, or None when the namespace held nothing
```

- **One transaction** — DELETE + INSERT share `txn`; commit once. This is the R-02 core guarantee.
- **Namespace-scoped, never delete-all** — the `LIKE 'namespace:%'` scope must NOT regress to
  `write.rs:161`'s `DELETE FROM entry_tags WHERE entry_id = ?1` (that would wipe every tag).
- **LIKE-escape (R-08)** — `like_escape` neutralizes `%`/`_`/`\` so the DELETE matches only true
  `namespace:` rows, never siblings under a different prefix. `ESCAPE '\'` clause is mandatory
  wherever the escaped pattern is used. (Alternative permitted by BRIEF: reject a namespace
  containing `%`/`_` as malformed at the handler; this file implements the escape path so the
  store is safe regardless. Pick ONE and document — see Open Questions.)
- Touches no `entries` row (R-01 invariance holds for replace too).

## `like_escape` helper (module-private)

```
fn like_escape(s: &str) -> String:
    // Order matters: escape the escape char first.
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
```

## Error handling

| Condition | Result |
|-----------|--------|
| Pool acquire timeout | `map_pool_timeout(e, PoolKind::Write)` → `StoreError` |
| SQL execute/commit failure | `StoreError::Database(e.into())` (service → `CoreError::Store`) |
| INSERT on cascaded-away entry (FK) | `StoreError::Database` (clean; no partial state) |
| replace INSERT fails mid-txn | `?` early-return drops txn → full rollback; prior survives (R-02) |

## Key test scenarios (hints for the tester — store-primitive seam, directly constructible)

1. **Invariance (R-01):** seed entry with non-zero `confidence/access_count/helpful_count/`
   `unhelpful_count/last_accessed_at`; run each of add/remove/replace; assert all five learning
   columns + `content_hash` + `previous_hash` + full edge set + `id` byte-identical pre/post; no
   supersession version minted.
2. **Read-freshness (R-01/NFR-04):** after `add_tag`, a live `load_tags_for_entries` / tag-filtered
   query shows the tag; after `remove_tag`, it is absent — no invalidation step.
3. **Atomic replace happy path (R-02 #1):** entry holds `delivery:partial`; `replace_tag(_, "delivery", "delivery:proven")`
   returns `Some("delivery:partial")`; exactly one `delivery:*` tag remains (`proven`).
4. **Rollback (R-02 #2):** inject a forced failure at the INSERT step (Step C); assert the DELETE
   rolled back — `delivery:partial` still present, zero `delivery:proven`, no zero-tag window.
5. **Replace with no prior (R-02 #4):** namespace empty → returns `None`, pure insert.
6. **LIKE over-match (R-08):** entry holds `delivery:proven` and `delivery_x:note`; replacing under
   derived namespace `delivery` must NOT delete `delivery_x:note` (escape proves `_` is literal).
7. **Injection (R-08):** a `tag`/`new_tag` containing `'`, `%`, `_`, `;` is stored and matched
   literally (bound params); no SQL executes from tag content.
8. **Idempotent add (Edge):** `add_tag` twice with same `(id, tag)` → single row, no PK error.
9. **Absent remove (Edge):** `remove_tag` for a tag not present → `Ok(None)`, no error.
10. **FK cascade (Integration):** delete the parent entry, then a primitive call surfaces a clean
    `StoreError`, not a partial write.
