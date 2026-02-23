## ADR-003: Category Allowlist as Runtime-Extensible HashSet

### Context

`context_store` must reject unknown categories. The initial set is `{outcome, lesson-learned, decision, convention, pattern, procedure}`. The product vision specifies that vnc-003 can add categories without code changes. Three options:
1. **Rust enum**: Compile-time exhaustive, but requires code changes to extend
2. **Configuration file**: Flexible but requires file I/O and config management
3. **Runtime-extensible data structure**: In-memory, lockable, supports dynamic addition

The scope explicitly states "runtime-extensible structure (not hardcoded enum)."

### Decision

Use `RwLock<HashSet<String>>` wrapped in a `CategoryAllowlist` struct, stored on `UnimatrixServer` as `Arc<CategoryAllowlist>`. The initial set is populated in `CategoryAllowlist::new()`. Runtime extension is via `add_category(String)`. Categories are case-sensitive, lowercase by convention.

The allowlist is NOT persisted to redb in vnc-002. It is rebuilt from the hardcoded initial set on each server startup. vnc-003 may choose to persist custom categories to the database, loading them on startup and merging with the hardcoded set.

Validation error messages include the complete list of valid categories, so agents can self-correct.

### Consequences

**Easier:**
- No code changes needed to add categories at runtime (vnc-003 calls `add_category`)
- `RwLock` allows concurrent reads (validation is the hot path) with rare writes
- No config files, no persistence complexity
- Error messages with valid category list make the system self-documenting

**Harder:**
- Categories reset to the hardcoded set on server restart (no persistence in vnc-002)
- Case-sensitive matching means "Decision" and "decision" are different -- agents must use lowercase
- No category metadata (description, allowed operations) -- just a set of strings
