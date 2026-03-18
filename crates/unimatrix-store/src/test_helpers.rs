//! Reusable test infrastructure for unimatrix-store and downstream crates.
//!
//! Available within this crate via `#[cfg(test)]` and to downstream crates
//! via the `test-support` feature flag.

use crate::SqlxStore;
use crate::pool_config::PoolConfig;
use crate::schema::{NewEntry, Status, TimeRange};

/// Open a test store in the given temporary directory.
///
/// Creates or opens `test.db` under `dir.path()`.
pub async fn open_test_store(dir: &tempfile::TempDir) -> SqlxStore {
    let path = dir.path().join("test.db");
    SqlxStore::open(&path, PoolConfig::default())
        .await
        .expect("failed to open test database")
}

/// A self-contained test database that owns its temporary directory.
///
/// Used by integration tests that need a `&SqlxStore` without managing
/// the `TempDir` lifetime manually.
pub struct TestDb {
    store: SqlxStore,
    _dir: tempfile::TempDir,
}

impl TestDb {
    /// Create a new in-memory-backed test store.
    ///
    /// Panics if the runtime is not available (must be called within a tokio context
    /// or via `tokio::runtime::Handle::current().block_on`).
    pub fn new() -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime");
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = rt.block_on(open_test_store(&dir));
        Self { store, _dir: dir }
    }

    /// Return a reference to the underlying store.
    pub fn store(&self) -> &SqlxStore {
        &self.store
    }
}

impl Default for TestDb {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for constructing test entries with sensible defaults.
pub struct TestEntry {
    title: Option<String>,
    content: String,
    topic: String,
    category: String,
    tags: Vec<String>,
    source: String,
    status: Status,
    created_by: String,
    feature_cycle: String,
    trust_source: String,
}

impl TestEntry {
    /// Create a new test entry builder with the given topic and category.
    pub fn new(topic: &str, category: &str) -> Self {
        Self {
            title: None,
            content: format!("Content for {topic}/{category}"),
            topic: topic.to_string(),
            category: category.to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: String::new(),
            feature_cycle: String::new(),
            trust_source: String::new(),
        }
    }

    /// Set the title.
    pub fn with_title(mut self, title: &str) -> Self {
        self.title = Some(title.to_string());
        self
    }

    /// Set the content.
    pub fn with_content(mut self, content: &str) -> Self {
        self.content = content.to_string();
        self
    }

    /// Set the tags.
    pub fn with_tags(mut self, tags: &[&str]) -> Self {
        self.tags = tags.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Set the status.
    pub fn with_status(mut self, status: Status) -> Self {
        self.status = status;
        self
    }

    /// Set the source.
    pub fn with_source(mut self, source: &str) -> Self {
        self.source = source.to_string();
        self
    }

    /// Set the created_by field.
    pub fn with_created_by(mut self, val: &str) -> Self {
        self.created_by = val.to_string();
        self
    }

    /// Set the feature_cycle field.
    pub fn with_feature_cycle(mut self, val: &str) -> Self {
        self.feature_cycle = val.to_string();
        self
    }

    /// Set the trust_source field.
    pub fn with_trust_source(mut self, val: &str) -> Self {
        self.trust_source = val.to_string();
        self
    }

    /// Build a `NewEntry` with sensible defaults for unset fields.
    pub fn build(self) -> NewEntry {
        NewEntry {
            title: self
                .title
                .unwrap_or_else(|| format!("Test: {}/{}", self.topic, self.category)),
            content: self.content,
            topic: self.topic,
            category: self.category,
            tags: self.tags,
            source: self.source,
            status: self.status,
            created_by: self.created_by,
            feature_cycle: self.feature_cycle,
            trust_source: self.trust_source,
        }
    }
}

/// Verify that an entry is correctly represented in all index tables.
///
/// Panics with a descriptive message if any index is inconsistent.
pub async fn assert_index_consistent(store: &SqlxStore, entry_id: u64) {
    let record = store
        .get(entry_id)
        .await
        .unwrap_or_else(|_| panic!("entry {entry_id} should exist in entries"));

    // Verify topic index
    let topic_results = store.query_by_topic(&record.topic).await.unwrap();
    assert!(
        topic_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} not found in topic index for topic '{}'",
        record.topic
    );

    // Verify category index
    let cat_results = store.query_by_category(&record.category).await.unwrap();
    assert!(
        cat_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} not found in category index for category '{}'",
        record.category
    );

    // Verify tag index
    for tag in &record.tags {
        let tag_results = store
            .query_by_tags(std::slice::from_ref(tag))
            .await
            .unwrap();
        assert!(
            tag_results.iter().any(|r| r.id == entry_id),
            "entry {entry_id} not found in tag index for tag '{tag}'"
        );
    }

    // Verify status index
    let status_results = store.query_by_status(record.status).await.unwrap();
    assert!(
        status_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} not found in status index for status {:?}",
        record.status
    );

    // Verify time index
    let time_results = store
        .query_by_time_range(TimeRange {
            start: record.created_at,
            end: record.created_at,
        })
        .await
        .unwrap();
    assert!(
        time_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} not found in time index for created_at {}",
        record.created_at
    );
}

/// Verify that an entry is NOT present in index positions for the given old values.
///
/// Used after updates to confirm stale index entries were removed.
pub async fn assert_index_absent(
    store: &SqlxStore,
    entry_id: u64,
    old_topic: &str,
    old_category: &str,
    old_tags: &[String],
    _old_status: Status,
) {
    let current = store.get(entry_id).await.ok();
    let current_topic = current.as_ref().map(|r| r.topic.as_str());

    if current_topic != Some(old_topic) {
        let topic_results = store.query_by_topic(old_topic).await.unwrap();
        assert!(
            !topic_results.iter().any(|r| r.id == entry_id),
            "entry {entry_id} still found in topic index for old topic '{old_topic}'"
        );
    }

    let current_category = current.as_ref().map(|r| r.category.as_str());
    if current_category != Some(old_category) {
        let cat_results = store.query_by_category(old_category).await.unwrap();
        assert!(
            !cat_results.iter().any(|r| r.id == entry_id),
            "entry {entry_id} still found in category index for old category '{old_category}'"
        );
    }

    let current_tags: Vec<String> = current.as_ref().map(|r| r.tags.clone()).unwrap_or_default();
    for tag in old_tags {
        if !current_tags.contains(tag) {
            let tag_results = store
                .query_by_tags(std::slice::from_ref(tag))
                .await
                .unwrap();
            assert!(
                !tag_results.iter().any(|r| r.id == entry_id),
                "entry {entry_id} still found in tag index for old tag '{tag}'"
            );
        }
    }
}

/// Seed a database with a deterministic set of entries for testing.
///
/// Returns the IDs of all inserted entries.
pub async fn seed_entries(store: &SqlxStore, count: usize) -> Vec<u64> {
    let topics = ["auth", "logging", "database", "api", "testing"];
    let categories = ["convention", "decision", "pattern"];
    let all_tags = ["rust", "error", "async", "testing", "performance"];

    let mut ids = Vec::new();
    for i in 0..count {
        let topic = topics[i % topics.len()];
        let category = categories[i % categories.len()];
        let num_tags = (i % all_tags.len()) + 1;
        let tags: Vec<&str> = all_tags[..num_tags].to_vec();

        let entry = TestEntry::new(topic, category)
            .with_tags(&tags)
            .with_title(&format!("Entry {i}"))
            .build();
        let id = store.insert(entry).await.unwrap();
        ids.push(id);
    }
    ids
}
