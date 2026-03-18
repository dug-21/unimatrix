# Config Externalization (W0-3)

## Problem Statement

Unimatrix is a domain-agnostic knowledge engine, but its behavior is hard-coupled to software delivery vocabulary. Constants defining knowledge categories, confidence decay rates, server instructions, and agent trust bootstrapping are compiled into the binary. Operators targeting non-dev domains (SRE, legal, air-quality monitoring) cannot run the same binary with domain-appropriate settings — they would need to fork the code or accept an ill-fitted configuration.

Additionally, two tools carry dev-workflow vocabulary that prevents Unimatrix from presenting itself as domain-neutral: `context_retrospective` uses Agile/Scrum terminology, and `CycleParams` describes its topic field in software-delivery terms.

A secondary problem: even with externalised constants, operators who don't understand the confidence model cannot tune the 6-factor weight vector meaningfully. Asking them to set `W_TRUST = 0.24` without understanding its role produces random configuration. The confidence weight problem requires a different interface — domain-calibrated presets that encode the full multi-dimensional weight pattern, with `custom` as an expert escape hatch.

## Goals

1. Load `~/.unimatrix/config.toml` (global) at server startup; apply settings before any request is handled.
2. Support an optional per-project override at `~/.unimatrix/{hash}/config.toml`; per-project values shadow global values, which shadow compiled defaults.
3. Externalize constants to TOML-typed config sections: `[profile]`, `[knowledge]`, `[confidence]` (custom preset only), `[server]`, `[agents]`.
4. Ship a `[profile] preset` system: four named knowledge-lifecycle presets (`authoritative`, `operational`, `empirical`, `collaborative`) plus `custom`. Named presets encode domain-calibrated weight vectors across all six confidence dimensions — operators identify their knowledge type, not ML weights. `custom` exposes raw `[confidence]` weights for expert use.
5. Validate all security-critical config values at load time; fail server startup on violation (reject, do not warn-and-continue).
6. Apply file-permission checks at startup: reject world-writable config; log warning on group-writable.
7. Preserve all compiled defaults unchanged when no config file is present so existing behavior is unaltered (`collaborative` preset = current dev defaults).
8. Rename the `context_retrospective` tool to `context_cycle_review` — hardcoded, not configurable.
9. Neutralise the `CycleParams` field doc for `topic` to communicate the domain-agnostic concept.

## Non-Goals

- Runtime config reload without restart — config is loaded once at startup; changes require restart.
- Config over environment variables — env vars (`UNIMATRIX_TICK_INTERVAL_SECS`, `UNIMATRIX_AUTO_QUARANTINE_CYCLES`) already exist and are not replaced or merged by this feature.
- Per-session or per-agent config overrides — config is global to the server instance.
- Schema migration — config is purely runtime state; no DB schema changes are introduced.
- OAuth or authentication config (`UNIMATRIX_CLIENT_TOKEN`) — deferred per ADR #1839 (W0-2).
- Config tooling (validate subcommand, `unimatrix config show`) — out of scope for W0-3.
- Domain packs — W0-3 enables them by providing the hook points; domain pack loading is a separate feature.
- **Raw weight tuning as a primary interface** — operators are not asked to set `W_TRUST = 0.24` directly. The `[profile]` preset system is the primary interface; raw `[confidence]` weights are only active when `preset = "custom"` and are an expert escape hatch, not the expected path.
- **Coherence gate lambda weights** (`confidence_freshness`, `graph_quality`, `embedding_consistency`, `contradiction_density`) — these are the 4-factor KB-health metric weights used in `compute_lambda()`. Operators cannot meaningfully tune these independently of domain science. They remain hardcoded constants in `coherence.rs` and are not part of any preset.
- **`[cycle]` config section** — the tool concept is already domain-neutral; "feature" vocabulary in the tool description is addressed by the hardcoded rename/doc fix (Goal 7/8), not by runtime config.
- Renaming `context_cycle` — the name is already domain-neutral.
- Externalising `PROVENANCE_BOOST` magnitude (`0.02`) — the boost magnitude is not a domain-specific constant; only which categories receive it is domain-specific.
- Externalising adaptive blend weight parameters (`observed_spread * 1.25`, clamp bounds `[0.15, 0.25]`) — these are part of the crt-019 adaptive system, not static config.
- **`UNIMATRIX_CONFIG` env var** — no env var override for the global config path. Low complexity to add later if CI/container deployments require it; no evidence of that need today.
- **Initialize-once semantics for W3-1 learned weights** — the config preset is a cold-start seed only. Once W3-1 (GNN adaptive learning) has stored a learned weight vector in `analytics.db`, those values must persist across restarts and must not be overwritten by the config preset. This lifecycle enforcement is W3-1's responsibility: W3-1 extends `resolve_confidence_params()` with a priority-0 check (`load_learned_weights`) that returns stored weights when they exist, bypassing config entirely. dsn-001 documents the extension point in ADR-006 but does not implement it — `analytics.db` and the `confidence_weights` table do not exist until W3-1.

