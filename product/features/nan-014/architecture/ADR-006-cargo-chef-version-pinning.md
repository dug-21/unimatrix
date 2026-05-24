## ADR-006: cargo-chef Version Pinning with --locked

### Context

SR-02 identifies that `cargo install cargo-chef` inside the Dockerfile installs the latest version from crates.io without version pinning or integrity verification. A compromised or buggy new release would silently affect every container build.

cargo-chef is a build-time tool that generates a `recipe.json` from the workspace's `Cargo.toml`/`Cargo.lock` files, enabling Docker layer caching for dependency compilation. It runs twice in the Dockerfile: once in the planner stage and once in the builder stage.

Options considered:

1. **No pin**: `cargo install cargo-chef` — always latest. Maximum convenience, zero reproducibility.
2. **Version pin only**: `cargo install cargo-chef --version 0.1.71` — reproducible version, but resolved dependencies may drift.
3. **Version pin + --locked**: `cargo install cargo-chef --version 0.1.71 --locked` — pinned version with lockfile-verified dependencies.
4. **Pre-built binary download with SHA-256**: Download a pre-compiled binary from cargo-chef's releases. Full integrity verification but adds download complexity and architecture handling.

### Decision

Use version pin + `--locked` in both Dockerfile stages:

```dockerfile
RUN cargo install cargo-chef --version 0.1.71 --locked
```

`--locked` ensures the resolved dependency tree matches the published `Cargo.lock` in the cargo-chef crate. This prevents a scenario where a cargo-chef dependency is yanked and replaced with a different version that changes behavior.

The version number (`0.1.71`) is captured at implementation time as the latest stable release. Updates are manual and deliberate — bump the version in the Dockerfile when upgrading.

cargo-chef is installed in both the planner and builder stages because Docker stages do not share filesystem state. The `cargo install` is cached by Docker layer caching — it rebuilds only when the Dockerfile `RUN` instruction changes (i.e., when the version is bumped).

### Consequences

- **Easier**: Reproducible builds — same cargo-chef version on every build, every architecture, every CI run.
- **Easier**: `--locked` catches dependency supply chain issues in cargo-chef itself.
- **Easier**: Version bumps are visible in the Dockerfile diff, creating an auditable upgrade trail.
- **Harder**: cargo-chef must be installed from source in each stage (~30s compile time). Acceptable: this is cached by Docker layer caching and only rebuilds on version bump.
- **Harder**: No binary-level integrity verification (SHA-256 of the compiled binary). Mitigated by: `--locked` ensures deterministic dependency resolution; the Rust compiler + locked deps produces a deterministic build. Full binary attestation would require a separate verification pipeline, which is disproportionate for a build tool.
