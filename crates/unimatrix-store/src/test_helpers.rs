//! Reusable test infrastructure for unimatrix-store and downstream crates.
//!
//! Available within this crate via `#[cfg(test)]` and to downstream crates
//! via the `test-support` feature flag.

use crate::schema::{NewEntry, Status, TimeRange};
use crate::Store;

/// A test database backed by a temporary directory.
///
/// Creates a fresh database on construction and automatically
/// cleans up the temporary directory when dropped.
pub struct TestDb {
    _dir: tempfile::TempDir,
    store: Store,
}

impl TestDb {
    /// Create a new test database in a temporary directory.
    pub fn new() -> Self {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).expect("failed to open test database");
        TestDb { _dir: dir, store }
    }

    /// Get a reference to the store.
    pub fn store(&self) -> &Store {
        &self.store
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
pub fn assert_index_consistent(store: &Store, entry_id: u64) {
    let record = store
        .get(entry_id)
        .unwrap_or_else(|_| panic!("entry {entry_id} should exist in ENTRIES"));

    // Verify TOPIC_INDEX
    let topic_results = store.query_by_topic(&record.topic).unwrap();
    assert!(
        topic_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} not found in TOPIC_INDEX for topic '{}'",
        record.topic
    );

    // Verify CATEGORY_INDEX
    let cat_results = store.query_by_category(&record.category).unwrap();
    assert!(
        cat_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} not found in CATEGORY_INDEX for category '{}'",
        record.category
    );

    // Verify TAG_INDEX
    for tag in &record.tags {
        let tag_results = store.query_by_tags(&[tag.clone()]).unwrap();
        assert!(
            tag_results.iter().any(|r| r.id == entry_id),
            "entry {entry_id} not found in TAG_INDEX for tag '{tag}'"
        );
    }

    // Verify STATUS_INDEX
    let status_results = store.query_by_status(record.status).unwrap();
    assert!(
        status_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} not found in STATUS_INDEX for status {:?}",
        record.status
    );

    // Verify TIME_INDEX
    let time_results = store
        .query_by_time_range(TimeRange {
            start: record.created_at,
            end: record.created_at,
        })
        .unwrap();
    assert!(
        time_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} not found in TIME_INDEX for created_at {}",
        record.created_at
    );
}

/// Verify that an entry is NOT present in index positions for the given old values.
///
/// Used after updates to confirm stale index entries were removed.
pub fn assert_index_absent(
    store: &Store,
    entry_id: u64,
    old_topic: &str,
    old_category: &str,
    old_tags: &[String],
    _old_status: Status,
) {
    // Check topic -- but only if the entry's current topic differs
    let current = store.get(entry_id).ok();
    let current_topic = current.as_ref().map(|r| r.topic.as_str());

    if current_topic != Some(old_topic) {
        let topic_results = store.query_by_topic(old_topic).unwrap();
        assert!(
            !topic_results.iter().any(|r| r.id == entry_id),
            "entry {entry_id} still found in TOPIC_INDEX for old topic '{old_topic}'"
        );
    }

    let current_category = current.as_ref().map(|r| r.category.as_str());
    if current_category != Some(old_category) {
        let cat_results = store.query_by_category(old_category).unwrap();
        assert!(
            !cat_results.iter().any(|r| r.id == entry_id),
            "entry {entry_id} still found in CATEGORY_INDEX for old category '{old_category}'"
        );
    }

    // Check old tags that are not in current tags
    let current_tags: Vec<String> = current
        .as_ref()
        .map(|r| r.tags.clone())
        .unwrap_or_default();
    for tag in old_tags {
        if !current_tags.contains(tag) {
            let tag_results = store.query_by_tags(&[tag.clone()]).unwrap();
            assert!(
                !tag_results.iter().any(|r| r.id == entry_id),
                "entry {entry_id} still found in TAG_INDEX for old tag '{tag}'"
            );
        }
    }
}

/// Seed a database with a deterministic set of entries for testing.
///
/// Returns the IDs of all inserted entries.
pub fn seed_entries(store: &Store, count: usize) -> Vec<u64> {
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
        let id = store.insert(entry).unwrap();
        ids.push(id);
    }
    ids
}
