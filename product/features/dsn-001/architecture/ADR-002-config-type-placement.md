## ADR-002: Config Type Placement — unimatrix-server owns UnimatrixConfig

### Context

`UnimatrixConfig` must be defined somewhere in the crate graph. Three candidates exist:

**Option A — `unimatrix-server`**: Config is a server-startup concern. The only
code that loads a TOML file and applies config to subsystems is in `main.rs` and
the server's own infra modules. `unimatrix-server` already depends on `toml` (it
will after this feature); no other crate needs to know the config shape.

**Option B — `unimatrix-core`**: Core is a shared-types crate. Placing config there
avoids duplication if multiple crates need it. However, `unimatrix-core` currently
has no TOML parsing dependency and no I/O; adding `toml` to its dependency graph
would contaminate the pure computation crates (`unimatrix-engine`, `unimatrix-store`)
that depend on `unimatrix-core`.

**Option C — new `unimatrix-config` crate**: Thin shared crate containing only
`UnimatrixConfig` and sub-structs. Eliminates the contamination risk. Adds a new
crate to the workspace for ~100 lines of struct definitions and `Default` impls —
a significant overhead-to-value ratio.

SR-08 from the risk assessment explicitly names this problem: `session_capabilities`
is in `unimatrix-store/registry.rs`, while `PERMISSIVE_AUTO_ENROLL` is in
`unimatrix-server/infra/registry.rs`. The risk says: "Architect should define config
types in a thin `unimatrix-config` crate or in `unimatrix-core` to avoid a circular
dependency. Alternatively, pass capabilities as plain values, not as `Arc<UnimatrixConfig>`."

The critical constraint is that `unimatrix-store` must not depend on
`unimatrix-server` types. An `Arc<UnimatrixConfig>` cannot be passed from server to
store because that would create a dependency cycle: `unimatrix-server` depends on
`unimatrix-store`; `unimatrix-store` cannot depend on `unimatrix-server`.

OQ-03 from the spec writer confirms: pass plain `Vec<Capability>` values across the
crate boundary rather than `Arc<UnimatrixConfig>`.

The only config values that cross into `unimatrix-store` are:
1. `permissive: bool` — passed to `agent_resolve_or_enroll(agent_id, permissive)`
   (already a parameter, not a constant)
2. `session_capabilities: Vec<Capability>` — passed to the store as resolved values

Both are already expressible as primitive parameters on existing store methods.

### Decision

`UnimatrixConfig` and its sub-structs (`KnowledgeConfig`, `ServerConfig`,
`AgentsConfig`, `ConfidenceConfig`, `CycleConfig`) live in
`crates/unimatrix-server/src/infra/config.rs`.

The `toml` crate is added only to `unimatrix-server/Cargo.toml`.

The boundary rule: **no `Arc<UnimatrixConfig>` crosses any crate boundary**. When
config values are needed in `unimatrix-store` or `unimatrix-engine`, the server
extracts the concrete value and passes it as a plain parameter:

| Config value | Boundary crossing | Mechanism |
|---|---|---|
| `knowledge.categories` | server → `CategoryAllowlist` | `Vec<String>` at construction |
| `knowledge.boosted_categories` | server → `SearchService` | `HashSet<String>` field |
| `knowledge.freshness_half_life_hours` | server → engine | `ConfidenceParams` field (ADR-001) |
| `agents.default_trust` | server → `AgentRegistry` | `bool` (permissive flag) |
| `agents.session_capabilities` | server → store | `Vec<Capability>` parameter |
| `server.instructions` | server-internal | Stays in `UnimatrixServer::new()` |

`CategoryAllowlist::new()` delegates to `CategoryAllowlist::from_categories(Vec<String>)`
which replaces the hardcoded `INITIAL_CATEGORIES`. The existing `new()` calls
`from_categories(INITIAL_CATEGORIES.to_vec())` so all existing tests remain valid
without modification (SR-07 resolved).

### Consequences

**Easier:**
- No new crate, no workspace overhead.
- `unimatrix-store` dependency graph is unchanged — no risk of contaminating the
  pure storage layer with config-parsing logic.
- The server crate already owns `AgentRegistry` server-side logic; owning the config
  types that drive it is cohesive.
- The `from_categories` constructor pattern unifies the test and production code
  paths (SR-07: `new()` becomes a thin wrapper over `from_categories`).

**Harder:**
- If a future crate outside `unimatrix-server` needs to apply config (e.g., a
  hypothetical `unimatrix-daemon` binary crate), it will need to either re-export
  config types or accept them as parameters. This is a manageable future refactor.
- Integration tests that construct server subsystems directly must use
  `CategoryAllowlist::new()` (the defaults path) or supply a `Vec<String>` — the
  config struct itself is not accessible from `unimatrix-store` test code.
