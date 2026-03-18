# Scope Risk Assessment: nxs-011

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | 312 rusqlite call sites migrated in one sweep — any missed site compiles and silently routes through a stale API | High | Med | Architect: enforce a `#[deny]` or Cargo feature flag that makes `rusqlite` a hard compile error in store/server after migration; do not rely on grep audits alone |
| SR-02 | Rust 1.89 native `async fn` in traits (RPITIT) confirmed stable; however, trait impls using `async fn` shed dyn-dispatch — any future `dyn EntryStore` usage pattern fails at compile time with a non-obvious error | Med | Low | Architect: document the dyn-dispatch constraint in the trait definition; add a compile-time test asserting the trait is NOT object-safe (expectation, not a bug) |
| SR-03 | `sqlx-data.json` becomes a required committed artefact; developers who run `cargo build` without regenerating after schema changes get confusing compile errors, not runtime errors | Med | High | Spec: mandate `cargo sqlx prepare` in the schema-change checklist; CI must fail with a human-readable message on stale cache, not a cryptic macro error |
| SR-04 | `migration.rs` (983 lines) adapted to run on a sqlx connection — adaptation is invasive and the migration logic has never been tested against sqlx; a regression silently corrupts schema version state | High | Med | Architect: design migration to run on a dedicated non-pooled `SqliteConnection` opened and closed before pool construction; spec must include a migration regression test harness covering all 12 schema versions |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | `AsyncVectorStore` and `AsyncEmbedService` disposition is unresolved (SCOPE.md §Non-Goals, Q4 deferred); during implementation the temptation to "clean up while we're here" is high | Med | High | Spec: add an explicit constraint — `async_wrappers.rs` non-DB wrappers are untouched; any removal requires a separate scope approval |
| SR-06 | Wave 1 features (NLI, graph edges, confidence weights) will add `AnalyticsWrite` enum variants; if the enum is defined in the store crate without a sealed extension pattern, every Wave 1 addition breaks the match exhaustiveness in the drain task | Med | Med | Architect: design `AnalyticsWrite` as extensible from the start (e.g., a `Custom` variant or non-exhaustive enum); spec this as a constraint |
| SR-07 | `unimatrix-observe` direct rusqlite use (`dead_knowledge.rs`) must migrate alongside the store; if this is treated as a separate delivery wave, the crate will fail to compile the moment `pub use rusqlite` is removed | High | Low | Architect: treat observe-crate migration as part of the same delivery wave, not a follow-on; acceptance criteria must include an observe-crate compile check |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | Analytics queue shed policy silently drops writes under load; no upstream caller is notified; `co_access`, `outcome_index`, and `sessions` data can go missing without any observable signal beyond a WARN log (entry #1542 pattern: define error semantics for background writers before implementation) | High | Med | Spec: define which tables' loss is acceptable under shed and which is not; architect should consider a shed counter in `context_status` output so operators can observe data loss |
| SR-09 | Store-owned drain task (resolved: started in `Store::open()`) creates a tokio task that outlives the Store drop in test contexts where the tokio runtime is short-lived; test isolation is broken silently if the drain task holds a write_pool connection open | Med | High | Architect: design `Store::drop()` to send a shutdown signal to the drain task and await its completion (or a timeout); test helpers must use this path |
| SR-10 | 1,445 sync tests converted to `#[tokio::test]` — mechanical but error-prone; tests that previously relied on synchronous drop ordering (e.g., transaction rollback on `?`) may silently change behavior in async contexts (entry #771 directly relevant) | Med | High | Spec: require a pre-migration test count baseline and a post-migration count check (AC-14 already scopes this); architect should flag any test that uses `?` inside a transaction body for manual review |

## Assumptions

- **§Goals Goal 7 (backend abstraction)**: Assumes sqlx's SQLite and PostgreSQL query APIs are sufficiently compatible that no query uses a SQLite-only SQL dialect feature. If any of the 312 existing queries uses `INSERT OR REPLACE`, `ON CONFLICT DO UPDATE`, or SQLite-specific `PRAGMA` in a query string, the backend abstraction claim does not hold without query rewrites.
- **§Constraints Constraint 2**: Assumes `cargo sqlx prepare --workspace` produces a single `sqlx-data.json` covering all crates. If per-crate files are required, the CI workflow is more complex than a single env var.
- **§Proposed Approach Phase 1**: Assumes `SqliteConnectOptions` applies PRAGMAs per connection reliably across the pool; if any PRAGMA (e.g., `journal_mode=WAL`) is connection-scoped in sqlx's connection lifecycle, pool creation order matters.

## Design Recommendations

- **SR-01 + SR-04**: Architect should sequence the migration as: (1) migrate `migration.rs` and open logic first on a dedicated connection, (2) migrate store crate methods, (3) remove `pub use rusqlite` as the last step — removing it triggers a compile-time audit of every remaining rusqlite import automatically.
- **SR-08**: Spec writer should add an acceptance criterion for the shed counter being exposed via `context_status`; silently dropping analytics writes without an operator-visible count makes the queue unobservable in production (product vision W0-1 explicitly calls out observability of shed events via AC-15, but a cumulative counter is stronger).
- **SR-09**: Architect must define drain task shutdown protocol before spec; if drain task shutdown is undefined, every integration test that opens and closes a Store risks a panic from a background task accessing a closed pool.
