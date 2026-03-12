## ADR-003: Deterministic JSON Key Ordering via Sequential Map Insertion

### Context

AC-14 requires byte-identical output for the same database state. JSON objects are unordered by spec, but the serialized byte representation depends on key iteration order. If keys are iterated in a non-deterministic order, two exports of the same database produce different byte streams (SR-05 from the Scope Risk Assessment).

`serde_json::Map<String, Value>` is backed by either `BTreeMap` (default) or `IndexMap` (with the `preserve_order` feature). The choice affects key ordering:

1. **BTreeMap (default serde_json)**: Keys are iterated in lexicographic order. Deterministic, but the output key order is alphabetical (`_table`, `access_count`, `category`, `confidence`, ...) which does not match the natural SQL column order. Harder to read.

2. **IndexMap (serde_json `preserve_order` feature)**: Keys are iterated in insertion order. Deterministic if insertion order is consistent. The output matches the natural column order, which is more readable and matches the SQL SELECT order.

3. **Custom struct with `#[derive(Serialize)]`**: A per-table struct with fields in the desired order. Deterministic via field declaration order. But requires defining a struct per table, conflicting with ADR-002's decision to use explicit `Value` construction.

### Decision

Use `serde_json::Map` with sequential insertion in a fixed order matching the SQL column list. The `_table` discriminator is inserted first, followed by columns in their SQL declaration order.

Since `serde_json::Map` defaults to `BTreeMap` (lexicographic order) when the `preserve_order` feature is not enabled, and we want column-natural order for readability, we enable the `preserve_order` feature on `serde_json` in `unimatrix-server`'s `Cargo.toml`:

```toml
serde_json = { version = "1", features = ["preserve_order"] }
```

With `preserve_order`, `serde_json::Map` is backed by `IndexMap`, which preserves insertion order. Since all per-table functions insert keys in the same hardcoded order on every invocation, the output is deterministic.

If enabling `preserve_order` has undesirable effects on other serde_json usage in the crate, the alternative is to use the default BTreeMap-backed Map and accept lexicographic key order. Both are deterministic; the choice is about readability. The implementation agent should verify that `preserve_order` does not break any existing tests before committing to this approach. If it does, fall back to default BTreeMap ordering.

### Consequences

- **Positive**: Byte-identical output for the same database state (AC-14).
- **Positive**: JSON key order matches the natural SQL column order, making the export human-readable and inspectable.
- **Positive**: No additional dependencies — `preserve_order` is a feature of the already-present `serde_json` crate.
- **Negative**: Enabling `preserve_order` changes `serde_json::Map` globally for the crate. All other code in `unimatrix-server` that uses `serde_json::Map` will also get insertion-order semantics. This is generally harmless (insertion order is a superset of "unordered") but should be verified.
- **Negative**: If `preserve_order` is not viable, falling back to BTreeMap ordering means keys appear alphabetically, which is less readable but still deterministic.
