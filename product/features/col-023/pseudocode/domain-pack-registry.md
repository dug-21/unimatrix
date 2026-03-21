# Pseudocode: domain-pack-registry

**Wave**: 2 (parallel with rule-dsl-evaluator and config-extension)
**Crate**: `unimatrix-observe`
**File**: `crates/unimatrix-observe/src/domain/mod.rs` (new module)
**Also modifies**: `crates/unimatrix-observe/src/lib.rs` (add `pub mod domain`)
**Also modifies**: `crates/unimatrix-observe/src/error.rs` (new error variants)

## Purpose

New module that defines `DomainPack`, `DomainPackRegistry`, the built-in "claude-code"
pack constant, and validation logic. The registry is initialized at server startup from
TOML config (via `with_builtin_claude_code()` + config packs) and threaded as `Arc`
into `SqlObservationSource`.

`rule-dsl-evaluator` lives in the same module file. See that pseudocode file for
`RuleDescriptor`, `ThresholdRule`, `TemporalWindowRule`, and `RuleEvaluator`.

## Module Structure

```
unimatrix-observe/src/domain/
    mod.rs    -- all domain types, registry, built-in pack
               -- also contains RuleDescriptor, RuleEvaluator (rule-dsl-evaluator)
```

A single `mod.rs` is sufficient. The file should stay well under 500 lines with the
two components combined. If it approaches the limit, split into:
```
unimatrix-observe/src/domain/
    mod.rs        -- pub use, re-exports, DomainPack, DomainPackRegistry
    evaluator.rs  -- RuleDescriptor, ThresholdRule, TemporalWindowRule, RuleEvaluator
```

## New Error Variants (unimatrix-observe/src/error.rs)

Add to the existing `ObserveError` enum:

```
PayloadTooLarge {
    session_id: String,
    event_type: String,
    size: usize,
}

PayloadNestingTooDeep {
    session_id: String,
    event_type: String,
    depth: usize,
}

InvalidSourceDomain {
    domain: String,
}

InvalidRuleDescriptor {
    rule_name: String,
    reason: String,
}
```

These variants are used in `ingest-security` (Wave 4) and domain pack validation.

## Built-in claude-code Pack (const)

```
const BUILTIN_CLAUDE_CODE_PACK: DomainPack = DomainPack {
    source_domain: "claude-code",
    event_types: vec![
        "PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop"
    ],
    categories: vec![
        "outcome", "lesson-learned", "decision", "convention",
        "pattern", "procedure", "duties", "reference"
    ],
    rules: vec![],    -- built-in claude-code rules are Rust DetectionRule impls, not DSL
}
```

Note: Rust does not allow `vec![]` in const. Implement this as a function:

```
fn builtin_claude_code_pack() -> DomainPack:
    DomainPack {
        source_domain: "claude-code".to_string(),
        event_types: vec![
            "PreToolUse".to_string(),
            "PostToolUse".to_string(),
            "SubagentStart".to_string(),
            "SubagentStop".to_string(),
        ],
        categories: vec![
            "outcome".to_string(),
            "lesson-learned".to_string(),
            "decision".to_string(),
            "convention".to_string(),
            "pattern".to_string(),
            "procedure".to_string(),
            "duties".to_string(),
            "reference".to_string(),
        ],
        rules: vec![],
    }
```

The categories must include all 8 `INITIAL_CATEGORIES` from `CategoryAllowlist` (C-10).

## DomainPack Struct

```
#[derive(Debug, Clone)]
pub struct DomainPack:
    pub source_domain: String
    pub event_types: Vec<String>
    pub categories: Vec<String>
    pub rules: Vec<RuleDescriptor>
```

## DomainPackRegistry

```
#[derive(Debug, Clone)]
pub struct DomainPackRegistry:
    inner: Arc<RwLock<HashMap<String, DomainPack>>>
```

### DomainPackRegistry::with_builtin_claude_code()

Always-called constructor that loads only the built-in claude-code pack:

```
pub fn with_builtin_claude_code() -> Self:
    let mut map = HashMap::new()
    map.insert("claude-code".to_string(), builtin_claude_code_pack())
    DomainPackRegistry { inner: Arc::new(RwLock::new(map)) }
```

### DomainPackRegistry::new(packs: Vec<DomainPack>)

Constructor for server startup that takes the config-supplied packs:

```
pub fn new(packs: Vec<DomainPack>) -> Result<Self, ObserveError>:
    -- Start with the built-in claude-code pack
    let mut map: HashMap<String, DomainPack> = HashMap::new()
    map.insert("claude-code".to_string(), builtin_claude_code_pack())

    for pack in packs:
        -- Validate source_domain is not reserved "unknown"
        if pack.source_domain == "unknown":
            return Err(ObserveError::InvalidSourceDomain {
                domain: "unknown".to_string()
            })

        -- Validate source_domain regex: ^[a-z0-9_-]{1,64}$
        if not validate_source_domain_format(&pack.source_domain):
            return Err(ObserveError::InvalidSourceDomain {
                domain: pack.source_domain.clone()
            })

        -- Validate all rule descriptors in the pack
        for rule in &pack.rules:
            validate_rule_descriptor(rule, &pack.source_domain)?

        -- Insert (overrides built-in claude-code if source_domain == "claude-code")
        map.insert(pack.source_domain.clone(), pack)

    Ok(DomainPackRegistry { inner: Arc::new(RwLock::new(map)) })
```

