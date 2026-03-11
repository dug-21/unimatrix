## ADR-004: Configurable Noisy Trust Sources

### Context

The SCOPE defines "Noisy" classification as `trust_source="auto"` only. SR-05 notes that neural-extracted entries (`trust_source="neural"`) share similar quality concerns, and limiting the Noisy filter to "auto" may miss a growing category.

Two approaches:
- A) Hardcode `trust_source == "auto"` check in the classifier
- B) Define a constant array `NOISY_TRUST_SOURCES: &[&str]` that the classifier checks via `.contains()`

### Decision

**Option B: Array constant `NOISY_TRUST_SOURCES`.**

```rust
/// Trust sources eligible for Noisy classification.
/// An entry is Noisy if: trust_source is in this list AND helpful_count == 0
/// AND injection_count >= 1.
pub const NOISY_TRUST_SOURCES: &[&str] = &["auto"];
```

The classifier checks `NOISY_TRUST_SOURCES.contains(&trust_source)` instead of `trust_source == "auto"`. Adding "neural" later requires changing one constant.

The constant is deliberately kept minimal (just "auto") for the initial release, matching the SCOPE. Adding "neural" is a follow-on decision when neural extraction volume justifies it.

### Consequences

- Single constant to modify when trust source policy evolves
- No runtime configuration needed — this is a code-level policy decision
- The `.contains()` call on a small slice is negligible cost
- Test coverage includes both included and excluded trust sources
