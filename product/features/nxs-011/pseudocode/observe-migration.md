# Component: unimatrix-observe Migration
## Files: `crates/unimatrix-observe/src/extraction/` (6 files) + `Cargo.toml`

---

## Purpose

Migrates `unimatrix-observe` to compile without `pub use rusqlite` in the store crate
(C-09). Converts `ExtractionRule::evaluate()` to `async fn` across the trait and all 5
extraction rule implementations (ADR-006, Option A). Removes the `spawn_blocking` wrapper
around `run_extraction_rules` in `background.rs`. Rewrites `dead_knowledge.rs` to use
async sqlx queries against `read_pool`.

Must land in the same delivery wave as the store crate (C-09). Once `pub use rusqlite`
is removed from `unimatrix-store/src/lib.rs`, the observe crate will fail to compile unless
this migration is also present.

**Scope: 5 extraction rules affected (not 21). The 21 detection rules (`DetectionRule::detect()`)
are a separate trait that never touches the store and are completely unaffected.**

---

## ExtractionRule Trait (extraction/mod.rs)

```rust
// BEFORE:
pub trait ExtractionRule: Send {
    fn name(&self) -> &str;
    fn evaluate(
        &self,
        observations: &[ObservationRecord],
        store: &Store,
    ) -> Vec<ProposedEntry>;
}

// AFTER (ADR-006, Option A — RPITIT async fn, same approach as ADR-005):
pub trait ExtractionRule: Send {
    fn name(&self) -> &str;  // NOT async — no store access needed

    async fn evaluate(
        &self,
        observations: &[ObservationRecord],
        store: &SqlxStore,
    ) -> Vec<ProposedEntry>;
}
```

The trait becomes non-object-safe (RPITIT, same as `EntryStore`). `Vec<Box<dyn ExtractionRule>>`
is no longer valid. ADR-006 mandates the delivery agent choose one of three dispatch
mechanisms and document it in code comments:

**Recommended approach (ADR-006 preference):** Replace `Vec<Box<dyn ExtractionRule>>` with
an explicit enum over the 5 concrete rule types:

```rust
/// Explicit enum dispatch for ExtractionRule variants.
/// Used instead of Box<dyn ExtractionRule> because ExtractionRule::evaluate is async
/// (RPITIT, Rust 1.89) and thus not object-safe. The extraction rule set is finite
/// and compile-time known — enum dispatch is preferred over dynamic dispatch here
/// (ADR-006 recommendation; consistent with zero-macro async preference).
pub enum ExtractionRuleVariant {
    DeadKnowledge(DeadKnowledgeRule),
    KnowledgeGap(KnowledgeGapRule),
    ImplicitConvention(ImplicitConventionRule),
    RecurringFriction(RecurringFrictionRule),
    FileDependency(FileDependencyRule),
}

impl ExtractionRuleVariant {
    fn name(&self) -> &str {
        match self {
            ExtractionRuleVariant::DeadKnowledge(r) => r.name(),
            ExtractionRuleVariant::KnowledgeGap(r) => r.name(),
            ExtractionRuleVariant::ImplicitConvention(r) => r.name(),
            ExtractionRuleVariant::RecurringFriction(r) => r.name(),
            ExtractionRuleVariant::FileDependency(r) => r.name(),
        }
    }

    async fn evaluate(
        &self,
        observations: &[ObservationRecord],
        store: &SqlxStore,
    ) -> Vec<ProposedEntry> {
        match self {
            ExtractionRuleVariant::DeadKnowledge(r) => r.evaluate(observations, store).await,
            ExtractionRuleVariant::KnowledgeGap(r) => r.evaluate(observations, store).await,
            ExtractionRuleVariant::ImplicitConvention(r) => r.evaluate(observations, store).await,
            ExtractionRuleVariant::RecurringFriction(r) => r.evaluate(observations, store).await,
            ExtractionRuleVariant::FileDependency(r) => r.evaluate(observations, store).await,
        }
    }
}
```

If the delivery agent determines the enum approach is too invasive (e.g., callers hold
`Vec<Box<dyn ExtractionRule>>` in many places), the alternative is to use `async_trait`
macro for `ExtractionRule` specifically. This is the only place in nxs-011 where
`async_trait` is permitted — and only if the enum approach is not chosen. Document the
choice in code comments.

---

## run_extraction_rules (extraction/mod.rs)

