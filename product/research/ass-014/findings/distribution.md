# D14-5: Distribution Strategy — Cortical Implant

**Research Question:** RQ-5 — How does the cortical implant get onto every machine that needs it, across platforms, with minimal friction?

**Date:** 2026-03-01
**Status:** Complete
**Answers:** RQ-5a through RQ-5f

---

## 1. Binary Architecture Options

### 1.1 Architecture Decision: Implant with ONNX vs. IPC-Only

The cortical implant's binary size depends entirely on whether it links `unimatrix-embed` (which pulls in ONNX Runtime) or communicates with the MCP server via IPC for embedding operations.

**Option A: Full Engine Binary (links unimatrix-core/store/embed/vector directly)**

The implant would include the complete Unimatrix engine stack:
- Rust application logic: ~3-5 MB (redb, hnsw_rs, bincode, serde, tokio, clap)
- ONNX Runtime shared library (`libonnxruntime.so`): ~14 MB (dynamic linking) or ~96 MB (static archive)
- all-MiniLM-L6-v2 ONNX model: ~87 MB
- Tokenizer: ~456 KB

Measured from the current build environment:
- `unimatrix-server` release binary (Linux aarch64): **17 MB** (dynamically linked against ONNX Runtime)
- `libonnxruntime.so.1.20.1` (Linux aarch64): **14 MB** (must ship alongside or statically link)
- `model.onnx`: **87 MB** (must be downloaded separately or bundled)

**Total distribution size for Option A: ~31 MB binary + 87 MB model = ~118 MB**

This is the same footprint as the MCP server itself. Shipping two copies of the full engine doubles the installation burden.

**Option B: IPC Client Binary (transport + serialization only)**

The implant communicates with the running MCP server via Unix domain socket or similar IPC:
- Rust application logic: ~2-3 MB (serde, serde_json, clap, tokio-net)
- No ONNX Runtime dependency
- No model files
- No redb, no hnsw_rs

**Estimated binary size for Option B: ~3-5 MB**

**Option C: Bundled Subcommand (`unimatrix hook ...`)**

The implant is a subcommand of the existing `unimatrix-server` binary:
- Zero additional binary — already installed
- Already has the full engine linked
- Binary grows by ~50-100 KB for the hook dispatch logic

**Net additional distribution size for Option C: ~0 MB**

### 1.2 Recommendation

**Option C (bundled subcommand) is strongly recommended as primary**, with Option B as fallback for standalone deployment.

Rationale:
1. Zero additional installation if the MCP server is already deployed
2. Version coupling is automatic — implant and server always match
3. Single distribution target instead of two
4. The MCP server binary already exists at 17 MB; adding hook dispatch is negligible
5. The `unimatrix-server hook <event>` subcommand can be the hook command referenced in `.claude/settings.json`

Option B remains viable for scenarios where the MCP server runs remotely and the implant needs to be a lightweight local agent. This is the local-to-centralized transition path — but it is a future concern, not an immediate one.

### 1.3 Platform Targets

| Platform | Triple | Priority | Notes |
|----------|--------|----------|-------|
| Linux x86_64 | `x86_64-unknown-linux-gnu` | P0 | Primary CI/server target, Codespaces |
| Linux aarch64 | `aarch64-unknown-linux-gnu` | P0 | Apple Silicon Docker, Graviton, current dev container |
| macOS aarch64 | `aarch64-apple-darwin` | P0 | Apple Silicon native development |
| macOS x86_64 | `x86_64-apple-darwin` | P1 | Legacy Intel Macs (declining market) |
| Windows x86_64 | `x86_64-pc-windows-msvc` | P2 | Windows development via native or WSL |

The P0 targets cover >95% of the current Claude Code user base. The Linux aarch64 target is validated by the current dev container build. ONNX Runtime provides prebuilt binaries for all five targets.

### 1.4 Cross-Compilation Strategy

**Recommended: cargo-zigbuild for Linux targets + native builds for macOS/Windows**

- **Linux targets**: `cargo zigbuild` uses Zig as a cross-linker, producing portable binaries with configurable glibc version. Builds from a single Linux host can target both x86_64 and aarch64. Build times drop from 15+ minutes (Docker-based cross-rs) to under 2 minutes.
- **macOS targets**: Native `cargo build` on macOS runners (GitHub Actions `macos-14` for aarch64, `macos-13` for x86_64). ONNX Runtime links against system frameworks.
- **Windows targets**: Native `cargo build` on `windows-latest` GitHub Actions runner.

