# SPECIFICATION: dsn-001 — Config Externalization (W0-3)

## Objective

Unimatrix is a domain-agnostic knowledge engine whose behavior is currently hard-coupled to software delivery vocabulary through compiled constants. This feature externalizes four groups of runtime constants to a two-level TOML configuration system (`~/.unimatrix/config.toml` global, `{data_dir}/config.toml` per-project), validated at startup with security-critical checks that abort on violation. It also performs a hardcoded rename of the `context_retrospective` MCP tool to `context_cycle_review` and neutralises the `CycleParams.topic` field documentation to remove domain-specific vocabulary.

---

## Functional Requirements

### FR-001: Global config file loading

The server MUST attempt to load `~/.unimatrix/config.toml` at startup. If the file does not exist, the server MUST start successfully using all compiled defaults. If the file is present and valid, settings MUST be applied before any subsystem is constructed and before any request is handled.

### FR-002: Per-project config file loading

The server MUST attempt to load `{data_dir}/config.toml` (where `data_dir` is `~/.unimatrix/{sha256(canonical_project_root)[..16]}/`) as a second config layer. If the file does not exist, global config values apply. If the file is present and valid, per-project values MUST shadow the corresponding global values.

### FR-003: Merge semantics

**Scalar fields** (strings, numbers, booleans): per-project value replaces global value. If absent from per-project config, the global value (or compiled default) is used.

**List fields** (`categories`, `boosted_categories`, `session_capabilities`): a per-project list MUST entirely replace the corresponding global list. List fields are never appended or merged. Absence in the per-project config means the global list (or compiled default) is used unchanged.

### FR-004: Config load insertion point

Config MUST be loaded in `tokio_main_daemon` and `tokio_main_stdio` only. The load call MUST occur after `project::ensure_data_directory()` returns `paths` (since `paths.data_dir` is required for per-project config path resolution) and before any subsystem construction. `tokio_main_bridge`, `Command::Hook`, and export/import subcommands MUST NOT load config.

### FR-005: Externalize `[knowledge]` section

Config MUST support the following `[knowledge]` section fields:

| Field | Type | Compiled default | Description |
|-------|------|-----------------|-------------|
| `categories` | `Vec<String>` | `["outcome","lesson-learned","decision","convention","pattern","procedure","duties","reference"]` | Domain category allowlist. Replaces `INITIAL_CATEGORIES`. |
| `boosted_categories` | `Vec<String>` | `["lesson-learned"]` | Categories receiving the provenance boost in search re-ranking. |
| `freshness_half_life_hours` | `f64` | `168.0` | Freshness decay half-life in hours. Replaces `FRESHNESS_HALF_LIFE_HOURS`. |

### FR-006: Externalize `[server]` section

Config MUST support a `[server]` section with a single field:

| Field | Type | Compiled default | Description |
|-------|------|-----------------|-------------|
| `instructions` | `String` | Current `SERVER_INSTRUCTIONS` const | MCP server instructions returned during the initialize handshake. |

### FR-007: Externalize `[agents]` section

Config MUST support the following `[agents]` section fields:

| Field | Type | Compiled default | Description |
|-------|------|-----------------|-------------|
| `default_trust` | `String` | `"permissive"` | Auto-enroll mode. `"permissive"` grants `[Read, Write, Search]`; `"strict"` grants `[Read, Search]`. |
| `session_capabilities` | `Vec<String>` | `["Read","Write","Search"]` (permissive) | Explicit capability set for auto-enrolled agents. Overrides the `default_trust` capability derivation. |

### FR-008: Reserve `[confidence]` section

`UnimatrixConfig` MUST include a `ConfidenceConfig` sub-struct with no fields and a `Default` impl. The `[confidence]` TOML section MUST be reserved and parseable (empty or absent) so that W3-1 can add fields without a config format break. No field in this section is consumed by W0-3.

### FR-009: Reserve `[cycle]` section

`UnimatrixConfig` MUST include a `CycleConfig` sub-struct with no fields and a `Default` impl. The `[cycle]` TOML section MUST be reserved and parseable (empty or absent) so that future features can add fields without a config format break. No field in this section is consumed by W0-3.

### FR-010: File size enforcement

Before passing any config file to the TOML parser, the server MUST read at most 64 KB (65536 bytes). If the file exceeds this limit, startup MUST abort with an error identifying the file path and the size violation.

### FR-011: File permission enforcement (Unix only)

On Unix systems (`#[cfg(unix)]`), the server MUST check file permissions on each config file it finds using `std::fs::metadata()` (follows symlinks):

