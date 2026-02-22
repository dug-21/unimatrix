# C1: Crate Setup -- Test Plan

## Verification

AC-01 and AC-17 are verified by build checks, not runtime tests.

### Build Verification
- `cargo build --workspace` succeeds with zero errors.
- `crates/unimatrix-vector/Cargo.toml` exists.
- Crate is `edition = "2024"` (via workspace).
- `#![forbid(unsafe_code)]` present in `lib.rs`.

### Compile-time Check (in lib.rs or index.rs tests)
```rust
#[test]
fn test_forbid_unsafe_code() {
    // This test is a documentation marker.
    // The real enforcement is #![forbid(unsafe_code)] at crate level.
    // Any unsafe block anywhere in the crate causes a compilation failure.
}
```

## Risks Covered
None. C1 is structural scaffolding.
