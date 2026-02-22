---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
---

# Rust Workspace


## Build Commands

```bash
# Build: first error + summary (truncate to prevent context bloat)
cargo build --workspace 2>&1 | grep -A5 "^error" | head -20
cargo build --workspace 2>&1 | tail -3

# Test: summary only
cargo test --workspace 2>&1 | tail -30

# Clippy: first warnings only
cargo clippy --workspace -- -D warnings 2>&1 | head -30
```

## Naming Conventions

| Element | Convention | Example |
|---------|------------|---------|
| Modules | snake_case | `http_polling_source.rs` |
| Structs | PascalCase | `HttpPollingSource` |
| Functions | snake_case | `fetch_data()` |
| Constants | SCREAMING_SNAKE | `DEFAULT_TIMEOUT` |
| Traits | PascalCase | `ResponseParser` |

## Code Quality

- `cargo fmt` before commit
- `cargo clippy` — no warnings
- Error handling uses the project error type with `.map_err()` context
- Logging uses `tracing` macros (info!, error!, debug!)
- No `.unwrap()` in non-test code
- No hardcoded secrets (use env vars)