## Background Research

### Hardcoded Constants — Exact Locations

**`[knowledge]` section:**
- `INITIAL_CATEGORIES` array (8 values): `crates/unimatrix-server/src/infra/categories.rs:8–17`
  - `CategoryAllowlist::new()` populates `RwLock<HashSet<String>>` from this constant at startup.
  - `add_category()` already exists on `CategoryAllowlist` but is never called from any MCP tool or startup path — it is dead infrastructure today. Config load will call it in a loop to seed the allowlist from config.
  - **No MCP path exists to add categories at runtime.** Today the only way to get a new category accepted is to edit source and rebuild.
- `boosted_categories` — hardcoded as a category string comparison `entry.category == "lesson-learned"` at:
  - `crates/unimatrix-server/src/services/search.rs:413,418,484,489`
  - Applied via `PROVENANCE_BOOST` constant from `unimatrix-engine/src/confidence.rs:56` (`0.02`).
- `FRESHNESS_HALF_LIFE_HOURS`: `crates/unimatrix-engine/src/confidence.rs:37` (`168.0`)
  - Used in `freshness_score()` at line 148 in same file.
  - In the `unimatrix-engine` crate — requires the const to be passed as a parameter or the crate to expose a configurable entry point.
  - **This is operator-interpretable**: legal domain → years; air quality → hours; dev → 1 week (168h). This is the one tunable in the confidence system that any operator can reason about without ML expertise.

**`[server]` section:**
- `SERVER_INSTRUCTIONS`: `crates/unimatrix-server/src/server.rs:179`
  - Loaded into `ServerInfo.instructions` at `server.rs:245`.
  - Direct prompt injection surface — passed verbatim as MCP server metadata to every connecting AI agent.

**`[agents]` section:**
- `PERMISSIVE_AUTO_ENROLL`: `crates/unimatrix-server/src/infra/registry.rs:25` (`true`)
  - Controls whether unknown agents get `[Read, Write, Search]` (permissive) vs `[Read, Search]` (strict).
- `session_capabilities` (default caps for auto-enrolled agents): `crates/unimatrix-store/src/registry.rs:113–119`
  - Permissive path: `[Read, Write, Search]`; non-permissive: `[Read, Search]`.
- Bootstrap agent definitions: `crates/unimatrix-store/src/registry.rs:16–82`
  - Hardcoded: `system` (System trust, all caps), `human` (Privileged trust, all caps), `cortical-implant` (Internal trust, Read+Search).
  - **Scoped to `default_trust` + `session_capabilities` only.** Full bootstrap list (system/human/cortical-implant) remains hardcoded — configuring agent IDs, trust levels, and capability sets requires significant store-layer refactoring and adds no domain-agnosticism value in practice.

**Tool vocabulary (hardcoded fixes, not config):**
- `context_retrospective` tool name: `crates/unimatrix-server/src/mcp/tools.rs` — rename to `context_cycle_review`. "Retrospective" is Agile vocabulary; "review" is domain-neutral and makes the pairing with `context_cycle` self-evident.
- `CycleParams.topic` field doc: `tools.rs:257–260` — currently says "Feature cycle identifier (e.g., 'col-022')". Update to convey the domain-agnostic concept: any bounded work unit a domain tracks (feature, incident, campaign, case).

### Server Startup Path

Config must be loaded in `tokio_main_daemon` and `tokio_main_stdio` (in `main.rs`), after `project::ensure_data_directory()` resolves paths, and before any subsystem construction. The natural insertion point is immediately after `ensure_data_directory()` returns `paths`, since `paths.data_dir` is needed for per-project config lookup.

The bridge mode (`tokio_main_bridge`) does not run server subsystems and does not need config.
The hook path (`Command::Hook`) is a sync path with a sub-50ms budget and must not load config.
Export/import subcommands are offline tools — no server config needed.