- World-writable (`mode & 0o002 != 0`): startup MUST abort with an error naming the file path and the violation.
- Group-writable (`mode & 0o020 != 0`): the server MUST log a warning (tracing `warn!`) and continue startup.

This check applies independently to both the global and per-project config files. On non-Unix systems, these checks are skipped entirely.

### FR-012: Post-parse field validation

Immediately after deserialization, `validate_config()` MUST enforce all constraints listed in the Security Model section. Any single violation MUST abort startup with a descriptive error. No field silently falls back to a default after a violation is detected.

### FR-013: CategoryAllowlist seeding from config

`CategoryAllowlist` MUST be constructed using the `categories` list from config (after validation). The existing `add_category()` method MUST be called in a loop to seed the allowlist. `CategoryAllowlist::new()` MUST continue to use the compiled `INITIAL_CATEGORIES` defaults (so existing tests that call `new()` remain valid). A `new_from_config(config: &KnowledgeConfig)` constructor (or equivalent) is the production path. `new()` MUST delegate to `new_from_config(&Default::default())` so test and production behavior share a single code path.

### FR-014: Boosted-categories runtime replacement

The four hardcoded `entry.category == "lesson-learned"` comparisons in `crates/unimatrix-server/src/services/search.rs` (lines 413, 418, 484, 489) MUST be replaced with a `HashSet<String>` membership test using the loaded `boosted_categories` set. After this change, no literal string `"lesson-learned"` in comparison position MUST remain in `search.rs`.

### FR-015: Freshness half-life parameterization

`FRESHNESS_HALF_LIFE_HOURS` in `crates/unimatrix-engine/src/confidence.rs` MUST NOT be used directly in `freshness_score()` computation. The loaded `freshness_half_life_hours` value MUST be threaded from the server config through the confidence computation call chain as a parameter. The compiled constant may remain as a documentation value but MUST NOT be the source of the runtime value once config is loaded.

### FR-016: Agent auto-enroll config wiring

`PERMISSIVE_AUTO_ENROLL` in `crates/unimatrix-server/src/infra/registry.rs` MUST be replaced by the configured `default_trust` value. `session_capabilities` from config MUST be used to derive the capability list passed to `agent_resolve_or_enroll`. The `permissive: bool` parameter to `SqlxStore::agent_resolve_or_enroll` is the insertion point; the capability list it selects MUST reflect the config value.

### FR-017: Server instructions config wiring

`SERVER_INSTRUCTIONS` const in `crates/unimatrix-server/src/server.rs` MUST be replaced by the loaded `[server] instructions` value when config is present. The compiled value is the default. `ServerInfo.instructions` at `server.rs:245` is the target field.

### FR-018: `context_retrospective` renamed to `context_cycle_review`

The `#[tool(name = "context_retrospective", ...)]` attribute in `tools.rs` MUST be changed to `#[tool(name = "context_cycle_review", ...)]`. The handler function name `context_retrospective` MUST be renamed to `context_cycle_review`. All other occurrences of `context_retrospective` as a tool name string or caller reference MUST be updated across the entire codebase. This rename is hardcoded, not configurable.

### FR-019: `CycleParams.topic` field doc neutralised

The doc comment on `CycleParams.topic` that currently reads `"Feature cycle identifier (e.g., 'col-022')"` MUST be replaced with language that communicates the domain-agnostic concept: a bounded work unit that a domain tracks, such as a feature, incident, campaign, case, or sprint. The example MUST NOT use software-delivery vocabulary as the only illustration.

### FR-020: `dirs::home_dir()` `None` handling

If `dirs::home_dir()` returns `None`, config loading MUST degrade gracefully: the server starts using compiled defaults, and a tracing `warn!` is emitted noting that the home directory could not be resolved. The server MUST NOT panic or abort startup in this case.

### FR-021: Malformed TOML error handling

If a config file is present, passes size and permission checks, but contains malformed TOML or fails deserialization, startup MUST abort with an error that identifies the file path and the parse error detail.

---

## Non-Functional Requirements

### NFR-001: Startup time impact

Config loading, validation, and subsystem wiring MUST add no more than 5 ms to startup time on an average developer machine (SSD, local filesystem). TOML parsing of a 64 KB file with the `toml 0.8` crate is well under 1 ms; validation is O(n) on category count (≤ 64 entries); ContentScanner regex matching on an 8 KB instructions string is bounded. This requirement is verified by observation, not automated measurement.

### NFR-002: Memory overhead

