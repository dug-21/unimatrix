# Agent Report: nan-014-agent-2-spec

## Task
Write SPECIFICATION.md for nan-014 (Container Packaging -- MIT Image).

## Status
COMPLETE

## Output
- `/workspaces/unimatrix/product/features/nan-014/specification/SPECIFICATION.md`

## Key Decisions

1. **Foreground mode as shared-function extraction**: Specified FR-4.5 requiring the shared daemon logic to be extracted into a common function callable by both `--foreground` and `--daemon --daemon-child`. This directly addresses SR-06 (highest-blast-radius risk) by preventing code duplication or conditional skipping in the existing daemon path.

2. **Health check as sync subcommand**: Specified FR-5.2 following the established sync CLI subcommand pattern (Unimatrix procedure #1192). UDS socket connection check rather than HTTP endpoint, since W2-2 has not delivered the HTTPS listener yet.

3. **Container CI independence from binary/npm pipeline**: Specified FR-6.7/FR-6.8 making container build jobs parallel with (not dependent on) binary builds. Container failures do not block releases (SR-10 mitigation).

4. **No EXPOSE directive**: Specified constraint C-10 omitting port exposure until W2-2 (SR-07 mitigation).

5. **Four-stage Dockerfile**: Added a separate model-download stage (Stage 3) between builder and runtime to cache models independently of source changes, as resolved in OQ-02.

## Open Questions for Architect

1. **Model bake-in path resolution**: Should baked-in models use `XDG_CACHE_HOME` env var override or a dedicated `/usr/local/share/unimatrix/models/` path with config.toml `[embed] cache_dir`? Impacts FR-1.5 and FR-5.5.

2. **Health check protocol depth**: Full MCP ping over UDS vs. socket-exists-and-accepts-connection. Impacts FR-5.3 implementation complexity.

3. **Docker stop_grace_period**: Whether compose file should set 30s to accommodate daemon shutdown (vector persistence + DB compaction).

## Risk Mitigations Incorporated

All 11 SR-XX risks from SCOPE-RISK-ASSESSMENT.md are addressed:
- SR-01: SHA-256 gate specified (NFR-4); mirror/cache noted as out-of-scope follow-up
- SR-02: cargo-chef version pin specified (FR-1.7)
- SR-03: Separate model-download stage specified (FR-1.4)
- SR-04: Digest pinning noted as out-of-scope hardening follow-up
- SR-05: Volume layout documented in Domain Models section
- SR-06: Shared-function extraction specified (FR-4.5, C-9)
- SR-07: No EXPOSE directive (FR-1.13, C-10)
- SR-08: Graceful permission error messages (NFR-8)
- SR-09: PidGuard container restart behavior (C-7)
- SR-10: Independent CI pipelines (FR-6.7, NFR-9)
- SR-11: Shared socket path resolution (FR-5.5)

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 20 entries; used #4554, #4274, #1192, #1199 for context