### Current Config Infrastructure

No TOML parsing infrastructure exists anywhere in the workspace. The `toml` crate is not in any `Cargo.toml`. The only existing runtime tunables are env vars parsed in `background.rs:80,141`. No `config.toml` file exists anywhere in the project.

The `dirs` crate (already a dependency of `unimatrix-server`) provides `dirs::home_dir()` for resolving `~/.unimatrix/`.

### ContentScanner (`[server] instructions` security)

`ContentScanner` is a singleton in `crates/unimatrix-server/src/infra/scanning.rs`. It holds 26 injection pattern regexes and 6 PII patterns. `ContentScanner::global()` returns the compiled singleton. The `scan()` method returns `Err(ScanResult)` on match; `scan_title()` checks injection patterns only (not PII).

For `[server] instructions` validation, `scan_title()` is the correct method: instructions are a prompt string, not PII-bearing content. Instructions that trigger any injection pattern must cause config load to fail with a startup error.

### Schema Migration Implications

None. Config is purely runtime state. No new DB tables or schema version bump is required.

### File Permission Model

Project data directories are created with mode `0o700` (`main.rs` → `ensure_data_directory` → `fs::set_permissions(..., 0o700)`). Config files in `~/.unimatrix/` sit one level up from this per-project directory. The global config `~/.unimatrix/config.toml` has no mode enforcement today. W0-3 introduces the first file-permission check in this codebase.

On Unix (Linux/macOS), `std::fs::metadata().permissions().mode()` gives the full mode bits. The check `mode & 0o002 != 0` identifies world-writable files. This is a startup-time check, not a runtime watch.

## Config Schema

```toml
[profile]
# Knowledge lifecycle preset. Sets all six confidence dimension weights and
# freshness_half_life_hours to domain-calibrated starting values.
# W3-1 GNN inherits these values as its cold-start and refines from there.
#
# "authoritative" — source matters most, changes rarely (policy, standards, precedents)
# "operational"   — guides action, ages quickly (runbooks, incidents, procedures)
# "empirical"     — derived from measurement, time-critical (sensors, metrics, feeds)
# "collaborative" — built by a team, votes meaningful (dev, research) [DEFAULT]
# "custom"        — read weights directly from [confidence] section below
preset = "collaborative"

[knowledge]
# Domain-specific category allowlist. Replaces the compiled INITIAL_CATEGORIES (8 dev categories).
# Each value: lowercase, [a-z0-9_-], max 64 chars. Total: ≤ 64 categories.
categories = ["outcome", "lesson-learned", "decision", "convention",
              "pattern", "procedure", "duties", "reference"]

# Categories that receive the provenance boost in search re-ranking.
# Must be a strict subset of `categories`.
boosted_categories = ["lesson-learned"]

# Freshness half-life in hours. Overrides the preset's default when set.
# Legal: 8760.0 (1 year). Air quality: 24.0 (1 day). Dev default: 168.0 (1 week).
# Omit to use the preset's built-in value.
freshness_half_life_hours = 168.0

[confidence]
# Only active when [profile] preset = "custom". Ignored for all named presets.
# Expert escape hatch — use a named preset unless you have domain science
# justification for specific values.
# Each weight in [0.0, 1.0]. Sum must be ≤ 1.0. All six required when preset = "custom".
weights = { base = 0.16, usage = 0.16, fresh = 0.18, help = 0.12, corr = 0.14, trust = 0.16 }

[server]
# Verbatim MCP server instructions returned during the initialize handshake.
# Injection patterns are validated at load time — startup aborts if triggered.
instructions = """..."""

[agents]
# Auto-enroll behaviour for unknown agents.
# "permissive" grants [Read, Write, Search]; "strict" grants [Read, Search].
default_trust = "permissive"

# Default capability set for auto-enrolled agents (overrides default_trust caps).
session_capabilities = ["Read", "Write", "Search"]
```

### Preset Weight Table

Exact values are an architect deliverable (require domain science validation before shipping). Illustrative profiles below — the ordering relationships are the invariants, not the specific numbers:

| Preset | W_FRESH | W_TRUST | W_USAGE | W_CORR | W_HELP | W_BASE | half_life |
|--------|---------|---------|---------|--------|--------|--------|-----------|
| `authoritative` | low | high | low | high | moderate | standard | 8760h (1yr) |
| `operational` | high | moderate | high | high | low | standard | 720h (1mo) |
| `empirical` | very high | low | moderate | low | none | standard | 24h |
| `collaborative` | moderate | moderate | moderate | moderate | moderate | standard | 168h (1wk) |