`Arc<UnimatrixConfig>` shared across subsystems. The maximum memory contribution of config at runtime is bounded by: 64 categories × 64 bytes max each = 4 KB for categories; 8 KB for instructions; negligible for scalar fields. Total config memory overhead MUST NOT exceed 32 KB.

### NFR-003: Default-only startup compatibility

When no config file exists at either path, all compiled defaults apply and all existing tests MUST pass without modification. This is the zero-config contract.

### NFR-004: Platform compatibility

File permission checks MUST be gated with `#[cfg(unix)]`. The config loading path MUST compile and run correctly on Linux and macOS. Windows behavior: size and TOML validation apply; permission checks are skipped.

### NFR-005: Thread safety

`Arc<UnimatrixConfig>` is the sharing mechanism. `UnimatrixConfig` fields are all `Send + Sync` (plain Rust types: `String`, `Vec<String>`, `f64`, `bool`). No `Mutex` is required on the config struct itself; it is immutable after startup.

### NFR-006: `toml` crate version pin

`toml = "0.8"` (exact pin, not caret) MUST be added to `unimatrix-server/Cargo.toml`. No other crate in the workspace currently depends on `toml`; pinning prevents transitive version drift.

---

## Acceptance Criteria

Each criterion carries its ID from SCOPE.md. Verification method is stated for each.

**AC-01** — When no `~/.unimatrix/config.toml` or per-project `config.toml` exists, the server starts with all compiled default values and all existing tests pass without modification.
*Verification*: Run full test suite with no config files present. All tests pass.

**AC-02** — When `[knowledge] categories` is set in config, the loaded list replaces the default 8-category allowlist and `CategoryAllowlist` reflects the new list for all validation calls.
*Verification*: Unit test: construct `CategoryAllowlist` from a config with a custom category list; validate that a category from the custom list passes and a category from the default list fails.

**AC-03** — When `[knowledge] boosted_categories` is set, those categories receive the provenance boost in search re-ranking; no literal `"lesson-learned"` string comparison remains in `search.rs`.
*Verification*: Code review confirms no `== "lesson-learned"` in `search.rs`. Unit test: configure `boosted_categories = ["custom-boost"]`, verify boost applied to that category and not to `"lesson-learned"`.

**AC-04** — When `[knowledge] freshness_half_life_hours` is set, `freshness_score()` uses that value instead of the compiled `168.0` constant.
*Verification*: Unit test: call `freshness_score()` with `half_life = 336.0`; verify that a 336-hour-old entry scores 0.5. No reference to `FRESHNESS_HALF_LIFE_HOURS` constant remains in `freshness_score()` body.

**AC-05** — When `[server] instructions` is set, that string appears in `ServerInfo.instructions` returned during the MCP initialize handshake.
*Verification*: Unit test: construct `UnimatrixServer` with a config containing a custom instructions string; verify `server_info.instructions == Some(custom_string)`.

**AC-06** — When `[agents] default_trust` and `session_capabilities` are set, unknown agents auto-enroll with the configured capabilities.
*Verification*: Unit test: configure `default_trust = "strict"`; call `agent_resolve_or_enroll` for an unknown agent; assert capabilities are `[Read, Search]`. Separate test: configure `session_capabilities = ["Read"]`; assert capabilities are `[Read]`.

**AC-07** — A per-project config at `{data_dir}/config.toml` overrides the global config for all keys it specifies; unspecified keys fall through to global or compiled defaults.
*Verification*: Unit test: global config sets `freshness_half_life_hours = 500.0`; per-project config sets `freshness_half_life_hours = 24.0`; merged config reads `24.0`. Separate test: global sets `categories = ["a"]`; per-project specifies no `categories`; merged config reads `["a"]`.

**AC-08** — A config file with world-writable permissions (`mode & 0o002 != 0`) causes startup to abort with an error message identifying the file path and violation.
*Verification*: Unit test (Unix only): write a temp config file, `chmod 777`; assert `load_config()` returns `Err` containing the file path.

**AC-09** — A config file with group-writable permissions (`mode & 0o020 != 0`) logs a warning but does not abort startup.
*Verification*: Unit test (Unix only): write a temp config file, `chmod 664`; assert `load_config()` returns `Ok` and a warning is emitted (captured via `tracing-test` or equivalent).

**AC-10** — A `[knowledge] categories` entry with characters outside `[a-z0-9_-]`, length > 64, or a total category count > 64 causes startup to abort with a descriptive error.
*Verification*: Unit tests: (a) category with uppercase → `Err`; (b) category with space → `Err`; (c) category of 65 chars → `Err`; (d) 65-element list → `Err`.

