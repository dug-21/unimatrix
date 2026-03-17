## ADR-006: Convert ExtractionRule::evaluate() to async fn

### Context

`ExtractionRule` is the trait in `unimatrix-observe` that drives knowledge extraction. Its
`evaluate()` method signature is currently synchronous:

```rust
fn evaluate(&self, observations: &[ObservationRecord], store: &Store) -> Vec<ProposedEntry>;
```

After nxs-011, `Store` (rusqlite + `Mutex<Connection>`) is replaced by `SqlxStore` (sqlx
dual-pool). `SqlxStore` has no synchronous store-access primitives — all methods are `async
fn`. The `store` parameter in `evaluate()` must therefore change type and access pattern.

`dead_knowledge.rs` is the only extraction rule that touches the store directly (calling
`store.lock_conn()` and `rusqlite::params!`). The four other extraction rules
(`knowledge_gap.rs`, `implicit_convention.rs`, `recurring_friction.rs`,
`file_dependency.rs`) accept `_store` but never use it.

Two options were evaluated:

**Option A — Convert `ExtractionRule::evaluate()` to `async fn` across all 5 extraction
rules.** The trait becomes:

```rust
pub trait ExtractionRule: Send {
    fn name(&self) -> &str;
    async fn evaluate(
        &self,
        observations: &[ObservationRecord],
        store: &SqlxStore,
    ) -> Vec<ProposedEntry>;
}
```

`dead_knowledge.rs` rewrites `query_accessed_active_entries` as an async sqlx query. The
other four rules gain `async` on their `evaluate` signature with no other change. The
`run_extraction_rules` runner function in `extraction/mod.rs` becomes `async fn` and
collects results with `.await`. The call site in `background.rs` drops the
`spawn_blocking` wrapper and calls `run_extraction_rules(...).await` directly.

**Option B — `spawn_blocking` at the single call site in `background.rs`.** The trait
stays synchronous. `dead_knowledge.rs` would still need a `block_on` or nested-runtime
bridge to call async store methods from within its sync `evaluate()`. This cannot be
solved by `spawn_blocking` alone: the rule body itself cannot call `.await` unless it is
in an async context or explicitly constructs a new runtime, which introduces nested tokio
runtimes — a known correctness hazard. Option B does not actually solve the problem; it
defers it to a worse location.

Additionally, nxs-011's stated goal is to eliminate `spawn_blocking` from the hot path.
Adding a new `spawn_blocking` for `run_extraction_rules` in `background.rs` would
contradict that goal and leave dead_knowledge.rs unable to perform its query anyway.

The human guidance for this feature is explicit: "I want the cleanest platform because
there is still significant capability we'll be adding on the roadmap." This confirms that
forward-compatibility and platform cleanliness outweigh minimizing change count.

This ADR supersedes the open question documented in the ARCHITECTURE.md Open Questions
section item 1 ("ExtractionRule trait async signature").

### Decision

**Option A: Convert `ExtractionRule::evaluate()` to `async fn` across all 5 extraction
rules using RPITIT (Rust 1.89), consistent with ADR-005's approach to `EntryStore`.**

The `ExtractionRule` trait becomes non-object-safe (same consequence as `EntryStore` after
ADR-005). The `run_extraction_rules` runner becomes async. `default_extraction_rules()`
continues to return `Vec<Box<dyn ExtractionRule>>` — this is permitted because
`Box<dyn ExtractionRule>` dispatch is valid for non-async trait methods like `name()`, but
`evaluate()` must be dispatched via the concrete type or via a manual async vtable. In
practice, the runner iterates the boxed rules and calls `.await` on each; this compiles
correctly under RPITIT because the `evaluate` future is erased at the call site via
`Box<dyn ExtractionRule>` only when all methods are object-safe. Since `async fn` in
traits is not object-safe with RPITIT, the runner must either:

1. Change `Vec<Box<dyn ExtractionRule>>` to a concrete enum of rule variants, OR
2. Use `async-trait` crate for the `ExtractionRule` trait specifically (object-safe async),
   consistent with the downstream dispatch pattern, OR
3. Store rules as `Vec<Arc<dyn ExtractionRule>>` with `async_trait` macro applied.

The delivery agent MUST choose between these three and document the choice in code
comments. Option A of this ADR requires the trait be async; the specific mechanism for
dynamic dispatch is a delivery-level implementation detail. The recommended approach,
consistent with nxs-011's preference for zero-macro async, is to switch `Vec<Box<dyn
ExtractionRule>>` to an explicit enum over the 5 concrete rule types, eliminating the
object-safety concern entirely. This is feasible because the extraction rule set is finite
and known at compile time, unlike the detection rules which are also 21 fixed types.

The `store` parameter type changes from `&Store` (rusqlite) to `&SqlxStore` (sqlx) in all
5 rule signatures and in `run_extraction_rules()`.

The `spawn_blocking` wrapper around `run_extraction_rules` in `background.rs` is removed
entirely. The call site becomes a direct `.await`.

### Consequences

**Delivery scope — files affected:**

| File | Change |
|------|--------|
| `unimatrix-observe/src/extraction/mod.rs` | `ExtractionRule::evaluate` → `async fn`; `run_extraction_rules` → `async fn`; parameter type `&Store` → `&SqlxStore`; dynamic dispatch strategy chosen (enum or async_trait) |
| `unimatrix-observe/src/extraction/dead_knowledge.rs` | `evaluate` → `async fn`; `query_accessed_active_entries` rewritten as async sqlx query; removes `use unimatrix_store::rusqlite` |
| `unimatrix-observe/src/extraction/knowledge_gap.rs` | `evaluate` → `async fn`; parameter renamed from `_store: &Store` to `_store: &SqlxStore`; body unchanged |
| `unimatrix-observe/src/extraction/implicit_convention.rs` | Same minimal change as knowledge_gap.rs |
| `unimatrix-observe/src/extraction/recurring_friction.rs` | Same minimal change as knowledge_gap.rs |
| `unimatrix-observe/src/extraction/file_dependency.rs` | Same minimal change as knowledge_gap.rs |
| `unimatrix-server/src/background.rs` | Remove `spawn_blocking` wrapper on `run_extraction_rules`; call becomes `.await` |

**Total extraction rule implementations affected: 5** (not 21 — the 21 detection rules use
`DetectionRule::detect()`, a separate trait in `detection/mod.rs` that does not touch the
store and is unaffected by nxs-011).

**What becomes easier:**
- Future extraction rules that need async store access can implement it directly without
  workarounds.
- `spawn_blocking` debt in `background.rs` is reduced by one more site.
- `dead_knowledge.rs` can use idiomatic sqlx queries instead of a locked connection guard.
- The roadmap's Wave 1 features (NLI, graph edges) can add extraction rules that do
  async work without needing a different trait or bridge pattern.

**What becomes harder:**
- If `Vec<Box<dyn ExtractionRule>>` is replaced by an enum, adding a new extraction rule
  requires updating the enum. This is acceptable — the extraction rule set is small and
  deliberate.
- Tests for `DeadKnowledgeRule` in `dead_knowledge.rs` that currently use synchronous
  `Store` must be rewritten for async (using `#[tokio::test]` and `SqlxStore`).
- The 4 extraction rules that did not previously need async gain the async keyword on
  their `evaluate()` method, which is a mechanical but non-trivial diff to verify.