`collaborative` = current compiled defaults (backward-compatible). W3-1 GNN cold-starts from whichever preset is active and refines toward actual usage — a non-dev domain starting at its correct preset converges significantly faster than one starting from `collaborative`.

## Config Security Model

Config is an **operator trust boundary**. A writable config file is equivalent to arbitrary server reconfiguration: `[server] instructions` is a direct system-prompt injection surface; `[knowledge] categories` gates what enters the knowledge base; `[agents]` controls who gets write access. The security model treats config as trusted-but-validated — loaded from the filesystem, owned by the operator, but never taken at face value.

### Pre-Parse

- **File size cap**: read at most 64 KB before passing to the TOML parser. A crafted large config file cannot cause memory exhaustion. Return a startup error if the file exceeds the cap.
- **File permissions** (Unix only, `#[cfg(unix)]`): checked for both `~/.unimatrix/config.toml` and `~/.unimatrix/{hash}/config.toml` independently.
  - World-writable (`mode & 0o002 != 0`): abort startup — any local process can reconfigure the server.
  - Group-writable (`mode & 0o020 != 0`): log a warning, continue — elevated risk but common in shared dev environments.
  - Symlinks: `std::fs::metadata()` follows symlinks and reports the target's permissions. This is the correct behaviour — what matters is who can write the file the server will actually read.

### Post-Parse Field Validation

All validation runs in `validate_config()` immediately after deserialization. Any failure aborts startup with a descriptive error; no field silently falls back to a default after a violation.

| Field | Constraint | Attack prevented |
|-------|-----------|-----------------|
| `[knowledge].categories` each value | `[a-z0-9_-]`, max 64 chars | Injection into error messages and tool descriptions |
| `[knowledge].categories` count | ≤ 64 | Memory and iteration DoS |
| `[knowledge].boosted_categories` each value | Must be in validated `categories` set (inherits char constraints) | Privilege of boost applied to unchecked label |
| `[knowledge].freshness_half_life_hours` | finite (`!is_nan() && !is_infinite()`), > 0.0, ≤ 87600.0 (10 years) | Division by zero in `freshness_score()`; NaN corruption throughout confidence scoring; absurd values making all knowledge permanently fresh or instantly stale |
| `[server].instructions` length | ≤ 8 KB (8192 bytes) | Unbounded system-prompt injection payload; memory pressure |
| `[server].instructions` content | `ContentScanner::global().scan_title()` — reject on any injection pattern | Prompt injection via operator-controlled system prompt |
| `[profile].preset` | Must be one of `{"authoritative", "operational", "empirical", "collaborative", "custom"}` | Unknown preset silently using wrong weights |
| `[confidence].weights` (custom only) | All six keys required; each in `[0.0, 1.0]`; sum ≤ 1.0; all finite | Weight misconfiguration corrupting all confidence scores |
| `[agents].default_trust` | Must be one of `{"permissive", "strict"}` — strict allowlist | Arbitrary trust string reaching registry logic; unknown values silently defaulting |
| `[agents].session_capabilities` each value | Must be one of `{"Read", "Write", "Search"}` — strict allowlist, `Admin` excluded | Privilege escalation: operator config granting Admin to all auto-enrolled agents |

### What Is Not Scanned

Free-form string fields not listed above (`instructions` aside) do not need character scanning because they are either validated against a closed allowlist (enum-like fields) or are not surfaced to agents or stored in the knowledge base. `freshness_half_life_hours` is numeric and cannot carry injection payloads.

### Trust Model Summary

- Config is owned by the server operator. If an attacker can write config, they own the server — no validation fully compensates for that. File permission enforcement is the primary defence.
- `[server].instructions` is the highest-risk field: it is passed verbatim to every connecting AI agent as part of the MCP initialize handshake. ContentScanner is a secondary defence; file permissions are the first.
- `[knowledge].categories` is the gate for the entire knowledge base schema. A misconfigured allowlist does not corrupt existing data but does control what new entries are accepted after restart.

---

## Proposed Approach

### Config Struct

Define a `UnimatrixConfig` struct with four sub-structs (`KnowledgeConfig`, `ServerConfig`, `AgentsConfig`) in a new `crates/unimatrix-server/src/infra/config.rs`. Derive `serde::Deserialize` and supply `Default` impls that reproduce current hardcoded values exactly.

