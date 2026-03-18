## ADR-004: Forward-Compatibility Stubs for [confidence] and [cycle]

### Context

SCOPE.md marks both `[confidence]` and `[cycle]` as non-goals for W0-3:
- `[confidence]` lambda weights are excluded because operators cannot meaningfully
  tune ML weights; W3-1's GNN will learn them automatically.
- `[cycle]` label parameters are excluded because the tool concept is already
  domain-neutral and the doc fix (Goal 7/8) addresses the vocabulary concern
  without runtime config.

However, SR-04 identifies a structural risk: PRODUCT-VISION W0-3 includes both
`[confidence]` and `[cycle]` sections in the intended config format. W3-1 explicitly
depends on `[confidence] weights` for GNN cold-start configuration. If `UnimatrixConfig`
is designed without these section stubs, W3-1 must either:
(a) add the sections and potentially conflict with any per-project configs that
    omit them (TOML parse error on unknown keys if strict parsing is used), or
(b) define a parallel config format that collides with W0-3's established format.

A TOML section that is parsed but empty (all fields are reserved for future use with
empty Default impls) costs nothing at runtime and zero operator-visible behavior change.
It reserves the namespace in the config format so later features can add fields without
a format break.

Two approaches:

**Option A — No stubs**: `UnimatrixConfig` has exactly four sections matching W0-3's
implemented scope (`[knowledge]`, `[server]`, `[agents]`, `[inference]` if added
later). W3-1 adds `[confidence]` and `[cycle]` sections when they implement them.
Risk: if a per-project config written for W0-3 includes a `[confidence]` section
(e.g., a user anticipating W3-1 and pre-writing config), the TOML parser will
reject it as an unknown key unless `deny_unknown_fields` is absent. With serde's
default behavior (unknown keys ignored), this is not a parse error — but users who
pre-write config will silently get no effect.

**Option B — Empty stubs**: Add `ConfidenceConfig` and `CycleConfig` as empty
structs with `#[derive(Default, Deserialize)]` and `#[serde(default)]` in
`UnimatrixConfig`. No fields, no behavior, no validation. Future waves add fields
to these structs without changing the outer `UnimatrixConfig` shape.

```rust
/// Reserved for W3-1: GNN cold-start weight configuration.
/// No fields are active in W0-3.
#[derive(Debug, Default, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct ConfidenceConfig {}

/// Reserved for future domain-label customisation.
/// No fields are active in W0-3.
#[derive(Debug, Default, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct CycleConfig {}
```

This is the "10-line hedge" described in SR-04. The TOML format accepts `[confidence]`
and `[cycle]` sections gracefully — they parse into empty structs with no effect.
When W3-1 adds `weights = { freshness = 0.35, ... }` to `ConfidenceConfig`, all
existing configs that already include an empty `[confidence]` section continue to
parse without error; configs without the section inherit `Default`.

### Decision

Add `ConfidenceConfig` and `CycleConfig` as **empty forward-compatibility stubs**
in `UnimatrixConfig`:

```rust
pub struct UnimatrixConfig {
    #[serde(default)]
    pub knowledge: KnowledgeConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub agents: AgentsConfig,
    /// Reserved for W3-1 (GNN cold-start weights). No fields active in W0-3.
    #[serde(default)]
    pub confidence: ConfidenceConfig,
    /// Reserved for domain label customisation. No fields active in W0-3.
    #[serde(default)]
    pub cycle: CycleConfig,
}
```

Both stubs are:
- Deserializable (accept any future TOML content without parse errors, because
  unknown fields are silently ignored by serde by default).
- Serializable (needed for debug output / `config show` tooling in a future wave).
- Comparable (`PartialEq`) for merge logic.
- Documented with a `W3-1` reference so implementers know why they exist.

No validation logic runs against `ConfidenceConfig` or `CycleConfig` in W0-3
(empty structs have nothing to validate).

The merge strategy (ADR-003) handles them identically to other sections: if
per-project has a non-default value, it wins — but since both stubs are always
`Default`, the merge is a no-op for these fields in W0-3.

### Consequences

**Easier:**
- W3-1 implementers add fields to `ConfidenceConfig` without touching the outer
  `UnimatrixConfig` struct — no format break, no operator-visible change until fields
  are populated.
- Users who pre-write `[confidence]` sections in anticipation of W3-1 do not get
  TOML parse errors; their configs are silently accepted (and silently no-op'd until
  W3-1).
- The `[cycle]` stub reserves the namespace, preventing a future rename collision if
  another feature were to introduce an incompatible `[cycle]` format.

**Harder:**
- `UnimatrixConfig` has two sections that do nothing in W0-3. This could confuse
  operators who discover them via documentation or `config show` output. Doc comments
  on the structs must clearly identify them as reserved-for-future-use.
- The empty stubs must be kept empty in W0-3's PR — a reviewer must catch any
  attempt to add logic to them prematurely.
