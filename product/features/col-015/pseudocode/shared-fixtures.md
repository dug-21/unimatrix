# Pseudocode: shared-fixtures

## Purpose

Provide reusable test infrastructure for pipeline validation across crates.

## File: src/test_scenarios.rs

Feature-gated: `#[cfg(any(test, feature = "test-support"))]`

### Types

```
struct EntryProfile {
    label: &'static str,
    status: Status,
    access_count: u32,
    last_accessed_at: u64,
    created_at: u64,
    helpful_count: u32,
    unhelpful_count: u32,
    correction_count: u32,
    trust_source: &'static str,
    category: &'static str,
}

struct CalibrationScenario {
    name: &'static str,
    description: &'static str,
    entries: Vec<EntryProfile>,
    now: u64,
    expected_ordering: Vec<usize>,  // indices into entries, best-first
}

struct RetrievalEntry {
    profile: EntryProfile,
    title: &'static str,
    content: &'static str,
    embedding: Option<Vec<f32>>,
    superseded_by: Option<usize>,
}

struct RetrievalScenario {
    name: &'static str,
    description: &'static str,
    entries: Vec<RetrievalEntry>,
    query: &'static str,
    expected_top_k: Vec<usize>,
    pairwise_assertions: Vec<(usize, usize)>,
}
```

### Constants

```
CANONICAL_NOW: u64 = 1_700_000_000
```

### Functions

```
fn profile_to_entry_record(profile: &EntryProfile, id: u64, now: u64) -> EntryRecord
    // Create EntryRecord with profile fields + defaults for non-signal fields
    // id, title=profile.label, content="", topic="test", category=profile.category
    // All timestamps from profile, zero defaults for unset fields

fn kendall_tau(ranking_a: &[u64], ranking_b: &[u64]) -> f64
    // Build position maps for both rankings
    // Count concordant and discordant pairs
    // Handle n <= 1 -> return 1.0
    // Return (C - D) / (n * (n-1) / 2)

fn assert_ranked_above(results: &[(u64, f64)], higher_id: u64, lower_id: u64)
    // Find positions of both IDs, panic with descriptive message if wrong order

fn assert_in_top_k(results: &[(u64, f64)], entry_id: u64, k: usize)
    // Check entry_id in first k results, panic with positions if not

fn assert_tau_above(ranking_a: &[u64], ranking_b: &[u64], min_tau: f64)
    // Compute tau, panic with actual value if below threshold

fn assert_confidence_ordering(entries: &[EntryRecord], expected_order: &[u64], now: u64)
    // Compute confidence for each, sort descending, compare ID order
```

### Standard Profiles (5)

```
fn expert_human_fresh() -> EntryProfile     // Active, 30 access, recent, 10/1 votes, human
fn good_agent_entry() -> EntryProfile       // Active, 15 access, 3 days old, 5/1 votes, agent
fn auto_extracted_new() -> EntryProfile     // Proposed, 2 access, very recent, 0/0, auto
fn stale_deprecated() -> EntryProfile       // Deprecated, 10 access, 90 days stale, 3/3, human
fn quarantined_bad() -> EntryProfile        // Quarantined, 1 access, 30 days, 1/8, unknown
```

### Standard Scenarios (3)

```
fn standard_ranking() -> CalibrationScenario
    // All 5 profiles, expected: expert > good_agent > auto_new > stale > quarantined

fn trust_source_ordering() -> CalibrationScenario
    // Same base profile but varying trust_source: human > system > agent > neural > auto

fn freshness_dominance() -> CalibrationScenario
    // Same base profile but varying freshness: now > 1day > 1week > 1month > 1year
```

## Cargo.toml Changes

```toml
[features]
test-support = []

[dev-dependencies]
unimatrix-engine = { path = ".", features = ["test-support"] }
```

## lib.rs Changes

```rust
#[cfg(any(test, feature = "test-support"))]
pub mod test_scenarios;
```

## Unit Tests (in test_scenarios.rs)

- T-KT-01: kendall_tau identical -> 1.0
- T-KT-02: kendall_tau reversed -> -1.0
- T-KT-03: kendall_tau known partial correlation
- T-KT-04: kendall_tau single element -> 1.0
- T-KT-05: kendall_tau two elements both orderings
- T-PROF-01: round-trip profile -> EntryRecord -> compute_confidence matches expected
- T-PROF-02: all 5 standard profiles produce distinct confidence values