```rust
// BEFORE (returned synchronously; called via spawn_blocking from background.rs):
pub fn run_extraction_rules(
    observations: &[ObservationRecord],
    store: &Store,
    rules: &[Box<dyn ExtractionRule>],
) -> Vec<ProposedEntry> {
    rules.iter()
        .flat_map(|rule| rule.evaluate(observations, store))
        .collect()
}

// AFTER (async, called directly from background.rs without spawn_blocking):
pub async fn run_extraction_rules(
    observations: &[ObservationRecord],
    store: &SqlxStore,
    rules: &[ExtractionRuleVariant],  // or Vec<Box<dyn ExtractionRule>> if async_trait chosen
) -> Vec<ProposedEntry> {
    let mut results = Vec::new();
    for rule in rules {
        let mut proposed = rule.evaluate(observations, store).await;
        results.append(&mut proposed);
    }
    results
}
```

Note: The loop is sequential (one rule at a time). If parallel evaluation is desired in
future, `tokio::join!` or `futures::join_all` can be used. For nxs-011, sequential is
correct and preserves existing behavior.

---

## default_extraction_rules (extraction/mod.rs)

```rust
// BEFORE:
pub fn default_extraction_rules() -> Vec<Box<dyn ExtractionRule>> {
    vec![
        Box::new(DeadKnowledgeRule::new()),
        Box::new(KnowledgeGapRule::new()),
        Box::new(ImplicitConventionRule::new()),
        Box::new(RecurringFrictionRule::new()),
        Box::new(FileDependencyRule::new()),
    ]
}

// AFTER (using enum variant):
pub fn default_extraction_rules() -> Vec<ExtractionRuleVariant> {
    vec![
        ExtractionRuleVariant::DeadKnowledge(DeadKnowledgeRule::new()),
        ExtractionRuleVariant::KnowledgeGap(KnowledgeGapRule::new()),
        ExtractionRuleVariant::ImplicitConvention(ImplicitConventionRule::new()),
        ExtractionRuleVariant::RecurringFriction(RecurringFrictionRule::new()),
        ExtractionRuleVariant::FileDependency(FileDependencyRule::new()),
    ]
}
```

---

## dead_knowledge.rs (the only rule with real logic changes)

```rust
// File: crates/unimatrix-observe/src/extraction/dead_knowledge.rs

pub struct DeadKnowledgeRule { /* unchanged */ }

impl ExtractionRule for DeadKnowledgeRule {
    fn name(&self) -> &str { "dead_knowledge" }

    // BEFORE (sync, lock_conn + rusqlite):
    // fn evaluate(&self, observations: &[ObservationRecord], store: &Store) -> Vec<ProposedEntry>

    // AFTER (async, sqlx read_pool):
    async fn evaluate(
        &self,
        observations: &[ObservationRecord],
        store: &SqlxStore,
    ) -> Vec<ProposedEntry> {
        // Query entries that have been accessed but may be stale/unused.
        let entries = match query_accessed_active_entries(store).await {
            Ok(entries) => entries,
            Err(e) => {
                tracing::error!(error = %e, "dead_knowledge: failed to query entries");
                return Vec::new();
            }
        };

        // Existing detection logic is unchanged — only the data source changes.
        self.apply_detection_logic(observations, &entries)
    }
}

/// Async replacement for the rusqlite lock_conn() query.
/// BEFORE: store.lock_conn() + rusqlite::params![Status::Active as i64]
/// AFTER: sqlx::query!() on read_pool
async fn query_accessed_active_entries(
    store: &SqlxStore,
) -> Result<Vec<(u64, String, u32)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT id, title, access_count FROM entries
         WHERE status = ?1 AND access_count > 0",
        Status::Active as i64
    )
    .fetch_all(&store.read_pool)
    .await?;

    Ok(rows.into_iter()
        .map(|r| (r.id as u64, r.title, r.access_count as u32))
        .collect())
}
```

Note: `store.read_pool` must be accessible. If `read_pool` is private to `SqlxStore`,
expose a `pub(crate) fn read_pool(&self) -> &SqlitePool` accessor, or make the field
`pub(crate)`. The observe crate is in a different crate, so either the accessor must be
`pub` or a public async method on `SqlxStore` must be used for this query. The cleanest
approach is to add a dedicated method on `SqlxStore`:

