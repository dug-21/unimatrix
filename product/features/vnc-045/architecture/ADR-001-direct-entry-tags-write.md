## ADR-001: `context_tag` writes `entry_tags` directly via a new single-row primitive, not `update()`

### Context
`context_tag(id, action, tag)` mutates the generic tag lane in place. Two existing store paths touch `entry_tags`: (1) `update()` (write.rs:97) rewrites all 24 entry columns including `content_hash`/`previous_hash` (write.rs:115) and does a DELETE-all + re-INSERT of tags (write.rs:161-174); (2) `context_correct` mints a supersession version and hard-resets the entire learning vector to zero (write_ext.rs:542-561). Both are wrong for a volatile tag: tags are outside the content hash (hash.rs:7-16), so a tag change must not disturb chain fields, and routing it through either path destroys accumulated self-learning history (ass-093 Q2, decisive argument). No standalone single-row tag primitive exists today — only full-replace and the create/correct INSERTs.

### Decision
Add a new store primitive that writes `entry_tags` directly with single-row statements, on the same `entry_id`, in one transaction, touching no `entries` column:
- `add_tag(entry_id, tag)` → `INSERT INTO entry_tags(entry_id, tag) VALUES(?, ?)` (mirror write.rs:168).
- `remove_tag(entry_id, tag)` → `DELETE FROM entry_tags WHERE entry_id=? AND tag=?` (single-row, NOT the DELETE-all of write.rs:161).
- `replace_prefixed_tag(entry_id, prefix, new_tag) -> Option<prior>` → atomic delete of prior `prefix*` + insert new (ADR-004).

The op MUST NOT call `update()` and MUST NOT call `context_correct`. `content_hash`, `previous_hash`, all learning columns (`confidence`, `access_count`, `last_accessed_at`, `helpful_count`, `unhelpful_count`), edges, and the embedding are left untouched (SD-1, SD-2, SD-3). No schema change; no DB migration.

### Consequences
- Easier: a tag change is integrity-safe by construction (tag is not hashed) and learning-preserving (no vector reset) — AC-01, AC-02 hold by construction.
- Easier: zero edge re-point (same `entry_id`), no re-embed, no re-hash — the cheap path ass-093 identified.
- Harder: a net-new store primitive must be tested against the temptation to reuse `update()`; the single-row DELETE must not regress into the DELETE-all pattern.
- Cross-references ADR-004 (replace atomicity), ADR-002 (why no in-memory refresh is needed).
