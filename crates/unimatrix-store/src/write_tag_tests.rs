//! Tests for the direct `entry_tags` write primitives (vnc-045):
//! `add_tag`, `remove_tag`, `replace_tag`.
//!
//! Seam: `SqlxStore::open` over a temp DB. Entries are seeded via `TestEntry`; the
//! `entry_tags` lane is inspected through the public `get()` read path and edges via
//! `query_outgoing_edges`. Covers R-01 (invariance), R-02 (atomic replace / rollback /
//! degrade), R-08 (LIKE over-match + injection), and defined edge-case behavior.

use crate::SqlxStore;
use crate::test_helpers::{TestEntry, open_test_store};

// Non-zero learning-vector values seeded on every entry so an accidental
// zero-out by a forbidden `update()`/`correct()` path would be caught.
const CONF: f64 = 0.87;
const ACCESS: i64 = 42;
const LAST_ACC: i64 = 1_700_000_000;
const HELPFUL: i64 = 7;
const UNHELPFUL: i64 = 3;
const EDGE_TARGET: i64 = 999;

/// Seed an entry with the given tags, non-zero learning columns, and one outgoing edge.
async fn seed(store: &SqlxStore, tags: &[&str]) -> u64 {
    let id = store
        .insert(
            TestEntry::new("vnc-045", "decision")
                .with_tags(tags)
                .build(),
        )
        .await
        .expect("insert");

    sqlx::query(
        "UPDATE entries SET confidence=?1, access_count=?2, last_accessed_at=?3,
                            helpful_count=?4, unhelpful_count=?5 WHERE id=?6",
    )
    .bind(CONF)
    .bind(ACCESS)
    .bind(LAST_ACC)
    .bind(HELPFUL)
    .bind(UNHELPFUL)
    .bind(id as i64)
    .execute(store.write_pool_server())
    .await
    .expect("set learning columns");

    sqlx::query(
        "INSERT INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
         VALUES (?1, ?2, 'Supports', 1.0, 123, 'agent', 'manual', 0)",
    )
    .bind(id as i64)
    .bind(EDGE_TARGET)
    .execute(store.write_pool_server())
    .await
    .expect("insert edge");

    id
}

/// Byte-for-byte snapshot of every field a tag mutation MUST leave untouched (R-01).
#[derive(Debug, PartialEq)]
struct Invariant {
    id: u64,
    confidence_bits: u64,
    access_count: u32,
    last_accessed_at: u64,
    helpful_count: u32,
    unhelpful_count: u32,
    content_hash: String,
    previous_hash: String,
    version: u32,
    superseded_by: Option<u64>,
    edges: Vec<(u64, String)>,
}

async fn snapshot(store: &SqlxStore, id: u64) -> Invariant {
    let e = store.get(id).await.expect("get");
    let mut edges: Vec<(u64, String)> = store
        .query_outgoing_edges(id)
        .await
        .expect("edges")
        .into_iter()
        .map(|r| (r.target_id, r.relation_type))
        .collect();
    edges.sort();
    Invariant {
        id: e.id,
        confidence_bits: e.confidence.to_bits(),
        access_count: e.access_count,
        last_accessed_at: e.last_accessed_at,
        helpful_count: e.helpful_count,
        unhelpful_count: e.unhelpful_count,
        content_hash: e.content_hash,
        previous_hash: e.previous_hash,
        version: e.version,
        superseded_by: e.superseded_by,
        edges,
    }
}

async fn tags_of(store: &SqlxStore, id: u64) -> Vec<String> {
    store.get(id).await.expect("get").tags
}

// -- R-01: invariance after mutation --------------------------------------------

#[tokio::test]
async fn test_add_tag_preserves_learning_columns() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &["existing:tag"]).await;

    let before = snapshot(&store, id).await;
    store.add_tag(id, "delivery:proven").await.expect("add_tag");
    let after = snapshot(&store, id).await;

    assert_eq!(
        (
            before.confidence_bits,
            before.access_count,
            before.last_accessed_at,
            before.helpful_count,
            before.unhelpful_count
        ),
        (
            after.confidence_bits,
            after.access_count,
            after.last_accessed_at,
            after.helpful_count,
            after.unhelpful_count
        ),
        "add_tag must not touch any learning column"
    );
}

#[tokio::test]
async fn test_add_tag_preserves_hash_chain() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &["existing:tag"]).await;

    let before = snapshot(&store, id).await;
    store.add_tag(id, "delivery:proven").await.expect("add_tag");
    let after = snapshot(&store, id).await;

    assert_eq!(
        before.content_hash, after.content_hash,
        "content_hash changed"
    );
    assert_eq!(
        before.previous_hash, after.previous_hash,
        "previous_hash changed"
    );

    // Integrity oracle: the stored hash still equals the recomputed content hash —
    // no `ContentHashMismatch` (tags are outside the hash, hash.rs:7-16).
    let e = store.get(id).await.unwrap();
    let recomputed = crate::hash::compute_content_hash(&e.title, &e.content);
    assert_eq!(
        e.content_hash, recomputed,
        "content_hash must match recomputed"
    );
}

