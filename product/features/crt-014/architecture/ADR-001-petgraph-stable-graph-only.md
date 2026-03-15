## ADR-001: petgraph with `stable_graph` feature only

### Context

crt-014 adds `petgraph` to `unimatrix-engine` as the graph algorithm substrate for the supersession DAG. petgraph's default feature set includes `graphmap`, `stable_graph`, `matrix_graph`, and `std`. Multiple feature flags enable different graph representations with different trade-offs. Using all default features increases compile surface and binary size unnecessarily.

`stable_graph` is specifically required because `StableGraph` maintains stable `NodeIndex` values across node removal. When entries are quarantined or deleted, any cached node indices remain valid — this matters for crt-017 (Contradiction Cluster Detection) which will extend the graph infrastructure.

`graphmap` and `matrix_graph` serve different use cases (keyed nodes and dense adjacency matrices respectively) and are not needed for crt-014's directed DAG with integer node weights.

### Decision

Add petgraph with `stable_graph` feature only, disabling all other default features:

```toml
petgraph = { version = "0.8", default-features = false, features = ["stable_graph"] }
```

Document the feature restriction with a comment in `Cargo.toml` explaining the rationale (stable indices for entry lifecycle, avoids compile bloat). Do not enable `serde-1` (no graph persistence needed — graphs are rebuilt from store), `rayon` (no parallel graph algorithms in v1), `dot_parser` or `generate` (visualization is Phase 3).

### Consequences

Easier: Compile surface is minimal. Feature intent is self-documenting via the explicit feature list. Stable node indices survive entry removal (quarantine/delete) without index shifting.

Harder: Future contributors must update `Cargo.toml` intentionally to add features — accidental feature drift is prevented but requires a conscious step when Phase 2/3 needs additional petgraph capabilities.
