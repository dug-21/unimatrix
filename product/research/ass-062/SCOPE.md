# ASS-062: Release and Versioning Strategy for Dual-Artifact Delivery

**Date**: 2026-05-27
**Status**: Scoped
**Depends on**: ASS-061 (feature gating decisions affect what the container bundles)
**Feeds**: nan-014 (container CI), release pipeline, upgrade documentation

---

## Question

How should Unimatrix version, tag, and release two primary artifacts (npm package + container image) that share a codebase but may diverge in bundled capabilities and update cadence — while maintaining a clear compatibility contract for users?

## Why This Matters

ADR-004 CI (#4572) already established that container build jobs are independent of binary/npm jobs in release.yml. But independence at the CI level doesn't answer the versioning questions: do both artifacts share a semver? What happens when the container needs a patch (e.g., base image CVE) that doesn't touch application code? If a UI ships in the container with its own iteration cadence, how does that compose? Without clear answers, users won't know which versions are compatible, and maintainers won't know what a version bump means.

## Bounded Questions

### Q1: Versioning Model

Options:

- **Unified semver** — one version (e.g., `v1.2.3`) for both npm package and container image. Simple, but container-only patches (base image update, model bundle change) force a version bump on the npm package too.
- **Unified semver + container build metadata** — `v1.2.3` for both, container image additionally tagged `v1.2.3+build.4` for container-only rebuilds. Semver spec allows this but tooling support varies.
- **Independent semver** — npm package `v1.2.3`, container image `v2.0.1`. Maximum flexibility but compatibility matrix required.
- **Unified semver + container suffix** — container gets `v1.2.3-container.1` for container-only patches. npm always gets the bare version.

Evaluate each against: user confusion risk, automation complexity, compatibility tracking overhead, and the reality of how often container-only patches will occur.

### Q2: Tag Conventions and Pipeline Triggers

Current: `v*` tag push triggers release.yml. ADR-004 CI splits into independent binary/npm and container branches.

- Does a single `v1.2.3` tag trigger both branches? If so, how do container-only rebuilds work?
- Alternative: `v1.2.3` for binary/npm, `container-v1.2.3` or `v1.2.3-container.1` for container-only?
- How do pre-release tags work? `v1.2.3-rc.1` — does the container get a pre-release image?
- Tag-to-image mapping: `:latest`, `:1`, `:1.2`, `:1.2.3`, `:1.2.3-slim` (without GGUF models)?

### Q3: Container Image Variants

If ASS-061 determines that ML models are baked into the image:

- `:1.2.3` — full image (ONNX cross-encoder included)
- `:1.2.3-slim` — no ML models (user mounts via volume)
- `:1.2.3-gguf` — includes GGUF model (1-8GB, if W2-5 ships)

If models are volume-mounted, variants reduce to one image. The versioning strategy must account for whichever model ASS-061 recommends.

### Q4: Schema Migration and Container Upgrades

When a new version includes a schema migration:

- Local (npm): migration runs automatically on next daemon start. User's project directory.
- Container: migration runs on container restart with the new image. But the volume persists.
- What's the rollback story? Can a user downgrade the container image safely? (Probably not if schema migrated forward — document this.)
- Should the container refuse to start if the volume's schema version is newer than the binary? (Forward compatibility guard.)

### Q5: Compatibility Matrix

With HTTPS transport (W2-2), multiple client types connect to one server:

- Claude Code (npm-installed, UDS or HTTPS)
- Codex CLI (HTTPS)
- Gemini CLI (HTTPS)

When the server (container) upgrades, do clients need to upgrade? The MCP protocol provides some stability, but:

- What's the server→client compatibility contract? (e.g., server v1.3 works with clients expecting v1.2 tools?)
- Should the server advertise its version in a health endpoint?
- Is there a minimum client version enforcement mechanism, or just documentation?

### Q6: Changelog and Release Notes

With two artifacts from one repo:

- One changelog or two?
- How do container-only changes (base image, model updates, compose config) appear in release notes?
- Does `uni-release` skill need updates to handle dual-artifact releases?

## Expected Output

- Recommended versioning model with rationale
- Tag convention specification (what triggers what)
- Container image tagging scheme
- Schema migration safety rules for container upgrades
- Compatibility contract (server↔client version guarantees)
- Updates needed to `uni-release` skill and release.yml

## Known Constraints and Prior Art

- ADR-004 CI (#4572): Container CI jobs independent of binary/npm release jobs
- Current release process: `uni-release` skill, `v*` tag triggers release.yml
- Release procedure (#4580): Step-by-step for binary/npm releases — container not yet covered
- MCP protocol: Provides tool-level stability but no version negotiation
- Schema migrations: Run automatically on startup, forward-only, no rollback mechanism currently
- ASS-061 dependency: Model distribution decision affects image variant strategy