**AC-11** — A `[knowledge] boosted_categories` value not present in `[knowledge] categories` causes startup to abort with an error naming the invalid value.
*Verification*: Unit test: `categories = ["a","b"]`, `boosted_categories = ["c"]`; assert `Err` containing `"c"`.

**AC-12** — A `[server] instructions` value matching any injection pattern in `ContentScanner` causes startup to abort with an error identifying the triggering pattern category.
*Verification*: Unit test: set `instructions` to a known injection-triggering string (from `ContentScanner` test corpus); assert `Err`.

**AC-13** — The MCP tool formerly named `context_retrospective` is now named `context_cycle_review`; all callers in protocols, skills, tests, and other files are updated; no occurrence of `context_retrospective` as a tool-name or caller reference remains in the codebase.
*Verification*: `grep -r "context_retrospective" .` returns zero results after the rename (allowing only this specification document and historical feature docs). Integration test in `test_protocol.py` asserts `context_cycle_review` is in the server's tool list.

**AC-14** — The `CycleParams.topic` field doc no longer references "feature" as the primary example; the doc communicates the domain-agnostic concept of a bounded work unit.
*Verification*: Code review of `tools.rs:CycleParams.topic` doc comment.

**AC-15** — A config file exceeding 64 KB causes startup to abort with an error before TOML parsing begins.
*Verification*: Unit test: write a temp file of 65537 bytes; assert `load_config()` returns `Err` before any TOML parse call.

**AC-16** — A `[knowledge] freshness_half_life_hours` value of `0.0`, negative, `NaN`, or `Infinity` causes startup to abort with a descriptive error.
*Verification*: Unit tests: one for each invalid value.

**AC-17** — A `[knowledge] freshness_half_life_hours` value greater than `87600.0` causes startup to abort with a descriptive error.
*Verification*: Unit test: value `87600.1` → `Err`; value `87600.0` → `Ok`.

**AC-18** — A `[agents] default_trust` value other than `"permissive"` or `"strict"` causes startup to abort with an error listing valid values.
*Verification*: Unit test: `default_trust = "open"` → `Err` containing `["permissive","strict"]`.

**AC-19** — A `[agents] session_capabilities` list containing any value other than `"Read"`, `"Write"`, or `"Search"` (including `"Admin"`) causes startup to abort with an error.
*Verification*: Unit test: `session_capabilities = ["Read","Admin"]` → `Err`.

**AC-20** — A `[server] instructions` value exceeding 8 KB causes startup to abort with an error before `ContentScanner` runs.
*Verification*: Unit test: `instructions` of 8193 bytes → `Err` before scanner invocation (verify by confirming error message identifies length, not injection).

**AC-21** — All new validation paths have unit tests. All existing unit and integration tests continue to pass.
*Verification*: `cargo test --workspace` passes with zero failures. New tests cover every new `Err` path in `validate_config()`.

---

## Domain Models

### `UnimatrixConfig`

The top-level deserialization target. All sub-structs derive `serde::Deserialize` and `Default`. The `Default` impl for each sub-struct reproduces current hardcoded values exactly.

```
UnimatrixConfig {
    knowledge: KnowledgeConfig,    // [knowledge] section
    server:    ServerConfig,       // [server] section
    agents:    AgentsConfig,       // [agents] section
    confidence: ConfidenceConfig,  // [confidence] section — reserved, no fields
    cycle:     CycleConfig,        // [cycle] section — reserved, no fields
}
```

**KnowledgeConfig**

| Field | Rust type | `Default` value |
|-------|-----------|-----------------|
| `categories` | `Vec<String>` | `INITIAL_CATEGORIES` (8 values) |
| `boosted_categories` | `Vec<String>` | `vec!["lesson-learned"]` |
| `freshness_half_life_hours` | `f64` | `168.0` |

**ServerConfig**

| Field | Rust type | `Default` value |
|-------|-----------|-----------------|
| `instructions` | `String` | `SERVER_INSTRUCTIONS` const value |

**AgentsConfig**

| Field | Rust type | `Default` value |
|-------|-----------|-----------------|
| `default_trust` | `String` | `"permissive"` |
| `session_capabilities` | `Vec<String>` | `["Read","Write","Search"]` |

**ConfidenceConfig** — empty struct, no fields, reserved for W3-1.

**CycleConfig** — empty struct, no fields, reserved for future use.

### Validation rules (enforced in `validate_config()`)

