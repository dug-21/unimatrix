## ADR-002: Domain Pack Registry via TOML Config at Startup

### Context

The observation pipeline must support multiple domains (Claude Code, SRE, scientific
instruments) without requiring code changes. A domain is more than just event types — it
also declares what knowledge categories it uses (`CategoryAllowlist`) and what detection
rules apply to its events.

Three design options were considered:

**Option A: Runtime MCP tool for domain registration**
A 13th MCP tool (`context_register_domain`) allows Admin callers to register domain packs
at runtime. This matches the pattern of `context_enroll` for agent registration.
Rejected: Adds a new tool schema with unclear extension surface on existing tools; not
needed for W1-5 since the only runtime domain is `"claude-code"`; runtime state is not
version-controllable.

**Option B: Compile-time Rust domain pack registration**
Domain packs are Rust structs implementing a `DomainPack` trait, registered via a
`register_domain_pack()` function at startup.
Rejected: Requires recompile for every new domain; contradicts the goal of no-code-change
extensibility for operators.

**Option C: TOML config-driven startup registration (chosen)**
Domain packs are declared as TOML stanzas in the config file. The `"claude-code"` pack is
always bundled as the default (active with zero config). Operators add additional packs to
the config file. This follows the exact same two-level hierarchy pattern as
`UnimatrixConfig` / `KnowledgeConfig` — no new patterns introduced.

The scope risk assessment (SR-05) identified that "Admin runtime re-registration via an
existing tool" was unresolved. The resolution per scope revision: Admin runtime override
is removed from W1-5 scope entirely. Config-file-driven startup is simpler, reproducible,
and version-controllable. If runtime override is needed, it is a follow-on feature.

### Decision

A `DomainPackConfig` struct and an `ObservationConfig` section are added to
`UnimatrixConfig`, following the exact `#[serde(default)]` pattern of `KnowledgeConfig`:

```toml
[observation]
# Optional: extend or replace the default claude-code pack.
# If this section is absent, the claude-code pack is loaded with its built-in defaults.

[[observation.domain_packs]]
source_domain = "claude-code"
event_types = ["PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop"]
categories = ["outcome", "lesson-learned", "decision", "convention",
              "pattern", "procedure", "duties", "reference"]
# No rule_file — claude-code rules are built-in Rust implementations

[[observation.domain_packs]]
source_domain = "sre"
event_types = ["incident_opened", "incident_resolved", "alert_fired", "alert_cleared"]
categories = ["incident", "postmortem", "runbook"]
rule_file = "/etc/unimatrix/sre-rules.toml"
```

The `DomainPackRegistry` is an `Arc<RwLock<HashMap<String, DomainPack>>>` initialized at
startup. At startup:
1. The built-in `"claude-code"` pack is always loaded first (cannot be absent)
2. TOML `[[observation.domain_packs]]` stanzas are merged in — if a stanza specifies
   `source_domain = "claude-code"`, it overrides the built-in defaults
3. Each domain pack's `categories` are merged into `CategoryAllowlist` via
   `CategoryAllowlist::from_categories()` — same path as today's config-driven categories

`DomainPackRegistry` is passed as `Arc` into `SqlObservationSource` so the ingest function
can resolve `source_domain` from the known event type strings.

The `"claude-code"` default pack is immutable in the source: its `event_types` and
`categories` are defined as `const` in the registry module and only overrideable via
explicit TOML declaration. Absent `[observation]` config produces identical behavior to
current production.

### Consequences

**Easier:**
- Operators add domains via config file — no recompile required
- `CategoryAllowlist` absorption is handled at startup via existing `from_categories()` —
  no new code path
- Config is version-controllable and reproducible across deployments
- Testing: domain pack registration is testable by instantiating `DomainPackRegistry`
  with synthetic packs, no MCP tool invocation needed

**Harder:**
- New domains require a server restart to activate (no hot-reload)
- External domain detection rules (via `rule_file`) must be loaded and validated at
  startup — startup error handling must report invalid rule files clearly
- The structural test for `UNIVERSAL_METRICS_FIELDS` must be updated to account for
  domain-pack-contributed categories; the test scope narrows to `"claude-code"` pack only