**Alternative: cross-rs** — uses Docker containers with pre-configured cross toolchains. More established but slower and does not publish Linux ARM binary releases as of 2025. Not recommended as primary due to performance and ARM limitations.

**GitHub Actions matrix:**
```yaml
strategy:
  matrix:
    include:
      - target: x86_64-unknown-linux-gnu
        os: ubuntu-latest
        tool: zigbuild
      - target: aarch64-unknown-linux-gnu
        os: ubuntu-latest
        tool: zigbuild
      - target: aarch64-apple-darwin
        os: macos-14
        tool: cargo
      - target: x86_64-apple-darwin
        os: macos-13
        tool: cargo
      - target: x86_64-pc-windows-msvc
        os: windows-latest
        tool: cargo
```

---

## 2. Distribution Mechanism Comparison

### 2.1 Detailed Analysis

#### Option 1: npm with Native Binaries (esbuild/turbo/biome model)

**How it works:** A base npm package (`@unimatrix/cortical`) declares platform-specific packages as `optionalDependencies`. Each platform package contains a single prebuilt binary with `os` and `cpu` fields in its `package.json` so npm only installs the matching one. A thin JavaScript wrapper (`bin/unimatrix-hook`) detects the platform and executes the native binary.

**Package structure:**
```
@unimatrix/cortical                    # base package (~5 KB)
  optionalDependencies:
    @unimatrix/cortical-linux-x64      # 17 MB binary
    @unimatrix/cortical-linux-arm64    # 17 MB binary
    @unimatrix/cortical-darwin-arm64   # 17 MB binary
    @unimatrix/cortical-darwin-x64     # 17 MB binary
    @unimatrix/cortical-win32-x64     # 17 MB binary
```

Each platform package's `package.json`:
```json
{
  "name": "@unimatrix/cortical-linux-x64",
  "version": "0.1.0",
  "os": ["linux"],
  "cpu": ["x64"]
}
```

**Precedent:** This pattern is proven at scale by esbuild (28+ platform packages, the original pioneer), Turborepo (turbo-darwin-arm64, turbo-linux-x64, etc.), Biome, SWC, and Sentry CLI. Sentry's engineering blog documents the dual-fallback strategy: `optionalDependencies` as primary + `postinstall` download as backup.

**Pros:**
- Ubiquitous: every developer with Node.js already has npm
- Automatic installation: `npm install -D @unimatrix/cortical` installs only the correct platform binary
- Lockfile versioning: team members are pinned to the same version via `package.json`
- Familiar update workflow: `npm update` or Dependabot/Renovate
- Works with npm, yarn, pnpm (all support `os`/`cpu` filtering)
- Can be a `devDependency` — no global install required

**Cons:**
- Requires Node.js runtime (for the wrapper script that locates the binary)
- npm registry has 200 MB uncompressed package size limit (not an issue for a 17 MB binary)
- `optionalDependencies` can be disabled (`--ignore-optional` in yarn)
- `postinstall` scripts can be disabled (security policy)
- Multiple npm packages to publish per release (6 packages: 1 base + 5 platforms)
- Not suitable if adopters do not use Node.js at all

**When both optionalDependencies AND postinstall are disabled:** The installation fails silently. Sentry's recommendation: try both mechanisms (optionalDependencies + postinstall fallback).

| Criterion | Score (1-5) | Notes |
|-----------|-------------|-------|
| Installation friction | 4 | One npm install command |
| Update friction | 5 | npm update, Dependabot, Renovate |
| Platform coverage | 5 | All 5 targets via optionalDependencies |
| Binary size impact | 4 | Only downloads matching platform (~17 MB) |
| Team consistency | 5 | Lockfile pins exact version |

#### Option 2: cargo install

**How it works:** `cargo install unimatrix-server` compiles from source on the developer's machine.

**Pros:**
- Standard Rust distribution channel
- Source audit possible
- No external infrastructure needed

**Cons:**
- Requires Rust toolchain (~1 GB installed)
- Compilation from source takes 2-5 minutes (ONNX runtime build is slow)
- ONNX Runtime's `ort` crate downloads prebuilt libraries during build (network dependency)
- Not viable for non-Rust teams
- Version pinning requires manual coordination

**Mitigated by cargo-binstall:** `cargo binstall unimatrix-server` downloads prebuilt binaries from GitHub Releases instead of compiling. Falls back to `cargo install` if no binary available. However, requires `cargo-binstall` to be installed first.

| Criterion | Score (1-5) | Notes |
|-----------|-------------|-------|
| Installation friction | 2 | Requires Rust toolchain or cargo-binstall |
| Update friction | 2 | Manual `cargo install --force` |
| Platform coverage | 4 | Any target Rust supports |
| Binary size impact | 3 | Full compilation, includes debug info unless configured |
| Team consistency | 1 | No lockfile, manual version coordination |

