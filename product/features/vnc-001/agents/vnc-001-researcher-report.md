# Agent Report: vnc-001-researcher

## Key Findings

1. **rmcp 0.16 is the correct SDK choice.** Official, well-maintained (1.14M downloads/month), with proc macro support for tool definition. Pin to `=0.16.0` due to pre-1.0 breaking change risk.

2. **Foundation crates are ready.** All four crates (store, vector, embed, core) are complete and tested. The async wrappers in unimatrix-core bridge synchronous foundations to the tokio runtime rmcp requires.

3. **Security infrastructure must precede tool implementations.** AGENT_REGISTRY and AUDIT_LOG must exist before vnc-002 writes the first entry. This is the security research's core recommendation and aligns with the user's security-first priority.

4. **Store::compact() vs Arc<Store> is a real design tension.** compact() requires `&mut self` but the server shares Store via Arc. This needs an architectural decision in Phase 2.

5. **Project isolation is well-understood.** SHA-256 hash of canonical project path, data at `~/.unimatrix/{hash}/`. Auto-detection via `.git/` walk-up is standard and deterministic.

6. **Model download latency is a UX concern.** First-run embedding model download can take 30+ seconds. The server initialization strategy needs to account for this.

## Scope Boundaries

- **IN**: Server binary, stdio transport, MCP lifecycle, project isolation, agent registry, audit log, agent identity flow, graceful shutdown, tool stubs, error mapping
- **OUT**: Tool implementations (vnc-002), input validation (vnc-002), content scanning (vnc-002), capability enforcement (vnc-002), HTTP transport, CLI, multi-project, config file

## Open Questions

1. Where to create AGENT_REGISTRY and AUDIT_LOG tables (store crate vs server)
2. compact() shutdown pattern with Arc<Store>
3. Model download timing relative to MCP init
4. AUDIT_LOG key type (redb lacks u128 support)

## Risks

- rmcp 0.16 breaking changes (mitigated by exact version pin)
- anndists patch maintenance burden
- Model download failure on first run in air-gapped environments