| Field path | Constraint | Error on violation |
|-----------|-----------|-------------------|
| `knowledge.categories` each | matches `^[a-z0-9_-]+$`, length ≤ 64 | startup abort |
| `knowledge.categories` total count | ≤ 64 | startup abort |
| `knowledge.boosted_categories` each | must be a member of the validated `categories` set | startup abort, names invalid value |
| `knowledge.freshness_half_life_hours` | finite (`!is_nan() && !is_infinite()`), `> 0.0`, `≤ 87600.0` | startup abort |
| `server.instructions` length | `≤ 8192` bytes (UTF-8 byte length) | startup abort before scan |
| `server.instructions` content | `ContentScanner::global().scan_title()` — no injection match | startup abort, names pattern category |
| `agents.default_trust` | one of `{"permissive","strict"}` (exact, case-sensitive) | startup abort, lists valid values |
| `agents.session_capabilities` each | one of `{"Read","Write","Search"}` (exact, case-sensitive; `Admin` excluded) | startup abort |

### Load order and merge algorithm

```
fn load_config(paths: &ProjectPaths) -> Result<UnimatrixConfig>:
    1. global_config = try_load_file("~/.unimatrix/config.toml")?
       → on absent: UnimatrixConfig::default()
       → on present: size_check → permission_check → toml::from_str → validate_config
    2. project_config = try_load_file("{paths.data_dir}/config.toml")?
       → same pipeline
    3. merged = merge(global_config, project_config)
       scalar fields:  project value replaces global if project file contained the section/field
       list fields:    project list replaces global list if project file contained the section/field
    4. return merged
```

The merge step distinguishes "field not present in file" from "field set to default value" by using `Option<T>` intermediate types during deserialization, then resolving to `T` during merge. The final `UnimatrixConfig` exposes only `T`, not `Option<T>`.

### Ubiquitous language

| Term | Definition |
|------|-----------|
| **Global config** | `~/.unimatrix/config.toml` — applies to all server instances for a user |
| **Per-project config** | `~/.unimatrix/{hash}/config.toml` — applies to one project's server instance |
| **Compiled default** | The value hardcoded in Rust source that applies when no config is present |
| **Category allowlist** | The set of valid category strings for knowledge entries; operator-configurable |
| **Boosted categories** | The subset of categories that receive `PROVENANCE_BOOST` in search re-ranking |
| **Provenance boost** | A fixed additive score (`PROVENANCE_BOOST = 0.02`) applied to entries in boosted categories during re-ranking |
| **Freshness half-life** | Hours after which a knowledge entry's freshness score falls to 0.5 |
| **Permissive mode** | Auto-enroll mode granting `[Read, Write, Search]` to unknown agents |
| **Strict mode** | Auto-enroll mode granting `[Read, Search]` to unknown agents |
| **validate_config** | The function that enforces all security-critical constraints after deserialization |
| **World-writable** | Unix file mode bit `0o002` set; any local user can overwrite the file |
| **Group-writable** | Unix file mode bit `0o020` set; any member of the file's group can overwrite it |

---

## User Workflows

### Workflow 1: Default (no config)

1. Operator installs and runs Unimatrix with no config files present.
2. Server starts; `load_config()` finds no files; `UnimatrixConfig::default()` is used.
3. All existing behavior is preserved identically.

### Workflow 2: Domain customization (global config)

1. Operator creates `~/.unimatrix/config.toml` with domain-specific settings (e.g., legal knowledge base with `freshness_half_life_hours = 8760.0`, custom categories).
2. Server starts; global config is loaded, size-checked, permission-checked, parsed, and validated.
3. `CategoryAllowlist` is seeded from the configured category list; `freshness_score()` uses 8760.0; server instructions reflect operator's text.
4. All subsequent knowledge operations use the domain-specific parameters.

### Workflow 3: Per-project override

1. Operator has a global config with `freshness_half_life_hours = 8760.0`.
2. For a specific project, operator creates `~/.unimatrix/{hash}/config.toml` with `freshness_half_life_hours = 24.0`.
3. Server starts for that project; global config loads first, per-project config loads second; merged config uses `24.0` for this project.
4. Other projects (with no per-project config) continue using `8760.0`.

### Workflow 4: Security violation — startup abort

1. An adversary or misconfigured tool sets a config file to `0o666` (world-writable).
2. Server startup reaches the permission check; detects `mode & 0o002 != 0`.
3. Startup aborts with a message: `"config file {path} is world-writable; refusing to start"`.
4. Operator corrects permissions (`chmod 600`) and restarts.

### Workflow 5: Tool callers after rename