#[tokio::test]
async fn test_add_tag_preserves_id_and_edges() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &["existing:tag"]).await;

    let before = snapshot(&store, id).await;
    store.add_tag(id, "delivery:proven").await.expect("add_tag");
    let after = snapshot(&store, id).await;

    assert_eq!(before.id, after.id, "id changed");
    assert_eq!(
        before.version, after.version,
        "version bumped (supersession minted)"
    );
    assert_eq!(
        before.superseded_by, after.superseded_by,
        "superseded_by changed"
    );
    assert_eq!(before.edges, after.edges, "edge set changed");
    assert_eq!(
        before.edges,
        vec![(EDGE_TARGET as u64, "Supports".to_string())]
    );
}

#[tokio::test]
async fn test_remove_tag_invariance() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &["delivery:partial"]).await;

    let before = snapshot(&store, id).await;
    store
        .remove_tag(id, "delivery:partial")
        .await
        .expect("remove_tag");
    let after = snapshot(&store, id).await;

    assert_eq!(
        before, after,
        "remove_tag must leave every non-tag field invariant"
    );
}

#[tokio::test]
async fn test_replace_tag_invariance() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &["delivery:partial"]).await;

    let before = snapshot(&store, id).await;
    store
        .replace_tag(id, "delivery", "delivery:proven")
        .await
        .expect("replace_tag");
    let after = snapshot(&store, id).await;

    assert_eq!(
        before, after,
        "replace_tag must leave every non-tag field invariant"
    );
}

// -- R-01: read-freshness (no stale window, no invalidation step) ----------------

#[tokio::test]
async fn test_tag_read_freshness() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &[]).await;

    store.add_tag(id, "delivery:proven").await.expect("add_tag");
    assert!(
        tags_of(&store, id)
            .await
            .contains(&"delivery:proven".to_string()),
        "tag must be visible immediately after add"
    );

    store
        .remove_tag(id, "delivery:proven")
        .await
        .expect("remove_tag");
    assert!(
        !tags_of(&store, id)
            .await
            .contains(&"delivery:proven".to_string()),
        "tag must be absent immediately after remove"
    );
}

// -- R-02: replace atomicity / rollback / degrade --------------------------------

#[tokio::test]
async fn test_replace_tag_single_value_evicts_prior() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &["delivery:partial"]).await;

    let prior = store
        .replace_tag(id, "delivery", "delivery:proven")
        .await
        .expect("replace_tag");

    assert_eq!(
        prior,
        Some("delivery:partial".to_string()),
        "must return evicted prior"
    );
    let delivery: Vec<String> = tags_of(&store, id)
        .await
        .into_iter()
        .filter(|t| t.starts_with("delivery:"))
        .collect();
    assert_eq!(
        delivery,
        vec!["delivery:proven".to_string()],
        "exactly one delivery:* tag"
    );
}

/// CORE (R-02): a forced INSERT failure mid-transaction rolls the whole txn back —
/// the prior value survives; never a zero-`namespace:*` window.
#[tokio::test]
async fn test_replace_tag_rollback_on_insert_failure() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &["delivery:partial"]).await;

    // Trigger aborts the INSERT of the sentinel tag, forcing Step C to fail AFTER the
    // Step B DELETE has run inside the same transaction. Because replace_tag never
    // reaches commit, dropping the txn rolls back the DELETE too.
    sqlx::query(
        "CREATE TRIGGER force_fail_insert BEFORE INSERT ON entry_tags
         WHEN NEW.tag = 'delivery:FAIL'
         BEGIN SELECT RAISE(ABORT, 'forced insert failure'); END",
    )
    .execute(store.write_pool_server())
    .await
    .expect("create trigger");

    let result = store.replace_tag(id, "delivery", "delivery:FAIL").await;
    assert!(result.is_err(), "forced INSERT failure must surface as Err");

    let tags = tags_of(&store, id).await;
    assert!(
        tags.contains(&"delivery:partial".to_string()),
        "prior delivery:partial must survive rollback"
    );
    assert!(
        !tags.contains(&"delivery:FAIL".to_string()),
        "failed new value must not be present"
    );
}

#[tokio::test]
async fn test_replace_tag_colon_less_degrades_to_add() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &["existing:tag"]).await;

    // Empty (colon-less) namespace → pure insert, no prior removed, no over-broad DELETE.
    let prior = store
        .replace_tag(id, "", "plainfoo")
        .await
        .expect("replace_tag");
    assert_eq!(prior, None, "colon-less replace evicts no prior");

    let tags = tags_of(&store, id).await;
    assert!(
        tags.contains(&"existing:tag".to_string()),
        "pre-existing tag must survive"
    );
    assert!(
        tags.contains(&"plainfoo".to_string()),
        "new tag must be inserted"
    );
}