### TOML Crate

Add `toml = "0.8"` to `unimatrix-server/Cargo.toml`. The `toml` crate is the standard Rust TOML parser with `serde` integration.

### Load Order

Per ADR-004, project data lives at `~/.unimatrix/{sha256(canonical_project_root)[..16]}/`. Per-project config follows the same pattern — it is stored alongside the project's `knowledge.db` and `analytics.db`, not in the project source tree (no accidental git commits, no path collisions).

1. Try `~/.unimatrix/config.toml` (global) — deserialize if present.
2. Try `~/.unimatrix/{hash}/config.toml` (per-project) — merge if present; per-project keys win over global keys; unspecified keys fall through to global or compiled defaults.
3. On absent file at either level: silently use defaults (no error, no warning).
4. On malformed TOML or security validation failure at either level: return `Err` → startup aborts.

Both files are subject to the full security model (size cap, permissions check, field validation). A violation in the per-project config aborts startup the same as a violation in the global config.

### Security Validation

Performed in a `validate_config()` function immediately after deserialization:
1. File permissions check (Unix only; skip on Windows).
2. Category values: each must match `[a-z0-9_-]`, max 64 chars, total ≤ 64.
3. `boosted_categories` must be strict subset of `categories`.
4. `[server] instructions`: pass through `ContentScanner::global().scan_title()` — reject if injection patterns detected.

### Wiring

- Pass `Arc<UnimatrixConfig>` into `CategoryAllowlist::new_from_config()`, `UnimatrixServer::new()`, and `ServiceLayer` constructors as needed.
- `FRESHNESS_HALF_LIFE_HOURS` is in `unimatrix-engine`. The config value must be plumbed as a parameter to `freshness_score()` — add a `freshness_half_life_hours: f64` parameter and pass it from the server through the confidence computation call chain.
- `boosted_categories` replaces the hardcoded `== "lesson-learned"` comparison in `search.rs` with a `HashSet<String>` lookup.
- `[agents]` settings replace `PERMISSIVE_AUTO_ENROLL` and the hardcoded session capability branches in `registry.rs`.
- `[server] instructions` replaces `SERVER_INSTRUCTIONS` const in `server.rs`.
- `context_retrospective` → `context_cycle_review`: rename the tool name in `#[tool(name = "...")]`, update the tool handler function name, update all references in protocols, skills, tests, and CLAUDE.md tool list.
- `CycleParams.topic` doc: update field doc comment to domain-neutral language.

## Acceptance Criteria