#### Option 3: GitHub Releases + Install Script

**How it works:** Prebuilt binaries are uploaded as GitHub Release assets. A `curl | sh` install script detects the platform and downloads the correct binary.

```bash
curl -sSL https://raw.githubusercontent.com/org/unimatrix/main/install.sh | sh
```

**Pros:**
- No runtime dependency (pure binary download)
- Works on any platform with curl/wget
- Platform detection is straightforward (uname -s, uname -m)
- Simple infrastructure (GitHub Releases are free)

**Cons:**
- Manual updates (re-run install script or check for new version)
- `curl | sh` has security concerns (MITM, script modification)
- No lockfile — teams must coordinate versions manually
- PATH installation may require sudo or manual config
- No automatic cleanup on uninstall

| Criterion | Score (1-5) | Notes |
|-----------|-------------|-------|
| Installation friction | 3 | One curl command, but may need PATH config |
| Update friction | 2 | Manual re-run of install script |
| Platform coverage | 5 | Any platform with curl |
| Binary size impact | 5 | Just the binary |
| Team consistency | 1 | No version pinning mechanism |

#### Option 4: Package Managers (Homebrew, apt, winget)

**How it works:** Native package manager distribution — Homebrew tap for macOS/Linux, apt repository for Debian/Ubuntu, winget manifest for Windows.

**Homebrew:**
```bash
brew tap unimatrix/tap
brew install unimatrix
```

**Pros:**
- Native package management experience per platform
- Auto-updates via platform mechanisms (`brew upgrade`)
- Dependency resolution handled by package manager

**Cons:**
- Multiple packaging formats to maintain (Homebrew formula, Debian package, winget manifest)
- Homebrew review process can be slow for initial listing
- apt repository requires hosting infrastructure
- No single lockfile across platforms
- Fragmented — team members on different OSes use different commands

| Criterion | Score (1-5) | Notes |
|-----------|-------------|-------|
| Installation friction | 4 | One package manager command |
| Update friction | 4 | Native update mechanisms |
| Platform coverage | 3 | Requires per-platform packaging work |
| Binary size impact | 5 | Just the binary + metadata |
| Team consistency | 2 | No cross-platform lockfile |

#### Option 5: Bundled with MCP Server (`unimatrix hook ...` subcommand)

**How it works:** The cortical implant is a subcommand of the existing `unimatrix-server` binary. Hook configuration in `.claude/settings.json` references the same binary:

```json
{
  "hooks": {
    "UserPromptSubmit": [{
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook UserPromptSubmit"
      }]
    }]
  }
}
```

**Pros:**
- Zero additional installation — if MCP server works, hooks work
- Version always matches server — no compatibility issues
- Single distribution target (whatever mechanism ships the server)
- Single binary to maintain, test, and security-audit
- Simplest mental model for users

**Cons:**
- Couples implant lifecycle to server releases
- Server binary grows slightly (hook dispatch logic: ~50-100 KB)
- If server is installed globally but project needs different version, conflicts arise
- Cannot run hooks independently of server installation

| Criterion | Score (1-5) | Notes |
|-----------|-------------|-------|
| Installation friction | 5 | Already installed |
| Update friction | 5 | Updated with server |
| Platform coverage | 5 | Same as server |
| Binary size impact | 5 | Negligible addition |
| Team consistency | 5 | Same binary, same version |

### 2.2 Comparison Matrix

| Criterion | npm | cargo install | GitHub Releases | Package Managers | Bundled |
|-----------|-----|---------------|-----------------|------------------|---------|
| Installation friction | 4 | 2 | 3 | 4 | **5** |
| Update friction | 5 | 2 | 2 | 4 | **5** |
| Platform coverage | 5 | 4 | 5 | 3 | **5** |
| Binary size impact | 4 | 3 | 5 | 5 | **5** |
| Team consistency | 5 | 1 | 1 | 2 | **5** |
| **Total** | **23** | **12** | **16** | **18** | **25** |

---

## 3. Recommended Distribution Strategy

### 3.1 Primary: Bundled Subcommand (`unimatrix-server hook`)

**The cortical implant ships as a subcommand of the existing MCP server binary.**

This is the clear winner across every criterion. The rationale:

1. **Zero additional installation friction.** Anyone running Unimatrix already has the binary. Adding hook support is a code change to the existing binary, not a new distribution artifact.

2. **Automatic version coupling.** The implant and server share the same Rust workspace, the same `Cargo.toml`, the same `unimatrix-core` traits. They cannot drift because they are the same binary.