### DomainPackRegistry::lookup(source_domain: &str)

Read-lock lookup. Returns a clone to avoid holding the lock:

```
pub fn lookup(&self, source_domain: &str) -> Option<DomainPack>:
    let guard = self.inner.read().unwrap_or_else(|e| e.into_inner())
    guard.get(source_domain).cloned()
```

### DomainPackRegistry::rules_for_domain(source_domain: &str)

Returns `RuleEvaluator` instances for all DSL rules in the given domain pack.
Built-in claude-code rules are NOT returned here (they are in `default_rules()`).

```
pub fn rules_for_domain(&self, source_domain: &str) -> Vec<Box<dyn DetectionRule>>:
    let guard = self.inner.read().unwrap_or_else(|e| e.into_inner())
    match guard.get(source_domain):
        None => return vec![]
        Some(pack) =>
            pack.rules.iter()
                .map(|descriptor| Box::new(RuleEvaluator::new(descriptor.clone())) as Box<dyn DetectionRule>)
                .collect()
```

### DomainPackRegistry::resolve_source_domain(event_type: &str)

Given a raw event_type string, return the source_domain for the domain whose
`event_types` list contains it. Returns `"unknown"` if no registered domain claims it.

```
pub fn resolve_source_domain(&self, event_type: &str) -> String:
    let guard = self.inner.read().unwrap_or_else(|e| e.into_inner())
    for (domain, pack) in guard.iter():
        -- Empty event_types means all event types match for this domain
        if pack.event_types.is_empty() || pack.event_types.iter().any(|et| et == event_type):
            return domain.clone()
    "unknown".to_string()
```

Note on EC-07 (overlapping event_type across packs): iteration order of HashMap is
non-deterministic. For W1-5 this is acceptable because the hook ingress path always
assigns `source_domain = "claude-code"` directly — `resolve_source_domain` is only
called for records that do NOT come from the hook path. Document this in the method's
docstring.

## Validation Helpers

### validate_source_domain_format(domain: &str) -> bool

```
fn validate_source_domain_format(domain: &str) -> bool:
    -- Regex: ^[a-z0-9_-]{1,64}$
    if domain.is_empty() || domain.len() > 64:
        return false
    domain.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
```

No regex crate needed — manual char check is sufficient and avoids a new dependency.

### validate_rule_descriptor(descriptor: &RuleDescriptor, pack_domain: &str) -> Result<(), ObserveError>

See rule-dsl-evaluator.md for this function — it is defined alongside `RuleDescriptor`.

## lib.rs Change

Add to `crates/unimatrix-observe/src/lib.rs`:
```
pub mod domain;
```

Also add re-exports if needed by consuming crates:
```
pub use domain::{DomainPack, DomainPackRegistry};
```

## types.rs Change

Remove the `HookType` re-export from `crates/unimatrix-observe/src/types.rs`:
```
-- DELETE: pub use unimatrix_core::observation::HookType;
-- KEEP: pub use unimatrix_core::observation::{ObservationRecord, ObservationStats, ParsedSession};
```

## Error Handling

- `new()` returns `Result<Self, ObserveError>` — startup failures propagate to server
  startup and cause the server to refuse to start (FM-01).
- `lookup()`, `rules_for_domain()`, `resolve_source_domain()` are infallible — they
  return `Option` or a default value.
- RwLock poison recovery: use `.unwrap_or_else(|e| e.into_inner())` (established
  pattern from `CategoryAllowlist`).

## Key Test Scenarios

1. **with_builtin_claude_code()**: contains exactly the "claude-code" pack;
   `lookup("claude-code")` returns `Some`.

2. **new() with valid additional pack**: pack is registered; `lookup("sre")` returns `Some`.

3. **Reserved domain rejection (EC-04)**: `new()` with a pack having
   `source_domain = "unknown"` returns `Err(InvalidSourceDomain)`.

4. **Invalid domain format (AC-07)**: `new()` with `source_domain = "SRE"` (uppercase)
   returns `Err(InvalidSourceDomain)`.

5. **resolve_source_domain known event**: `"PreToolUse"` resolves to `"claude-code"`.

6. **resolve_source_domain unknown event**: `"incident_opened"` (with no registered sre pack)
   resolves to `"unknown"`.

7. **EC-05 empty event_types**: pack with `event_types = []` — `resolve_source_domain`
   returns that domain for any event_type string.

8. **rules_for_domain no DSL rules**: claude-code pack has no DSL rules; returns `vec![]`.

9. **AC-08 structural assertion**: no public write method on `DomainPackRegistry` other
   than `new()` / `with_builtin_claude_code()`.

10. **R-10 CategoryAllowlist idempotency**: test that adding a pack whose categories
    overlap with `INITIAL_CATEGORIES` does not fail or duplicate entries (this is
    tested at the server startup level, not in this module directly).