- AC-01: When no `~/.unimatrix/config.toml` or per-project `config.toml` exists, server starts with all existing default values and all existing tests pass without modification.
- AC-02: When `[knowledge] categories` is set, the new list replaces the default 8-category allowlist and `CategoryAllowlist` reflects it.
- AC-03: When `[knowledge] boosted_categories` is set, those categories receive the provenance boost in search re-ranking; the previously hardcoded `"lesson-learned"` comparison is no longer present in `search.rs`.
- AC-04: When `[knowledge] freshness_half_life_hours` is set, `freshness_score()` uses that value instead of the compiled `168.0` constant.
- AC-05: When `[server] instructions` is set, that string appears in the MCP `ServerInfo.instructions` field returned during the initialize handshake.
- AC-06: When `[agents] default_trust` and `session_capabilities` are set, unknown agents auto-enroll with the configured default capabilities.
- AC-07: A per-project config at `{data_dir}/config.toml` overrides the global `~/.unimatrix/config.toml` for all keys it specifies; unspecified keys fall through to global or defaults.
- AC-08: A config file with world-writable permissions (`mode & 0o002 != 0`) causes server startup to abort with an error message identifying the file path and violation.
- AC-09: A config file with group-writable permissions (`mode & 0o020 != 0`) logs a warning but does not abort startup.
- AC-10: A `[knowledge] categories` entry with characters outside `[a-z0-9_-]`, length > 64, or a total category count > 64 causes startup to abort with a descriptive error.
- AC-11: A `[knowledge] boosted_categories` value not present in `[knowledge] categories` causes startup to abort with an error naming the invalid value.
- AC-12: A `[server] instructions` value matching any injection pattern in `ContentScanner` causes startup to abort with an error identifying the triggering pattern category.
- AC-13: The MCP tool formerly named `context_retrospective` is now named `context_cycle_review`; all callers in protocols, skills, and tests are updated; no reference to `context_retrospective` remains in the codebase.
- AC-14: The `CycleParams.topic` field doc no longer references "feature" as the canonical example; the doc communicates the domain-agnostic concept of a bounded work unit.
- AC-22: When no `[profile]` section is present, the server uses the `collaborative` preset (current compiled defaults) — all existing tests pass unchanged.
- AC-23: When `preset = "authoritative"`, `"operational"`, `"empirical"`, or `"collaborative"`, the corresponding compiled weight vector and `freshness_half_life_hours` are used; the `[confidence]` section is ignored even if present.
- AC-24: When `preset = "custom"`, all six `[confidence] weights` are required; startup aborts with a descriptive error if any are absent.
- AC-25: When `preset = "custom"` and `[knowledge] freshness_half_life_hours` is also set, the `[knowledge]` value takes precedence over any default; when only `[confidence]` is used, `freshness_half_life_hours` must also be specified there.
- AC-26: An unrecognised `preset` value causes startup to abort with an error listing valid values.
- AC-27: W3-1 cold-start reads the effective weight vector (from the active preset or `custom` values) — the `ConfidenceParams` struct carries these values at startup.
- AC-15: A config file exceeding 64 KB causes startup to abort with an error before TOML parsing begins.
- AC-16: A `[knowledge].freshness_half_life_hours` value of `0.0`, negative, `NaN`, or `Infinity` causes startup to abort with a descriptive error.
- AC-17: A `[knowledge].freshness_half_life_hours` value greater than `87600.0` causes startup to abort with a descriptive error.
- AC-18: A `[agents].default_trust` value other than `"permissive"` or `"strict"` causes startup to abort with an error listing valid values.
- AC-19: A `[agents].session_capabilities` list containing any value other than `"Read"`, `"Write"`, or `"Search"` (including `"Admin"`) causes startup to abort with an error.
- AC-20: A `[server].instructions` value exceeding 8 KB causes startup to abort with an error before ContentScanner runs.
- AC-21: All new validation paths have unit tests. All existing unit and integration tests continue to pass.

## Constraints

- **`toml` crate** must be added to `unimatrix-server/Cargo.toml` — it is not present anywhere in the workspace today.
- **`FRESHNESS_HALF_LIFE_HOURS` lives in `unimatrix-engine`** — plumbing this config value requires changing the engine crate's public API (adding a parameter to `freshness_score()` and through to `compute_confidence()`). The engine crate is pure (no I/O); the cleanest approach is a function parameter, not a global.
- **`context_retrospective` rename blast radius** — the tool name appears in protocol files, skill files, tests, and CLAUDE.md. All references must be updated in the same PR; a partial rename breaks callers.
- **`agent_bootstrap_defaults()` is in `unimatrix-store`** — full bootstrap list configurability is out of scope. Only `default_trust` and `session_capabilities` (server-layer concerns) are externalised.
- **No schema migration** — existing DB entries with "lesson-learned" category will continue to receive the boost regardless of config changes to `boosted_categories`. Config changes affect new behavior from the point of restart only.
- **File permission check is Unix-only** — `std::os::unix::fs::PermissionsExt` is not available on Windows. Wrap with `#[cfg(unix)]`.
- **Test isolation** — tests that construct server subsystems directly (e.g., `CategoryAllowlist::new()` in categories.rs tests) must remain valid with the default constructor. Config injection for tests requires a `new_from_config()` path alongside the existing `new()` default.
- **rmcp 0.16.0 pin** — the rmcp version is pinned at `=0.16.0`. No version change is permitted.

## Open Questions

None.

## Vision Variance Resolutions

- **VARIANCE-1 CLOSED** — Confidence weights are externalised via the `[profile]` preset system. Named presets encode domain-calibrated starting values across all six dimensions; `custom` exposes raw weights for experts. W3-1 cold-starts from the active preset, resolving the vision's cold-start requirement. ADR-004 forward-compat stub for `[confidence]` now has concrete purpose.
- **VARIANCE-2 CLOSED** — `[cycle]` label configurability replaced with a hardcoded doc-fix. Accepted as sufficient; namespace reserved via ADR-004 stub.
- **VARIANCE-3 CLOSED** — `default_trust = "permissive"` confirmed as correct default (W0-2 deferral rationale; OAuth/W2-3 will make `"strict"` meaningful). Vision example to be corrected in a documentation pass.

## Tracking

https://github.com/dug-21/unimatrix/issues/306