3. **Single distribution target.** Whatever mechanism ships `unimatrix-server` (currently: build from source in dev container) automatically ships the implant. When the project matures to formal distribution (M9: nan-001 through nan-004), the implant comes along for free.

4. **Simplest user mental model.** "Install Unimatrix. Everything works." No separate implant binary, no version matrix, no extra packages.

### 3.2 Secondary Distribution: npm with Native Binaries

**For external adoption beyond the Unimatrix dev team, npm distribution is the recommended channel for the combined `unimatrix` binary.**

This is the distribution mechanism for the server+implant binary itself (M9: nan-001 scope), not a separate implant package:

```bash
npm install -D @unimatrix/cli
```

The npm package would follow the esbuild/Sentry pattern:
- Base package `@unimatrix/cli` with a thin JS wrapper
- Platform packages (`@unimatrix/cli-linux-x64`, etc.) as optionalDependencies
- Postinstall fallback download from GitHub Releases
- `os` and `cpu` fields for automatic platform filtering

This defers to M9 (nan-001: CLI Binary, nan-004: Release Automation). The current milestone (M5) focuses on the bundled subcommand approach.

### 3.3 Tertiary: GitHub Releases + cargo-binstall

For Rust developers and CI environments:
- GitHub Releases with prebuilt binaries for all 5 platform targets
- `cargo-binstall` auto-detection of GitHub Release assets
- Install script as convenience wrapper

### 3.4 Distribution Hierarchy

```
Phase 1 (M5 — now):
  unimatrix-server hook <event>
  Built from source in dev container
  Zero additional distribution work

Phase 2 (M9 — nan-001/004):
  npm install -D @unimatrix/cli         # primary external channel
  cargo binstall unimatrix              # Rust developer channel
  curl -sSL .../install.sh | sh         # universal fallback
  brew install unimatrix/tap/unimatrix  # macOS convenience

Phase 3 (future — centralized):
  Centralized unimatrix-server in Docker container (per-org or per-team)
  Cortical implant deployed locally in every repo (project-hash initialization)
  WASM thin client via npm — single .wasm artifact, all platforms
  npm install -D @unimatrix/cortical    # WASM client, no native cross-compilation
  Connects to remote Unimatrix instance via HTTPS
  Node.js is already a Claude Code prerequisite — not an additive dependency
```

### 3.5 Phase 3 Architecture: Centralized Unimatrix with WASM Client

**Decision (2026-03-01):** The centralized deployment model uses a dockerized `unimatrix-server` with WASM-based cortical implant clients.

**Server side:**
- `unimatrix-server` runs in its own Docker container (per-org or per-team deployment)
- Owns redb database, ONNX runtime, HNSW index — all the heavy infrastructure
- Exposes HTTPS endpoint for remote cortical implant connections
- Project isolation via project-hash namespacing (same hash algorithm as local mode)
- Multi-repo: multiple projects connect to the same server, isolated by hash

**Client side (cortical implant):**
- Thin WASM client compiled from Rust to `wasm32-wasip2`
- Distributed via npm: `npm install -D @unimatrix/cortical`
- Single `.wasm` artifact — **eliminates the entire cross-compilation matrix** (no more 5 platform-specific packages)
- ~1-2 MB (just transport + JSON serialization, no ONNX, no redb, no HNSW)
- Runs via Node.js WASI support — Node.js is already a prerequisite for Claude Code, so this is **not an additive runtime dependency**
- Initialized per-repo via project hash: `unimatrix init` computes hash, registers with centralized server, writes hook config

**Why WASM for Phase 3:**
1. **Distribution simplification** — One `.wasm` file replaces 5 native binaries. npm distribution becomes trivial (no `optionalDependencies` platform dance)
2. **Sandboxing** — WASI capabilities are explicitly granted. The client can only access network (to reach server) and stdio (to communicate with Claude Code hooks). Cannot read arbitrary files or processes
3. **Non-additive runtime** — Claude Code requires Node.js. WASI support in Node.js means zero new dependencies
4. **Transport fit** — Phase 3 uses HTTPS (not Unix domain sockets). WASI Preview 2 `wasi:http` is mature for outbound HTTPS. The UDS limitation that would block WASM in Phase 1 is irrelevant in Phase 3

**Why NOT WASM for Phase 1:**
- Phase 1 uses Unix domain sockets (forced by redb exclusive locks + local server)
- WASI UDS support is immature
- Bundled subcommand is zero-friction and already native
- No cross-compilation needed (single dev container build)

