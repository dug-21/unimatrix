## ADR-001: `SessionIdentitySource` as a Resolved Enum, Not a Trait Object

### Context

SR-04 (High risk) requires a named abstraction boundary between "how we learn who is
connecting" and "what we do with that identity." Without this boundary, W2-2/W2-3 OAuth
must modify startup logic in both `tokio_main_daemon()` and `tokio_main_stdio()` — and
potentially `build_context()` — rather than swapping a single component.

The abstraction must:
1. Allow the env-var implementation to be replaced by JWT claim extraction in W2-2
   without touching capability resolution or audit attribution logic
2. Be resolved once at startup (STDIO/daemon) or once per connection (HTTP in W2-2)
3. Produce a `ValidatedAgentId` newtype that carries proof of validation so downstream
   code cannot accidentally use an unvalidated raw `String`
4. Be simple enough that a spec writer can write acceptance criteria against it

Two shapes were considered: **trait object** (`dyn SessionIdentitySource`) and **enum**
(`SessionIdentitySource { EnvVar, JwtClaims { .. } }`).

**Trait object arguments:**
- Open extension: any code can add a new source without touching the enum
- Natural for dependency injection in tests

**Enum arguments:**
- The variant set is small and closed: env var (W0-2), JWT claims (W2-2), config file
  (W0-3 optional). There is no scenario where a third-party crate provides a new source.
- Enum variants are listed in one file — a reader scanning for "how does this resolve
  identity" finds one exhaustive list, not scattered `impl` blocks
- No `dyn` dispatch, no `Box<dyn>`, no lifetime parameters
- `resolve()` method on the enum is as easy to mock in tests as a trait (pass a known
  variant with a known env var value set)
- The W2-2 extension is additive: add one variant, one `resolve()` match arm. No
  existing code changes.
- Trait objects require `Send + Sync + 'static` for async startup — adds complexity
  with no benefit given the closed variant set

### Decision

`SessionIdentitySource` is an **enum** with a `resolve() -> Result<ValidatedAgentId, SessionIdentityError>` method:

```rust
pub enum SessionIdentitySource {
    EnvVar,
    JwtClaims { token: String },  // reserved for W2-2; not implemented in alc-003
}
```

`ValidatedAgentId` is a newtype (`struct ValidatedAgentId(String)`) that can only be
constructed by `SessionIdentitySource::resolve()`. This makes it impossible to pass an
unvalidated string into `enroll_session_agent()`.

`SessionIdentityError` is an enum with variants `Missing`, `Invalid`, and `ProtectedName`
so startup can emit a specific, actionable error message (AC-04, AC-07).

The module lives in `crates/unimatrix-server/src/mcp/session_identity.rs`. It is
`pub(crate)` — not part of the public API of any crate.

W2-2 adds `SessionIdentitySource::JwtClaims { token: String }` and its `resolve()`
implementation. No other file changes for the abstraction itself.

### Consequences

**Easier:**
- W2-2 has a single, named replacement target with clear acceptance criteria
- The `ValidatedAgentId` newtype gives compile-time proof that validation occurred;
  no runtime panic path from passing raw agent_id strings
- Exhaustive `match` on the enum means the compiler enforces that W2-2 handles its
  new variant everywhere it is used

**Harder:**
- The `JwtClaims` variant is present in the enum but unimplemented in alc-003; it must
  carry `#[allow(dead_code)]` until W2-2 ships
- The enum cannot be extended by external crates (acceptable — this is intentional;
  identity sources are an operator concern, not a plugin point)
