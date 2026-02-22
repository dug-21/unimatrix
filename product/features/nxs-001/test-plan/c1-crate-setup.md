# C1: Crate Setup Test Plan

## AC-01: Cargo Workspace Compiles

### Test: Build verification
- `cargo build --workspace` succeeds with zero errors and zero warnings
- Cargo.toml at repo root defines [workspace] with unimatrix-store member
- Crate is edition = "2024"
- `#![forbid(unsafe_code)]` compiles (no unsafe blocks anywhere)

This is verified by the build step itself, not a unit test.
