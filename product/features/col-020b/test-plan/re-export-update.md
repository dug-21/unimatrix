# Test Plan: C7 — Re-export Update

**File:** `crates/unimatrix-observe/src/lib.rs`
**Risks:** R-07 (missed import site)

## Verification Method

Compilation gate only. No dedicated unit tests needed.

### Compilation Verification

```bash
cargo build --workspace
```

If any import site still references `KnowledgeReuse` instead of `FeatureKnowledgeReuse`, the compiler will produce an error. This is a compile-time guarantee.

### Pre-Implementation Check

Before implementing, grep the workspace for all `KnowledgeReuse` references to identify every site that needs updating:

```bash
grep -rn "KnowledgeReuse" crates/
```

Expected sites:
- `crates/unimatrix-observe/src/types.rs` -- struct definition
- `crates/unimatrix-observe/src/lib.rs` -- re-export
- `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` -- return type
- `crates/unimatrix-server/src/mcp/tools.rs` -- import and usage

## Risk Coverage

- R-07: Compilation is a complete gate. If the re-export or any import is missed, the build fails immediately. No silent failure path exists.