#[tokio::test]
async fn test_replace_tag_no_prior_in_namespace() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &[]).await;

    let prior = store
        .replace_tag(id, "delivery", "delivery:proven")
        .await
        .expect("replace_tag");
    assert_eq!(prior, None, "empty namespace yields no prior");
    assert!(
        tags_of(&store, id)
            .await
            .contains(&"delivery:proven".to_string())
    );
}

/// Two racing replaces on the same (entry, namespace) never leave two `namespace:*`
/// tags — the single-transaction scope makes it last-writer-wins.
#[tokio::test]
async fn test_replace_tag_one_transaction_atomic() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &["delivery:a"]).await;

    let (r1, r2) = tokio::join!(
        store.replace_tag(id, "delivery", "delivery:x"),
        store.replace_tag(id, "delivery", "delivery:y"),
    );
    r1.expect("replace 1");
    r2.expect("replace 2");

    let delivery: Vec<String> = tags_of(&store, id)
        .await
        .into_iter()
        .filter(|t| t.starts_with("delivery:"))
        .collect();
    assert_eq!(
        delivery.len(),
        1,
        "exactly one delivery:* tag must remain, got {delivery:?}"
    );
}

// -- R-08: LIKE over-match + injection -------------------------------------------

#[tokio::test]
async fn test_replace_tag_like_underscore_namespace_no_over_match() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    // Sibling "axb:sibling" would be caught by an UNescaped LIKE 'a_b:%' (_ = any char).
    let id = seed(&store, &["a_b:old", "axb:sibling"]).await;

    let prior = store
        .replace_tag(id, "a_b", "a_b:new")
        .await
        .expect("replace_tag");
    assert_eq!(prior, Some("a_b:old".to_string()));

    let tags = tags_of(&store, id).await;
    assert!(tags.contains(&"a_b:new".to_string()), "new tag present");
    assert!(
        !tags.contains(&"a_b:old".to_string()),
        "prior under namespace removed"
    );
    assert!(
        tags.contains(&"axb:sibling".to_string()),
        "sibling under a DIFFERENT prefix must survive the escaped DELETE"
    );
}

#[tokio::test]
async fn test_replace_tag_like_percent_namespace_no_over_match() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    // Sibling "pXYZq:sib" would be caught by an UNescaped LIKE 'p%q:%' (% = any run).
    let id = seed(&store, &["p%q:old", "pXYZq:sib"]).await;

    let prior = store
        .replace_tag(id, "p%q", "p%q:new")
        .await
        .expect("replace_tag");
    assert_eq!(prior, Some("p%q:old".to_string()));

    let tags = tags_of(&store, id).await;
    assert!(tags.contains(&"p%q:new".to_string()), "new tag present");
    assert!(!tags.contains(&"p%q:old".to_string()), "prior removed");
    assert!(
        tags.contains(&"pXYZq:sib".to_string()),
        "sibling must survive escaped DELETE"
    );
}

#[tokio::test]
async fn test_add_tag_sql_metachar_stored_literally() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &[]).await;

    let nasty = "weird'; DROP TABLE entries; --%_";
    store.add_tag(id, nasty).await.expect("add_tag");
    assert!(
        tags_of(&store, id).await.contains(&nasty.to_string()),
        "metacharacter-laden tag must be stored verbatim (bound params)"
    );

    // entries table still exists (no injection executed) and remove matches literally.
    store.remove_tag(id, nasty).await.expect("remove_tag");
    assert!(
        !tags_of(&store, id).await.contains(&nasty.to_string()),
        "literal remove"
    );
    assert!(
        store.exists(id).await.expect("exists"),
        "entry must still exist"
    );
}

// -- Edge cases ------------------------------------------------------------------

#[tokio::test]
async fn test_add_tag_duplicate() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &[]).await;

    store
        .add_tag(id, "delivery:proven")
        .await
        .expect("first add");
    store
        .add_tag(id, "delivery:proven")
        .await
        .expect("duplicate add must be a no-op");

    let count = tags_of(&store, id)
        .await
        .into_iter()
        .filter(|t| t == "delivery:proven")
        .count();
    assert_eq!(
        count, 1,
        "duplicate add is idempotent — single row, no PK error"
    );
}

#[tokio::test]
async fn test_remove_tag_absent() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &["existing:tag"]).await;

    store
        .remove_tag(id, "never:added")
        .await
        .expect("absent remove is a no-op");
    assert_eq!(
        tags_of(&store, id).await,
        vec!["existing:tag".to_string()],
        "removing an absent tag changes nothing"
    );
}

#[tokio::test]
async fn test_tag_on_cascade_deleted_entry() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = seed(&store, &["existing:tag"]).await;

    store.delete(id).await.expect("delete entry");

    // Inserting a tag for a cascade-deleted entry must fail the FK constraint cleanly
    // (no partial write), not silently succeed.
    let result = store.add_tag(id, "delivery:proven").await;
    assert!(
        result.is_err(),
        "add_tag on a deleted entry must surface a clean StoreError"
    );
}