**Transition path:**
```
Phase 1: unimatrix-server hook <event>  →  UDS  →  local unimatrix-server  →  local redb
Phase 3: @unimatrix/cortical (.wasm)    →  HTTPS →  dockerized unimatrix-server → centralized redb
```

The Transport trait (RQ-4) abstracts this — the WASM client implements the same trait with an HTTPS backend. Hook event schema, request/response types, and degradation behavior are identical.

---

## 4. Configuration Automation

### 4.1 Hook Configuration Schema

Claude Code hooks are configured in JSON settings files at three scopes:
- `~/.claude/settings.json` — user-global (all projects)
- `.claude/settings.json` — project-level (committed to repo)
- `.claude/settings.local.json` — project-level (gitignored)

The cortical implant's hook configuration would register a single binary across all relevant events:

```json
{
  "hooks": {
    "SessionStart": [{
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook SessionStart"
      }]
    }],
    "UserPromptSubmit": [{
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook UserPromptSubmit"
      }]
    }],
    "PreToolUse": [{
      "matcher": "*",
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook PreToolUse"
      }]
    }],
    "PostToolUse": [{
      "matcher": "*",
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook PostToolUse"
      }]
    }],
    "PreCompact": [{
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook PreCompact"
      }]
    }],
    "SubagentStart": [{
      "matcher": "*",
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook SubagentStart"
      }]
    }],
    "SubagentStop": [{
      "matcher": "*",
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook SubagentStop"
      }]
    }],
    "Stop": [{
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook Stop"
      }]
    }],
    "TaskCompleted": [{
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook TaskCompleted"
      }]
    }],
    "SessionEnd": [{
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook SessionEnd"
      }]
    }]
  }
}
```

**Key observation:** The single-binary router pattern means one `command` value across all events. The binary reads `hook_event_name` from stdin JSON and dispatches internally. This is the same pattern claude-flow uses.

**Coexistence with existing hooks:** The current `.claude/settings.json` already has observation hooks (bash scripts for col-002). The cortical implant configuration would replace these — the implant absorbs the observation role. During migration, both can coexist since Claude Code runs all matching hooks for an event.

### 4.2 Configuration Methods

**Method 1: Manual (highest control, highest friction)**

User edits `.claude/settings.json` directly. Documented in a setup guide.

- Friction: High. User must copy/paste a large JSON block.
- Control: Full. User sees exactly what is configured.
- Recommended for: Early adopters, advanced users.

**Method 2: `unimatrix init` command (medium friction)**

The `unimatrix-server init` subcommand (alc-003 scope) writes the hook configuration:

```bash
unimatrix-server init
# Detects project root
# Writes .claude/settings.json hooks section
# Preserves existing non-Unimatrix hooks
# Reports what was configured
```

Implementation considerations:
- Must merge with existing `.claude/settings.json` (JSON parse, merge hooks object, re-serialize)
- Must not clobber user-defined hooks for the same events (append to hooks array, not replace)
- Should detect if Unimatrix hooks are already configured (idempotent)
- Should offer `--dry-run` to preview changes

- Friction: Low. One command.
- Control: Medium. User can review after.
- Recommended for: Standard setup flow.

**Method 3: Plugin-based (lowest friction, most portable)**

Claude Code supports plugins with `hooks/hooks.json`. A Unimatrix plugin could bundle the hook configuration:

```
.claude/plugins/unimatrix/
  hooks/hooks.json    # hook configuration
  README.md           # plugin description
```

The `hooks.json` uses `${CLAUDE_PLUGIN_ROOT}` for path references. When the plugin is enabled, its hooks automatically merge with user/project hooks.

- Friction: Very low. Enable plugin, done.
- Control: Plugin-managed. User can disable via `/hooks` menu.
- Recommended for: Standardized team deployments.

**Method 4: Auto-configure on MCP server startup (lowest friction, least transparent)**

The MCP server detects missing hook configuration and writes it on first startup.

- Friction: Zero.
- Control: Low — hidden side effect.
- Risk: Modifying settings files without user action violates principle of least surprise.
- Recommended: NOT recommended. Too magical. Users should opt in to hook installation.

### 4.3 Recommendation

**Primary: `unimatrix init` command (Method 2)**, deferred to alc-003 implementation.

**Interim: Manual configuration (Method 1)** with documented JSON to copy-paste.

**Future: Plugin mechanism (Method 3)** when Claude Code's plugin ecosystem matures.

---

## 5. Versioning Strategy

### 5.1 Version Coupling Model

