# ASS-002: MCP Protocol Deep Dive (Track 2A)

**Phase**: Assimilate (Research Spike)
**Source**: Pre-Roadmap Spike, Track 2A
**Date**: 2026-02-20
**Status**: In Progress

---

## Objective

Understand exactly how Claude Code discovers, invokes, and uses MCP tool responses. Produce an MCP integration guide that informs the interface specification (Track 3).

## Research Questions

1. What's the exact JSON-RPC lifecycle? (initialize → tools/list → tools/call → ...)
2. How does Claude Code render tool responses in its context?
3. What's the token cost of tool descriptions in the system prompt?
4. Does Claude Code support MCP resources (passive context) or only tools (active invocation)?
5. Can tool descriptions include usage hints that influence when Claude invokes them?
6. How does Claude Code handle tool errors? Retry? Surface to user?
7. Is there a Rust MCP SDK we should use, or do we implement the protocol directly?

## Method

- Read MCP specification
- Read Claude Code MCP documentation
- Investigate Rust MCP SDK landscape
- Build a minimal hello-world MCP server in Rust that Claude Code can connect to
- Observe behavior

## Deliverable

MCP integration guide documenting:
- Protocol flow (JSON-RPC lifecycle)
- Response format best practices
- Tool description patterns
- Rust SDK evaluation
- Resource vs. tool support in Claude Code
- Error handling behavior