1. Protocol or skill file calls `context_retrospective`.
2. After this feature ships, the MCP tool no longer exists under that name.
3. The rename to `context_cycle_review` must be applied to all caller files in the same PR.
4. Callers use `context_cycle_review` going forward.

---

## SR-05 Exhaustive Rename Checklist: `context_retrospective` → `context_cycle_review`

This checklist covers every location where `context_retrospective` appears as a tool name, caller reference, string literal, or documentation identifier. Build passing is necessary but insufficient — all non-Rust files must be updated in the same PR.

The search `grep -r "context_retrospective" .` was run against the full workspace. Results are grouped by file type and update requirement.

### Rust source files (compiled — build will fail if missed)

| File | Line(s) | What to change |
|------|---------|----------------|
| `crates/unimatrix-server/src/mcp/tools.rs` | `#[tool(name = "context_retrospective", ...)]` attr | Change to `name = "context_cycle_review"` |
| `crates/unimatrix-server/src/mcp/tools.rs` | `async fn context_retrospective(` | Rename function to `context_cycle_review` |
| `crates/unimatrix-server/src/mcp/tools.rs` | `operation: "context_retrospective".to_string()` (audit log, ~line 1457) | Change to `"context_cycle_review"` |
| `crates/unimatrix-server/src/mcp/tools.rs` | `operation: "context_retrospective/lesson-learned".to_string()` (~line 1734) | Change to `"context_cycle_review/lesson-learned"` |
| `crates/unimatrix-server/src/mcp/tools.rs` | `"context_cycle"` description mentions `context_retrospective` ("confirm via context_retrospective", ~line 1505 and ~line 1560) | Update both cross-references to `context_cycle_review` |
| `crates/unimatrix-server/src/mcp/tools.rs` | Doc comment above `RetrospectiveParams` struct: "Parameters for the context_retrospective tool." | Update to `context_cycle_review` |
| `crates/unimatrix-server/src/mcp/tools.rs` | Doc comment on `write_lesson_learned`: "Called inside a tokio::spawn from context_retrospective." (~line 1617) | Update to `context_cycle_review` |
| `crates/unimatrix-server/src/server.rs` | Comment: "Shared with UDS listener; drained by context_retrospective handler." (~line 207) | Update comment |
| `crates/unimatrix-server/src/server.rs` | Comment: "features that complete without calling `context_retrospective` or `context_cycle`." (~line 147) | Update comment |
| `crates/unimatrix-server/src/server.rs` | Comment: "context_retrospective" in `PendingEntriesAnalysis` doc (~line 65) | Update comment |
| `crates/unimatrix-observe/src/types.rs` | Comment: "Complete analysis output returned by context_retrospective." (~line 221) | Update to `context_cycle_review` |
| `crates/unimatrix-observe/src/session_metrics.rs` | `assert_eq!(classify_tool("context_retrospective"), "other")` (~line 601) | Change test string to `"context_cycle_review"` |

### Python integration test files (not compiled — build will not catch)

| File | Line(s) | What to change |
|------|---------|----------------|
| `product/test/infra-001/harness/client.py` | `def context_retrospective(self, ...)` (~line 629) | Rename method to `context_cycle_review` |
| `product/test/infra-001/harness/client.py` | `return self.call_tool("context_retrospective", args, ...)` (~line 642) | Change tool name string to `"context_cycle_review"` |
| `product/test/infra-001/suites/test_tools.py` | Comment `# === context_retrospective (col-002)` (~line 768) | Update comment |
| `product/test/infra-001/suites/test_tools.py` | Comment `# === context_retrospective baseline comparison (col-002b)` (~line 789) | Update comment |
| `product/test/infra-001/suites/test_tools.py` | Comment `# === context_retrospective format dispatch (vnc-011)` (~line 983) | Update comment |
| `product/test/infra-001/suites/test_tools.py` | All `server.context_retrospective(...)` call sites (~lines 773, 779, 785, 893, 897, 935, 939, 966, 996, 1009, 1022) | Rename all to `server.context_cycle_review(...)` |
| `product/test/infra-001/suites/test_tools.py` | Comment "context_retrospective can find them via SqlObservationSource" (~line 814) | Update comment |
| `product/test/infra-001/suites/test_protocol.py` | `"context_retrospective"` in expected tool list (~line 55) | Change to `"context_cycle_review"` |

### Protocol and skill files (not compiled — build will not catch)