**With the bundled subcommand approach, versioning is trivially solved: the implant IS the server.** They share the same `version` in `Cargo.toml`, the same Git commit, the same release tag. There is no version mismatch scenario.

This changes if/when the implant becomes a separate binary (Phase 3: centralized deployment with standalone IPC client). At that point:

### 5.2 Version Negotiation (Future: Separate Binary)

When the implant separates from the server, a version compatibility protocol is needed:

**Protocol:**
1. Implant sends its version on every IPC request: `{"implant_version": "0.3.0", ...}`
2. Server checks compatibility against its own version
3. Compatible: proceed normally
4. Minor mismatch (e.g., implant 0.3 / server 0.4): warn but proceed. Server may support deprecated implant features.
5. Major mismatch (e.g., implant 0.x / server 1.x): hard fail with clear error message

**Semantic versioning contract:**
- Patch version (0.3.x): always compatible
- Minor version (0.x.0): backward compatible within same major
- Major version (x.0.0): requires matching major version

**Graceful degradation on mismatch:**
- Implant should degrade gracefully, not crash: skip injection, log warning, continue without enhancement
- Hook command exit code 0 on version mismatch (non-blocking) — hooks that exit non-zero with stderr show errors to the user but do not block work

### 5.3 Shared Library Versioning

The implant and server share Rust workspace crates:
- `unimatrix-core` (traits, domain types)
- `unimatrix-store` (redb storage)
- `unimatrix-vector` (HNSW index)
- `unimatrix-embed` (ONNX embedding)

These are internal crates, not published to crates.io. Version alignment is enforced by the workspace `Cargo.toml` — all crates build from the same commit. No separate versioning needed while they share a workspace.

---

## 6. Update & Consistency

### 6.1 Phase 1: Source Build (Current)

Currently, Unimatrix is built from source in the dev container:
```bash
cargo build --release
```

Update: `git pull && cargo build --release`

Team consistency: All developers use the same dev container image with the same Rust toolchain version (`rust-version = "1.89"`). Building from the same Git commit produces identical binaries.

### 6.2 Phase 2: npm Distribution (M9)

When distributed via npm:
```bash
npm install -D @unimatrix/cli
```

**Team consistency mechanisms:**
- `package.json` lockfile (`package-lock.json` / `yarn.lock` / `pnpm-lock.yaml`) pins the exact version
- All team members install the same version via lockfile
- Dependabot or Renovate can automate version bump PRs
- CI validates against the lockfile version

**Update workflow:**
```bash
npm update @unimatrix/cli     # updates to latest compatible version
npm install @unimatrix/cli@0.4.0  # explicit version pin
```

### 6.3 Phase 2: GitHub Releases

For non-npm environments:
- Pin version in CI scripts: `UNIMATRIX_VERSION=0.3.0` in environment config
- Install script accepts version argument: `install.sh --version 0.3.0`
- cargo-binstall supports version pinning: `cargo binstall unimatrix@0.3.0`

**Team consistency:** Requires discipline. No lockfile mechanism inherent in `curl | sh`. Recommend documenting expected version in project's `.tool-versions` or equivalent.

### 6.4 Version Enforcement

For teams that need strict version consistency, the `SessionStart` hook can validate:

```bash
# In unimatrix-server hook SessionStart:
# Read expected version from .unimatrix-version file
# Compare against actual binary version
# Warn (stdout) if mismatch — does not block (exit 0)
```

This is a post-M9 refinement, not a Phase 1 concern.

---

## 7. Dev Container Strategy

### 7.1 Current Setup

The current `.devcontainer.json` is minimal:

```json
{
  "image": "mcr.microsoft.com/devcontainers/base:bookworm",
  "features": {
    "ghcr.io/devcontainers/features/rust:1": {},
    "ghcr.io/devcontainers/features/node:1": {},
    "ghcr.io/devcontainers/features/python:1": {}
  },
  "postCreateCommand": "curl -fsSL https://claude.ai/install.sh | bash && ..."
}
```

Unimatrix is built from source inside the container. The `postCreateCommand` installs Claude Code but not Unimatrix binaries.

### 7.2 Phase 1: Build from Source (Immediate)

For the Unimatrix project itself, building from source remains appropriate:
```json
{
  "postCreateCommand": "curl -fsSL https://claude.ai/install.sh | bash && cargo build --release && echo 'alias dsp=...' >> ~/.bashrc"
}
```

The dev container already has the Rust toolchain. Adding `cargo build --release` to `postCreateCommand` ensures the binary is available on container creation. Takes ~2-5 minutes on first build.

**Hook configuration:** Include `.claude/settings.json` with hook configuration in the repository. When the dev container starts, hooks are already configured because the settings file is committed.