```rust
// In SqlxStore (unimatrix-store/src/db.rs or a new read method):
pub async fn query_accessed_active_entries(&self) -> Result<Vec<(u64, String, u32)>, StoreError> {
    // ... same query as above, using self.read_pool ...
}
```

Then `dead_knowledge.rs` calls `store.query_accessed_active_entries().await`.

This keeps read_pool private to the store crate and exposes a semantically named method.

---

## knowledge_gap.rs, implicit_convention.rs, recurring_friction.rs, file_dependency.rs

These 4 rules receive only the mechanical `async` signature change. No body changes:

```rust
// BEFORE:
fn evaluate(
    &self,
    observations: &[ObservationRecord],
    _store: &Store,  // unused
) -> Vec<ProposedEntry> {
    // existing logic, no store access
}

// AFTER:
async fn evaluate(
    &self,
    observations: &[ObservationRecord],
    _store: &SqlxStore,  // still unused; type changes
) -> Vec<ProposedEntry> {
    // existing logic unchanged
}
```

---

## background.rs call site (server crate)

```rust
// BEFORE (in unimatrix-server/src/background.rs):
let proposed = tokio::task::spawn_blocking({
    let store = Arc::clone(&store);
    let observations = observations.clone();
    let rules = default_extraction_rules();
    move || {
        run_extraction_rules(&observations, &store, &rules)
    }
}).await
.unwrap_or_default();

// AFTER:
let proposed = run_extraction_rules(
    &observations,
    &store,
    &default_extraction_rules(),
).await;
```

The `spawn_blocking` wrapper is removed entirely. The call site becomes a direct `.await`.
This satisfies ADR-006's requirement to remove the `spawn_blocking` debt from `background.rs`.

---

## Cargo.toml Change (unimatrix-observe)

```toml
# BEFORE:
[dependencies]
unimatrix_store = { path = "../unimatrix-store" }
rusqlite = { workspace = true }   # Remove this

# AFTER:
[dependencies]
unimatrix_store = { path = "../unimatrix-store" }
# rusqlite removed; observe now uses SqlxStore from unimatrix-store
```

---

## Error Handling

- `query_accessed_active_entries`: maps `sqlx::Error` to `StoreError::Database` (or returns
  the `sqlx::Error` directly if the public method is on `SqlxStore`). On error: `evaluate`
  logs at ERROR and returns an empty `Vec<ProposedEntry>` — safe degradation, not a crash.
- `run_extraction_rules`: no error return type; errors are handled within each rule's
  `evaluate()`. The runner collects only the proposals that succeed.

---

## Key Test Scenarios

1. **`test_dead_knowledge_evaluate_from_async_context`** (R-08): `#[tokio::test]` — create
   a `SqlxStore` with test entries, call `DeadKnowledgeRule::evaluate().await` directly;
   assert no panic ("cannot start a runtime from within a runtime"). (AC, R-08 scenario 2)

2. **`test_run_extraction_rules_async`**: Call `run_extraction_rules().await` from a
   `#[tokio::test]`; assert all 5 rules execute without panic.

3. **`test_dead_knowledge_query_returns_active_entries`**: Populate store with active
   entries (some accessed, some not); call `query_accessed_active_entries`; assert only
   entries with `access_count > 0` are returned.

4. **`test_other_4_rules_still_compile_and_run`**: Each of the 4 non-dead-knowledge rules
   compiles with the new `async fn evaluate` signature; `evaluate().await` returns without
   panic even with an empty observation list.

5. **`test_no_rusqlite_in_observe_crate`**: CI check —
   `grep -r "rusqlite" crates/unimatrix-observe/` returns zero matches.

6. **`test_observe_background_task_no_spawn_blocking`** (R-08 scenario 1): Run full server
   integration test with observations pipeline active; assert zero "spawn_blocking" calls
   in observe crate; assert no panic.

7. **`test_extraction_rules_send_bound`** (R-07): Each `ExtractionRuleVariant` must be
   `Send` to be used in the async background task. Add compile-time check:
   `fn assert_send<T: Send>(_: T) {}` called with `ExtractionRuleVariant::DeadKnowledge(...)`.

---

## OQ-DURING Items Affecting This Component

- **ADR-006 dispatch mechanism**: Delivery agent must choose between enum dispatch
  (recommended) and `async_trait` for `ExtractionRule`. Document the choice in a code
  comment at the top of `extraction/mod.rs`. This is the only remaining delivery-level
  decision in this component.