| File | Line(s) | What to change |
|------|---------|----------------|
| `.claude/protocols/uni/uni-agent-routing.md` | "Data gathering (context_retrospective + artifact review)" (~line 151) | Change to `context_cycle_review` |
| `.claude/skills/uni-retro/SKILL.md` | `mcp__unimatrix__context_retrospective(feature_cycle: "{feature-id}")` (~line 29) | Change to `mcp__unimatrix__context_cycle_review(feature_cycle: "{feature-id}")` |
| `packages/unimatrix/skills/retro/SKILL.md` | Same call as above (~line 29) | Change to `mcp__unimatrix__context_cycle_review(...)` |

### README and product documentation

| File | Line(s) | What to change |
|------|---------|----------------|
| `README.md` | Table row for `context_retrospective` (~line 218) | Change tool name to `context_cycle_review` in the table row |

### Verification step (required, not optional)

After all updates: run `grep -r "context_retrospective" . --include="*.rs" --include="*.py" --include="*.md" --include="*.toml"` and confirm zero matches (excluding this SPECIFICATION.md and historical feature docs that are archival records, not callers).

---

## Constraints

**C-01** — `toml = "0.8"` must be added to `unimatrix-server/Cargo.toml` as an exact pin. The `toml` crate is not present anywhere in the workspace today. Run `cargo tree` post-add to surface transitive conflicts.

**C-02** — `FRESHNESS_HALF_LIFE_HOURS` is in `unimatrix-engine`. The config value must be plumbed as a parameter through `freshness_score()` and `compute_confidence()`. The architect must resolve whether to use a bare `f64` parameter or a `ConfidenceParams` context struct. A `ConfidenceParams` struct is preferred: it absorbs future W3-1 additions without further API churn. Either decision is acceptable but must be recorded in an ADR.

**C-03** — `ContentScanner::global()` must be initialized before `load_config()` calls `scan_title()`. The startup ordering in `tokio_main_daemon`/`tokio_main_stdio` must be verified or enforced. If `ContentScanner::global()` is lazily initialized, its initialization must be triggered before config loading begins. If the ordering cannot be guaranteed by type-system constraints, a documented startup ordering comment and integration test coverage are required.

**C-04** — Config types (`UnimatrixConfig` and sub-structs) must be defined such that the `unimatrix-store` crate does not need to depend on `unimatrix-server` types. `session_capabilities` values must be passed to the store layer as plain `Vec<Capability>` values, not as `Arc<UnimatrixConfig>`. The architect decides whether config types live in `unimatrix-server/src/infra/config.rs` (simplest) or a thin shared crate.

**C-05** — `agent_bootstrap_defaults()` in `unimatrix-store` is out of scope. The bootstrap list (system, human, cortical-implant), their trust levels, and their full capability sets remain hardcoded. Only `PERMISSIVE_AUTO_ENROLL` and the auto-enroll session capabilities are externalised.

**C-06** — No schema migration is introduced. Config is purely runtime state. No DB tables or schema version bump.

**C-07** — File permission check is Unix-only (`#[cfg(unix)]`). `std::os::unix::fs::PermissionsExt::mode()` is not available on Windows.

**C-08** — `rmcp` version is pinned at `=0.16.0`. No version change is permitted.

**C-09** — The `[confidence]` and `[cycle]` sections are reserved stubs. They MUST NOT contain any active fields in W0-3. Their presence in `UnimatrixConfig` is purely a forward-compatibility hedge for W3-1 and future features.

**C-10** — Per SCOPE.md non-goals: confidence dimension weights (`W_BASE`, `W_USAGE`, etc.), coherence gate lambda weights, `PROVENANCE_BOOST` magnitude, adaptive blend weight parameters, and per-session/per-agent config overrides are all explicitly out of scope for this feature.

**C-11** — No `UNIMATRIX_CONFIG` environment variable for overriding the global config path is introduced by this feature.

**C-12** — Config is loaded once at startup; runtime reload without restart is not supported.

---

## Dependencies

