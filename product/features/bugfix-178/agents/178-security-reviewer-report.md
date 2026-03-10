# Security Review: 178-security-reviewer

## Risk Level: low

## Summary

This PR introduces a `RateLimitConfig` struct to make rate limiter thresholds configurable, enabling tests to use permissive limits (`u32::MAX`) instead of hitting production rate limits. The change is minimal, well-scoped, and preserves all production defaults via the delegation pattern (`new()` calls `with_rate_config()` with `Default`).

## Findings

### Finding 1: No input validation on RateLimitConfig values
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/gateway.rs:112-116`
- **Description**: `RateLimitConfig` accepts any `u32`/`u64` values including zero. A `window_secs: 0` would create a zero-duration window where every request immediately evicts, effectively disabling rate limiting. A `search_limit: 0` or `write_limit: 0` would reject all requests.
- **Recommendation**: Not blocking because (a) `RateLimitConfig` is `pub(crate)` so only internal code can construct it, (b) all production paths use `Default` which has sane values, and (c) the `with_rate_config` constructor is `pub(crate)`. If this config is ever exposed externally, add validation.
- **Blocking**: no

### Finding 2: tempdir leak in test via std::mem::forget
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/gateway.rs:719`
- **Description**: The `with_rate_config_uses_custom_limits` test uses `std::mem::forget(dir)` to keep the tempdir alive. This is a pre-existing pattern (used in `new_permissive()` and `make_limited_gateway()`), not newly introduced by this PR. It leaks temporary directories but only in tests.
- **Recommendation**: Consistent with existing test infrastructure. No action needed.
- **Blocking**: no

## Blast Radius Assessment

**Worst case**: If `RateLimitConfig::default()` returned wrong values, production rate limiting would be misconfigured. However, the `default_rate_limit_config_matches_production` test explicitly asserts the values (300, 60, 3600), catching any accidental change. The delegation from `new()` to `with_rate_config()` is straightforward and verified by the existing test suite.

**Affected components**: `SecurityGateway`, `ServiceLayer`, `TestHarness`. All production code paths (`main.rs`, `server.rs`, `listener.rs`, `shutdown.rs`) call `ServiceLayer::new()` which uses `RateLimitConfig::default()`. No production behavior change.

## Regression Risk

Low. The change is purely additive:
- `ServiceLayer::new()` signature unchanged -- all 4 production callers compile without modification
- `SecurityGateway::new()` signature unchanged
- New `with_rate_config` methods are only called from test infrastructure
- `RateLimitConfig` is `pub(crate)`, not exposed in public API

## OWASP Assessment

| Check | Result |
|-------|--------|
| Input validation | Config is internal-only, not from external input. Default validated by test. |
| Access control | `pub(crate)` visibility prevents external construction of permissive configs. |
| Injection | No new external inputs. |
| Deserialization | No deserialization of config. |
| Error handling | Unchanged. |
| Dependencies | No new dependencies. |
| Secrets | No secrets in diff. |

## PR Comments
- Posted 1 approval comment on PR #180
- Blocking findings: no
