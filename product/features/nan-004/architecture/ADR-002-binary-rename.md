## ADR-002: Binary Rename from unimatrix-server to unimatrix

### Context

The current binary is named `unimatrix-server` (defined in `crates/unimatrix-server/Cargo.toml` as `[[bin]] name = "unimatrix-server"`). The SCOPE resolves (Q1) that the binary should be renamed to `unimatrix` with subcommands: `unimatrix hook <event>`, `unimatrix export`, `unimatrix import`, and server mode as the default (no subcommand).

This rename is a breaking change for:
- This repository's `.claude/settings.json` (7 hook commands reference `unimatrix-server`)
- This repository's `.mcp.json` (references `unimatrix-server` binary path)
- The existing sync CLI subcommand pattern (Unimatrix entries #1102, #1104, #1160) which documents adding subcommands to `unimatrix-server`

SR-06 flags this as medium severity / high likelihood.

### Decision

Rename the binary in a single atomic change:

1. Update `crates/unimatrix-server/Cargo.toml`:
   ```toml
   [[bin]]
   name = "unimatrix"
   path = "src/main.rs"
   ```

2. Update `main.rs` CLI definition:
   ```rust
   #[command(name = "unimatrix", about = "Unimatrix knowledge engine")]
   ```

3. Update `.mcp.json` in this repository to use the new binary name/path.

4. Update `.claude/settings.json` in this repository to use `unimatrix hook <Event>` in all 7 hook commands.

5. Add new subcommands to the `Command` enum:
   - `Version` — prints version string and exits (sync path, no tokio).
   - `ModelDownload` — downloads ONNX model to cache (sync path).

All changes ship in a single commit. The crate name remains `unimatrix-server` (Cargo crate rename is a separate concern and not needed for nan-004).

No backward compatibility shim for the old `unimatrix-server` binary name. This is acceptable because:
- There are no external consumers yet (no published releases).
- The rename happens before the first npm publish.
- The Unimatrix project itself is the only user, and its configs are updated atomically.

### Consequences

**Easier:**
- Clean command namespace: `unimatrix init`, `unimatrix hook`, `unimatrix export`.
- npm `bin` field maps naturally to `unimatrix`.
- Aligns with the product name (Unimatrix, not Unimatrix Server).

**Harder:**
- All references to `unimatrix-server` in scripts, docs, and Unimatrix knowledge entries become stale. Knowledge entries should be corrected via `context_correct` after the rename ships.
- The existing sync CLI subcommand pattern (#1102, #1104, #1160) must be updated to reference the new binary name.
- `cargo build` output changes from `target/release/unimatrix-server` to `target/release/unimatrix`.