| Dependency | Type | Notes |
|-----------|------|-------|
| `toml = "0.8"` | New crate dependency | Add to `unimatrix-server/Cargo.toml` as exact pin |
| `serde` with `derive` feature | Existing | Already present in `unimatrix-server`; needed for `Deserialize` on config structs |
| `dirs` crate | Existing | Already a dependency of `unimatrix-server`; used for `dirs::home_dir()` |
| `ContentScanner` | Existing internal | `crates/unimatrix-server/src/infra/scanning.rs`; `scan_title()` used for instructions validation |
| `CategoryAllowlist` | Existing internal | `crates/unimatrix-server/src/infra/categories.rs`; seeded from config |
| `PERMISSIVE_AUTO_ENROLL` + `agent_resolve_or_enroll` | Existing internal | `crates/unimatrix-server/src/infra/registry.rs`, `crates/unimatrix-store/src/registry.rs` |
| `SERVER_INSTRUCTIONS` | Existing internal | `crates/unimatrix-server/src/server.rs:179` |
| `FRESHNESS_HALF_LIFE_HOURS` + `freshness_score()` | Existing internal | `crates/unimatrix-engine/src/confidence.rs:37,148` |
| `PROVENANCE_BOOST` comparisons in `search.rs` | Existing internal | Lines 413, 418, 484, 489 in `crates/unimatrix-server/src/services/search.rs` |
| `project::ensure_data_directory()` | Existing internal | Called in `main.rs`; provides `paths.data_dir` for per-project config resolution |

---

## NOT In Scope

The following are explicitly excluded to prevent scope creep. Any implementation touching these areas is a variance that will be flagged.

- **Confidence dimension weights** (`W_BASE`, `W_USAGE`, `W_FRESH`, `W_HELP`, `W_CORR`, `W_TRUST`) — not externalised; deferred to W3-1 GNN learning.
- **Coherence gate lambda weights** (`freshness`, `graph`, `contradiction`, `embedding`) — not externalised; remain hardcoded in `coherence.rs`.
- **`PROVENANCE_BOOST` magnitude** (`0.02`) — only which categories receive it is configurable; the magnitude is not.
- **Adaptive blend weight parameters** (`observed_spread * 1.25`, clamp bounds `[0.15, 0.25]`) — part of the crt-019 adaptive system.
- **`agent_bootstrap_defaults()` configurability** — full bootstrap list (system/human/cortical-implant), their trust levels, and full capability sets remain hardcoded.
- **`[confidence]` section fields** — the `ConfidenceConfig` struct is reserved but empty in W0-3. No fields are added.
- **`[cycle]` section fields** — the `CycleConfig` struct is reserved but empty in W0-3.
- **Renaming `context_cycle`** — the name is already domain-neutral.
- **Runtime config reload** — config is loaded once at startup; restart is required for changes.
- **`UNIMATRIX_CONFIG` env var** — no env var override for global config path.
- **Config tooling** (`validate` subcommand, `unimatrix config show`) — deferred.
- **Domain packs** — W0-3 provides the hook points; domain pack loading is a separate feature.
- **OAuth/auth config** — deferred per ADR #1839 (W0-2).
- **Per-session or per-agent config overrides** — config is global to the server instance.
- **Schema migration** — no DB changes.
- **`toml` crate version > 0.8** — exact pin required; no upgrade in this feature.

---

## Knowledge Stewardship

Queried: `/uni-query-patterns` for config externalization, TOML startup validation, MCP tool rename patterns — no results (Unimatrix MCP server not callable from this agent context; codebase read directly as secondary evidence). Key conventions confirmed from codebase: `CategoryAllowlist` poison recovery via `.unwrap_or_else(|e| e.into_inner())`, `ContentScanner::global()` singleton pattern, `validate_*` function naming convention in `infra/validation.rs`, `Arc<T>` threading pattern for shared subsystem state.

---

## Open Questions for the Architect

**OQ-01 (SR-02, High priority)**: Should `freshness_score()` take a bare `freshness_half_life_hours: f64` parameter, or should a `ConfidenceParams` struct be introduced to absorb future W3-1 additions without further API churn? The struct costs 10 lines now and prevents another cross-crate API break at W3-1. Recommendation: use `ConfidenceParams` struct. The architect must decide and record in an ADR before touching engine code.

**OQ-02 (SR-03)**: What is the exact initialization order of `ContentScanner::global()` relative to the config load insertion point in `tokio_main_daemon` and `tokio_main_stdio`? If the scanner is lazily initialized, is there a guarantee it is initialized before config loading begins? If not, the architect must either reorder initialization or delay the instructions validation step.

**OQ-03 (SR-07, SR-08)**: Should `UnimatrixConfig` types live in `unimatrix-server/src/infra/config.rs` (simplest, avoids new crate), or in a thin `unimatrix-config` crate (shareable across crate boundary)? The store crate currently has no dependency on server-layer types. Passing plain `Vec<Capability>` values across the boundary avoids the dependency; this is the recommended approach, but the architect must confirm.

**OQ-04 (SR-06, resolved in this spec)**: List field merge semantics: per-project list replaces global list entirely (not appended). This is specified in FR-003. The architect should confirm this is the intended operator UX before implementation begins.
