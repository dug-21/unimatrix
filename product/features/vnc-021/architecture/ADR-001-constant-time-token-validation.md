## ADR-001: Constant-Time Token Validation via subtle::ConstantTimeEq

### Context

The static bearer token is a 256-bit secret (32 bytes, 64 hex chars). HTTP requests present the token in the `Authorization: Bearer <hex>` header. The server must compare the presented token against the stored token. A naive `==` comparison leaks timing information proportional to the length of the matching prefix, enabling a statistical timing attack to recover the token byte-by-byte over many requests (Bleichenbacher-style).

The `subtle` crate (v2.x) provides `ConstantTimeEq` which runs in fixed time regardless of input content. `subtle` is already a transitive dependency via `rustls` in the lockfile, so promoting it to a direct dependency adds no new crate downloads (Unimatrix #4661).

Alternatives considered:
- **Hash-then-compare** (SHA-256 both sides, compare hashes): Adds computation overhead and does not eliminate timing if the hash comparison itself is non-constant-time. Still requires `subtle` for the hash comparison to be safe.
- **Double-HMAC** (HMAC both sides with a random key, compare MACs): More robust theoretically but over-engineered for a static token comparison. Adds key management complexity.

### Decision

Use `subtle::ConstantTimeEq` to compare the raw token bytes directly. The comparison happens after parsing the `Authorization` header but before any request processing. Early-return on missing or malformed headers (wrong prefix, non-hex) is acceptable because those paths do not leak information about the token value.

Implementation requirements:
1. Parse header: extract bytes after `"Bearer "` prefix
2. Hex-decode the presented token to `[u8; 32]`
3. Compare via `token.ct_eq(&stored_token)` -- returns `subtle::Choice`
4. Convert `Choice` to bool via `.into()`

The `subtle` crate must be listed as a direct dependency in `Cargo.toml` to make the security dependency explicit, even though it is already transitive.

### Consequences

Easier: Token comparison is provably constant-time. No custom crypto. The `subtle` crate is widely audited and used by rustls itself.

Harder: Developers must not introduce early-return optimizations in the comparison path. The `subtle::ConstantTimeEq` usage must be preserved across refactors -- a code review checkpoint.