### 7.3 Phase 2: Pre-Built Binary in Container (External Adoption)

For projects that adopt Unimatrix without being Unimatrix:

**Option A: Dev Container Feature**
```json
{
  "features": {
    "ghcr.io/unimatrix/features/unimatrix:1": {}
  }
}
```

A custom dev container feature that:
1. Downloads the prebuilt binary for the container's platform from GitHub Releases
2. Installs to `/usr/local/bin/unimatrix-server`
3. Downloads the ONNX model to `/home/vscode/.cache/unimatrix/models/`
4. Runs `unimatrix-server init` to configure hooks (if not already configured)

**Option B: Pre-built Image Layer**
```dockerfile
FROM mcr.microsoft.com/devcontainers/base:bookworm
RUN curl -sSL https://github.com/.../unimatrix-server-linux-x64 -o /usr/local/bin/unimatrix-server \
    && chmod +x /usr/local/bin/unimatrix-server
```

**Option C: postCreateCommand download**
```json
{
  "postCreateCommand": "curl -sSL https://unimatrix.dev/install.sh | sh"
}
```

### 7.4 GitHub Codespaces

Codespaces use dev containers, so all three options above apply. Key considerations:

- **Cold start**: If building from source, Codespace creation takes ~5 minutes for the Rust build. Pre-built binaries reduce this to seconds.
- **Prebuilds**: GitHub Codespaces supports prebuilds — a pre-built image with all dependencies cached. The Rust build can be part of the prebuild, making Codespace creation instant.
- **Persistent storage**: The ONNX model (~87 MB) should be cached in the prebuild or downloaded to persistent storage, not re-downloaded on every Codespace creation.
- **Architecture**: Codespaces run on Linux x86_64 or aarch64 (configurable). The cortical implant must detect and use the correct binary.

### 7.5 Recommendation

| Scenario | Approach | Timeline |
|----------|----------|----------|
| Unimatrix development | Build from source in dev container | Now |
| Early external adopters | postCreateCommand with install script | M9 |
| Production external adoption | Dev container feature | M9+ |
| Team Codespaces | Prebuild with cached binary + model | M9+ |

---

## 8. Packaging Plan

### 8.1 Phase 1: Bundled Subcommand (M5 — col-006)

**Implementation steps:**

1. **Add `hook` subcommand to `unimatrix-server`**
   - New module in `crates/unimatrix-server/src/hook/` (or `src/cli/hook.rs`)
   - `clap` subcommand: `unimatrix-server hook <EVENT_NAME>`
   - Reads JSON from stdin (same format Claude Code provides)
   - Dispatches to event-specific handlers
   - Writes JSON response to stdout (when needed)
   - Exits with appropriate code (0 = success, 2 = block)

2. **Update `.claude/settings.json`**
   - Replace current bash observation scripts with `unimatrix-server hook` commands
   - Add new hook events (UserPromptSubmit, PreCompact, SessionStart, SessionEnd, etc.)

3. **Test infrastructure**
   - Unit tests for hook dispatch logic
   - Integration tests that simulate Claude Code's JSON input format
   - Test each hook event's input parsing and output generation

4. **Documentation**
   - Hook event → handler mapping table
   - Configuration example for `.claude/settings.json`

**No distribution work required.** The binary is already built as part of the existing Rust workspace.

### 8.2 Phase 2: Release Automation (M9 — nan-004)

**Implementation steps:**

1. **GitHub Actions CI/CD pipeline**
   - Build matrix for 5 platform targets (see section 1.4)
   - `cargo-zigbuild` for Linux targets, native builds for macOS/Windows
   - Binary artifact upload to GitHub Releases
   - Release triggered by Git tag (`v0.x.0`)

