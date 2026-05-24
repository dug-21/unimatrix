# nan-014 Pseudocode Agent Report

## Agent ID
nan-014-agent-1-pseudocode

## Status
COMPLETE

## Files Produced

| File | Component | Lines |
|------|-----------|-------|
| `pseudocode/OVERVIEW.md` | Cross-cutting | 96 |
| `pseudocode/pidguard-self-pid.md` | PidGuard self-PID guard | 82 |
| `pseudocode/config-env-override.md` | UNIMATRIX_CONFIG env var | 118 |
| `pseudocode/serve-foreground.md` | --foreground flag | 104 |
| `pseudocode/health-subcommand.md` | health CLI subcommand | 152 |
| `pseudocode/dockerignore.md` | .dockerignore | 70 |
| `pseudocode/dockerfile.md` | Three-stage Dockerfile | 188 |
| `pseudocode/docker-compose.md` | docker-compose.yml | 78 |
| `pseudocode/ci-container-jobs.md` | CI container jobs | 134 |

## Components Covered
All 8 per Implementation Brief Component Map:
1. pidguard-self-pid (wave 1)
2. config-env-override (wave 1)
3. serve-foreground (wave 1)
4. health-subcommand (wave 1)
5. dockerignore (wave 2)
6. dockerfile (wave 2)
7. docker-compose (wave 2)
8. ci-container-jobs (wave 2)

## Open Questions for Implementation Agents

1. **ORT SHA-256 hashes**: Must be captured at implementation time by downloading both tarballs.
2. **cargo-chef version**: Verify 0.1.71 is latest stable. Update if newer version exists.
3. **Model download paths**: Verify `model-download` with `HOME=/data` writes to `/data/.cache/unimatrix/models/`.
4. **Planner COPY strategy**: Per-crate Cargo.toml COPY assumes 9 workspace crates. Verify list at implementation time.
5. **health::run return type**: Pseudocode recommends `fn run(...) -> i32` (matching `run_stop` pattern) over `Result + process::exit` for testability. Implementation agent should decide.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 15 entries returned; relevant: #4574 (ADR-006 cargo-chef), #4570 (ADR-002 ORT SHA-256), #4572 (ADR-004 CI independence), #4575 (ADR-007 self-PID guard), #4573 (ADR-005 data path), #4569 (ADR-001 foreground mode)
- Queried: mcp__unimatrix__context_search -- #1192 (sync CLI subcommand procedure) applied to health-subcommand design
- Queried: mcp__unimatrix__context_get #1192 -- full procedure text retrieved and followed
- Queried: mcp__unimatrix__context_get #4554 -- W2-1 feature context confirmed
- Deviations from established patterns: none. Health subcommand follows procedure #1192 exactly.
