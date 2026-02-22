# C9: Test Infrastructure Pseudocode

## Purpose

Reusable test helpers for this crate and downstream crates. Provides TestDb lifecycle, TestEntry builder, and assertion helpers.

## Module: test_helpers.rs (conditionally compiled)

Accessible via `#[cfg(test)]` within the crate and via `test-support` feature flag for downstream.

```rust
#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers { ... }
```

### TestDb

```
struct TestDb {
    _dir: tempfile::TempDir,  // kept alive for RAII cleanup
    store: Store,
}

impl TestDb:
    fn new() -> Self:
        let dir = tempfile::TempDir::new().unwrap()
        let path = dir.path().join("test.redb")
        let store = Store::open(&path).unwrap()
        TestDb { _dir: dir, store }

    fn store(&self) -> &Store:
        &self.store
```

Drop is automatic via TempDir's Drop impl (removes directory).

### TestEntry Builder

```
struct TestEntry {
    title: Option<String>,
    content: String,
    topic: String,
    category: String,
    tags: Vec<String>,
    source: String,
    status: Status,
}

impl TestEntry:
    fn new(topic: &str, category: &str) -> Self:
        Self {
            title: None,
            content: format!("Content for {topic}/{category}"),
            topic: topic.to_string(),
            category: category.to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
        }

    fn with_title(mut self, title: &str) -> Self:
        self.title = Some(title.to_string())
        self

    fn with_content(mut self, content: &str) -> Self:
        self.content = content.to_string()
        self

    fn with_tags(mut self, tags: &[&str]) -> Self:
        self.tags = tags.iter().map(|s| s.to_string()).collect()
        self

    fn with_status(mut self, status: Status) -> Self:
        self.status = status
        self

    fn with_source(mut self, source: &str) -> Self:
        self.source = source.to_string()
        self

    fn build(self) -> NewEntry:
        NewEntry {
            title: self.title.unwrap_or_else(|| format!("Test: {}/{}", self.topic, self.category)),
            content: self.content,
            topic: self.topic,
            category: self.category,
            tags: self.tags,
            source: self.source,
            status: self.status,
        }
```

### Assertion Helpers

```
fn assert_index_consistent(store: &Store, entry_id: u64):
    // Get the record from ENTRIES
    let record = store.get(entry_id).expect("entry should exist")

    // Verify TOPIC_INDEX
    let topic_results = store.query_by_topic(&record.topic).unwrap()
    assert!(topic_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} not found in TOPIC_INDEX for topic '{}'", record.topic)

    // Verify CATEGORY_INDEX
    let cat_results = store.query_by_category(&record.category).unwrap()
    assert!(cat_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} not found in CATEGORY_INDEX for category '{}'", record.category)

    // Verify TAG_INDEX (for each tag)
    for tag in &record.tags:
        let tag_results = store.query_by_tags(&[tag.clone()]).unwrap()
        assert!(tag_results.iter().any(|r| r.id == entry_id),
            "entry {entry_id} not found in TAG_INDEX for tag '{tag}'")

    // Verify STATUS_INDEX
    let status_results = store.query_by_status(record.status).unwrap()
    assert!(status_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} not found in STATUS_INDEX for status {:?}", record.status)

    // Verify TIME_INDEX
    let time_results = store.query_by_time_range(TimeRange {
        start: record.created_at,
        end: record.created_at,
    }).unwrap()
    assert!(time_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} not found in TIME_INDEX for created_at {}", record.created_at)


fn assert_index_absent(store: &Store, entry_id: u64, topic: &str, category: &str, tags: &[String], status: Status, created_at: u64):
    // Verify entry is NOT in old index positions
    let topic_results = store.query_by_topic(topic).unwrap()
    assert!(!topic_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} still found in TOPIC_INDEX for old topic '{topic}'")

    let cat_results = store.query_by_category(category).unwrap()
    assert!(!cat_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} still found in CATEGORY_INDEX for old category '{category}'")

    for tag in tags:
        let tag_results = store.query_by_tags(&[tag.clone()]).unwrap()
        assert!(!tag_results.iter().any(|r| r.id == entry_id),
            "entry {entry_id} still found in TAG_INDEX for old tag '{tag}'")

    let status_results = store.query_by_status(status).unwrap()
    assert!(!status_results.iter().any(|r| r.id == entry_id),
        "entry {entry_id} still found in STATUS_INDEX for old status {:?}", status)


fn seed_entries(store: &Store, count: usize) -> Vec<u64>:
    let topics = ["auth", "logging", "database", "api", "testing"]
    let categories = ["convention", "decision", "pattern"]
    let all_tags = ["rust", "error", "async", "testing", "performance"]

    let mut ids = Vec::new()
    for i in 0..count:
        let topic = topics[i % topics.len()]
        let category = categories[i % categories.len()]
        let tags: Vec<&str> = all_tags[..((i % all_tags.len()) + 1)].to_vec()

        let entry = TestEntry::new(topic, category)
            .with_tags(&tags)
            .with_title(&format!("Entry {i}"))
            .build()
        let id = store.insert(entry).unwrap()
        ids.push(id)
    ids
```

## Error Handling

- Test helpers use `.unwrap()` and `.expect()` -- panics are appropriate in tests.
- Assertion helpers produce descriptive panic messages.

## Key Test Scenarios

- AC-19: TestDb creates temp dir, opens store, auto-cleans on drop
- AC-19: TestEntry builder produces valid NewEntry with sensible defaults
- AC-19: assert_index_consistent verifies all indexes after insert
- AC-19: seed_entries populates a database for query testing