2. **npm packaging**
   - Create npm package structure (base + 5 platform packages)
   - Publish script: build all platforms, package, publish in correct order (platform packages first, base package last — esbuild's pattern to avoid race conditions)
   - Postinstall fallback script for download from GitHub Releases
   - Verify with `npm pack --dry-run` before publishing

3. **Homebrew tap**
   - Create `homebrew-tap` repository
   - Formula downloads binary from GitHub Releases
   - Auto-update formula on new release (via GitHub Action)

4. **Install script**
   - Platform detection (uname -s, uname -m)
   - Binary download from GitHub Releases
   - SHA-256 checksum verification
   - PATH installation (to `~/.local/bin` or `/usr/local/bin`)

5. **Dev container feature**
   - `devcontainer-feature.json` metadata
   - Install script (reuse install.sh logic)
   - ONNX model download and caching
   - Publish to ghcr.io feature registry

---

## 9. Open Risks

### R1: ONNX Runtime Platform Coverage
**Risk:** ONNX Runtime does not provide prebuilt binaries for all target triples, or platform-specific builds behave differently.
**Mitigation:** The `ort` crate's `download` strategy fetches prebuilt ONNX Runtime from Microsoft. Verify all 5 target triples have prebuilt binaries. Fallback: `compile` strategy builds from source (adds ~10 minutes to CI).
**Severity:** Medium. Measured: current dev container (Linux aarch64) works with prebuilt `libonnxruntime.so.1.20.1` at 14 MB.

### R2: npm optionalDependencies Reliability
**Risk:** Some package manager configurations disable optional dependencies or postinstall scripts, preventing binary installation.
**Mitigation:** Dual fallback (optionalDependencies + postinstall download), clear error messages when binary is missing, alternative installation channels (GitHub Releases, cargo-binstall).
**Severity:** Low. This is a solved problem — esbuild, Turborepo, and Sentry all handle it successfully.

### R3: Binary Size Growth
**Risk:** As features are added (col-007 through col-011), the server binary grows. Currently 17 MB; could reach 25-30 MB.
**Mitigation:** Rust's dead code elimination is effective. The hook dispatch code is small (~50-100 KB). Monitor binary size in CI. If needed, `strip` symbols and enable LTO (link-time optimization) to reduce size.
**Severity:** Low. 17 MB is well within acceptable range for a native binary distributed via npm (esbuild's platform packages are ~9 MB each; Sentry CLI is ~15 MB).

### R4: Model Distribution
**Risk:** The ONNX model (87 MB) must be available at runtime. It is not bundled in the binary — it is downloaded from Hugging Face Hub on first use. In CI/container environments, this download may fail or be slow.
**Mitigation:** Cache model in dev container prebuild. Include model in npm package as data file (adds 87 MB to download). Or: provide a separate `unimatrix-server download-model` command run once during setup.
**Severity:** Medium. This is the largest single artifact and affects first-run experience. The bundled subcommand approach means the model is already downloaded by the MCP server — the implant reuses it.

### R5: Claude Code Hook Schema Stability
**Risk:** Claude Code's hook JSON input schema may change between versions, breaking the implant's parser.
**Mitigation:** Parse JSON loosely (ignore unknown fields, use `serde(default)`). Test against Claude Code's documented schema. Monitor Claude Code release notes for hook API changes.
**Severity:** Medium. The hook API is documented and appears stable, but Anthropic has not made explicit stability guarantees. The `hook_event_name` field and common fields (`session_id`, `transcript_path`, `cwd`) are unlikely to change.

### R6: Windows ONNX Runtime Linking
**Risk:** ONNX Runtime on Windows requires Visual C++ Redistributable. If not installed, the binary fails to start.
**Mitigation:** Static linking of ONNX Runtime on Windows (uses the 96 MB static archive). Or: document Visual C++ Redistributable as a prerequisite. Windows is P2 priority — can be deferred.
**Severity:** Low (P2 platform).

### R7: Lockfile Cross-Platform Inconsistencies
**Risk:** npm lockfiles with platform-specific optionalDependencies can behave differently across platforms. Known issue with npm (GitHub issue #8320) and Turborepo (#1335).
**Mitigation:** Use npm v9+ which handles platform-specific optionalDependencies correctly. Test lockfile generation across all target platforms in CI.
**Severity:** Low. Affects npm distribution only (Phase 2).

---

## References

- [Sentry Engineering: How to publish binaries on npm](https://sentry.engineering/blog/publishing-binaries-on-npm) — definitive guide on the optionalDependencies + postinstall pattern
- [esbuild: Platform-Specific Binaries](https://deepwiki.com/evanw/esbuild/6.2-platform-specific-binaries) — original pioneer of npm native binary distribution
- [Turborepo CLI Architecture](https://deepwiki.com/vercel/turborepo/2.4-cli-architecture) — Rust binary distributed via npm thin wrapper
- [Orhun's Blog: Packaging Rust for npm](https://blog.orhun.dev/packaging-rust-for-npm/) — practical guide for Rust binary npm packaging
- [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) — Zig-based cross compilation for Rust
- [cross-rs](https://github.com/cross-rs/cross) — Docker-based cross compilation
- [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) — Binary installation from cargo
- [Claude Code Hooks Reference](https://code.claude.com/docs/en/hooks) — official hook configuration schema and events
- [houseabsolute/actions-rust-cross](https://github.com/houseabsolute/actions-rust-cross) — GitHub Actions for Rust cross-compilation
