# ASS-008 Wave 3: Hook-Based Cryptographic Intent Signing — Synthesis

**Date**: 2026-02-24
**Type**: Research Spike — Wave 3 Synthesis
**Predecessors**: `ANALYSIS.md` (Wave 1), `ANALYSIS-2-orchestrator-architecture.md` (Wave 2)
**Companion files**:
- `RESEARCH-hook-based-auth.md` — Full architecture, attack resistance, implementation patterns
- Claude Code hooks deep dive (agent transcript)

---

## Executive Summary

**The idea works.** Claude Code's PreToolUse hooks are a viable trusted intermediary for cryptographic intent signing. The mechanism provides a 2FA-like guarantee where:

- **Factor 1**: The tool call (what the LLM wants to do)
- **Factor 2**: A cryptographic signature proving the call passed through a deterministic, LLM-independent process

The LLM cannot forge, bypass, or influence the signature. The signing key never enters the LLM's context window. The hook overwrites any LLM-provided auth fields via `updatedInput`, making forgery attempts irrelevant.

**Performance impact**: ~7-15ms per tool call (0.1-0.7% of round-trip). Negligible.

**Key limitation**: Claude Code-specific. Non-Claude-Code clients (Cursor, Windsurf) need an MCP proxy/gateway as a fallback — architecturally equivalent but different deployment model.

---

## 1. How It Works

```
LLM generates:     { tool: "context_store", content: "...", topic: "nxs-005" }
                         |
                         v
Hook intercepts:    PreToolUse fires (deterministic shell process)
                    - Reads tool_name + tool_input from stdin
                    - Computes: canonical = sort(tool_name + tool_input + timestamp + nonce)
                    - Signs: signature = HMAC-SHA256(canonical, secret_key)
                    - Returns updatedInput: original params + { _auth_signature, _auth_timestamp, _auth_nonce }
                         |
                         v
Unimatrix receives: { content: "...", topic: "nxs-005",
                       _auth_signature: "a8f3c...", _auth_timestamp: 1708732800, _auth_nonce: "x7k2..." }
                    - Strips _auth_* fields → clean params
                    - Reconstructs canonical from clean params
                    - Verifies signature against stored key
                    - Checks timestamp freshness (30s window)
                    - Checks nonce uniqueness (replay prevention)
                    - Proceeds if valid, rejects if not
```

### Why It's Secure

| Property | Guarantee |
|----------|-----------|
| **LLM can't forge signature** | Signing key is on disk, not in LLM context. Hook is a separate process. |
| **LLM can't bypass hook** | Hooks are enforced by Claude Code runtime. LLM has no mechanism to skip them. Snapshot captured at session startup. |
| **LLM can't tamper after signing** | `updatedInput` IS the final input. No intermediate step between hook output and MCP request. |
| **No TOCTOU gap** | The signed data is the sent data. Hook signs its own output, not a separate copy. |
| **Replay prevention** | Timestamp window (30s) + nonce uniqueness check. For stdio, client and server share the same clock. |
| **Forgery via parameters** | If LLM includes `_auth_signature` in its params, the hook's `updatedInput` overwrites it entirely. |

---

## 2. What It Does NOT Protect Against

This is critical to understand. Hook-based signing proves **provenance** (the call went through a trusted intermediary), not **benevolence** (the call is safe).

| Threat | Protected? | Why |
|--------|:---------:|-----|
| LLM claims false agent_id | No | The hook faithfully signs whatever params the LLM generated, including a spoofed agent_id. **But**: combined with session tokens (Wave 1), agent_id becomes advisory — identity comes from the token, not the parameter. |
| Prompt injection → malicious content | No | If injection causes the LLM to store harmful content, the hook signs it. Content scanning (vnc-002) handles this. |
| Prompt injection → wrong parameters | No | If injection manipulates parameter values, those values get signed. Server-side validation handles this. |
| Denial of service (rapid calls) | No | Each call is properly signed. Rate limiting handles this. |
| Semantic poisoning | No | Signed entries can still be semantically misleading. Cortical-phase detection handles this. |

**Bottom line**: Hook-based signing is Layer 3 in the defense stack. It works WITH Layers 1-2 (transport security, identity resolution) and Layers 4-5 (capability enforcement, content scanning), not instead of them.

---

## 3. Attack Resistance Summary

10 attacks analyzed:

| Attack | Result |
|--------|--------|
| A1: LLM forges signature | **Defeated** — hook overwrites via updatedInput |
| A2: LLM bypasses hook | **Defeated** — hooks enforced by runtime, not LLM |
| A3: LLM reads signing key via Bash | **Mitigated** — file permissions + hook blocking reads of key path |
| A4: Replay attack | **Defeated** — timestamp + nonce |
| A5: Parameter tampering after signing | **Defeated** — updatedInput IS the sent data |
| A6: TOCTOU race condition | **Defeated** — no intermediary between signing and sending |
| A7: Hook script compromise | **Mitigated** — loaded at startup, ConfigChange requires review, git integrity |
| A8: Prompt injection → bad parameters | **Not addressed** (different layer's job) |
| A9: Key exfiltration via side channel | **Mitigated** — separate process, separate memory space |
| A10: Clock skew | **Non-issue** for stdio (shared clock) |

---

## 4. The Hook Configuration

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "mcp__unimatrix__.*",
        "hooks": [
          {
            "type": "command",
            "command": "\"$CLAUDE_PROJECT_DIR\"/.claude/hooks/unimatrix-sign.sh",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

The matcher `mcp__unimatrix__.*` ensures only Unimatrix tool calls are signed. Other MCP servers and built-in tools are unaffected.

The signing script lives in the project repo (`.claude/hooks/unimatrix-sign.sh`), committed and protected by git integrity. The signing key lives outside the repo (`~/.unimatrix/hook-key`), never committed.

---

## 5. Implementation Phases

### Phase 1: HMAC-SHA256 (Simplest, Immediate)

- Shared secret between hook and Unimatrix
- Key generated at `unimatrix init`, stored in `~/.unimatrix/hook-key`
- Hook is a ~40-line bash script using `openssl` and `jq`
- Server-side: ~100 lines of Rust validation code
- **Limitation**: Symmetric — anyone with the key can sign

### Phase 2: Ed25519 (Asymmetric, Non-Repudiation)

- Hook holds private key, Unimatrix holds public key
- Supports multiple signers (different hook instances, different machines)
- Non-repudiation: can prove which hook signed a specific request
- Hook could be a compiled Rust binary for performance (~1-2ms vs ~7-15ms for bash)

### Phase 3: Biscuit Tokens (Capability Scoping)

- Hook mints a scoped Biscuit token per-request
- Token encodes: allowed tools, allowed topics/categories, TTL
- Combines signing + capability scoping in one mechanism
- Requires Biscuit integration in both hook binary and Unimatrix server
- **This is where Wave 1 (warrants) and Wave 3 (hooks) converge**

---

## 6. Novel Patterns Enabled by Hooks

The research identified capabilities beyond basic signing:

| Pattern | How It Works | Value |
|---------|-------------|-------|
| **Capability budget** | Hook tracks write count per session, refuses to sign after N writes | Limits blast radius of compromised agent |
| **Time-based restrictions** | Hook refuses to sign write operations outside configured hours | Prevents autonomous overnight knowledge modifications |
| **Content pre-screening** | Hook runs lightweight content scan BEFORE signing | Catches injection patterns before they reach Unimatrix |
| **Dead man's switch** | If hook process crashes, ALL tool calls are blocked | Fail-closed instead of fail-open |
| **Escalation detection** | Hook maintains state, detects unusual patterns (e.g., 10 writes in 5 seconds) | Behavioral circuit breaker |

These are free — they come with the hook infrastructure and don't require Unimatrix changes.

---

## 7. Client Compatibility

| Client | Hooks? | Signing Viable? | Fallback |
|--------|:------:|:--------------:|----------|
| Claude Code CLI | Yes | Yes | N/A |
| Claude Code SDK | Yes | Yes | N/A |
| Cursor | No | No | MCP proxy/gateway |
| Windsurf | No | No | MCP proxy/gateway |
| Custom clients | Varies | Depends | MCP proxy/gateway |

For non-Claude-Code clients, the same signing logic moves to an MCP proxy/gateway process — architecturally equivalent, different deployment.

**Unimatrix's primary target is Claude Code**, so hook-based signing covers the main use case. Gateway support is a straightforward extension for broader compatibility.

---

## 8. How This Fits the Full Defense Stack

Combining all three waves of research:

```
Layer 6: Behavioral Monitoring (future, Cortical phase)
  └─ Escalation detection, anomaly detection, contradiction detection

Layer 5: Content Scanning (existing, vnc-002)
  └─ ~35 regex patterns, category allowlist, content policies

Layer 4: Capability Enforcement (existing, vnc-002)
  └─ Agent trust levels, per-tool permissions, topic restrictions

Layer 3: Intent Verification (THIS WAVE — hook-based signing)    ← NEW
  └─ Cryptographic proof that call passed through trusted intermediary
  └─ Per-call parameter integrity, replay prevention, nonce check

Layer 2: Agent Identity (Wave 1 — session tokens + warrants)
  └─ Opaque tokens, Biscuit/Tenuo warrants, process identity

Layer 1: Transport Security (existing)
  └─ stdio process isolation, future: TLS for HTTP
```

Each layer addresses different threats. An attacker would need to:
- Bypass transport isolation (Layer 1) AND
- Forge or steal a session token (Layer 2) AND
- Forge a cryptographic signature (Layer 3) AND
- Pass capability checks (Layer 4) AND
- Evade content scanning (Layer 5)

...to successfully poison the knowledge store. Five independent barriers.

---

## 9. Relationship to Wave 2 (Unimatrix as Orchestrator)

Hook-based signing works in BOTH architectural models:

**Current (passive MCP server)**: External agents call Unimatrix. Hooks sign the calls. Unimatrix verifies.

**Future (active orchestrator)**: Unimatrix spawns agents. Unimatrix provides the hook script as part of the agent configuration. The signing key is generated by Unimatrix and injected into the agent's environment. This is even more secure because Unimatrix controls both sides of the trust boundary.

In the orchestrator model, the hook becomes part of Unimatrix's own infrastructure — it's not a user-installed script but a system-managed component. Key provisioning, rotation, and revocation are all under Unimatrix's control.

---

## 10. Key Insight: Signed Intent ≠ Safe Intent

Your original question was about "digitally signed intent." The research validates the mechanism but reveals an important nuance:

**Signing proves WHO, not WHAT.**

A signed intent proves:
- The call originated from a known, registered hook instance
- The parameters weren't tampered with in transit
- The call wasn't replayed from a previous session

A signed intent does NOT prove:
- The LLM's parameters are benign
- The content isn't poisoned
- The agent isn't being manipulated via prompt injection

This is why signing is Layer 3, not the whole stack. It eliminates identity spoofing and parameter tampering. Content safety requires Layers 4-5. Semantic safety requires Layer 6.

But here's the valuable insight: **a signed request that fails content scanning is MORE useful than an unsigned one** — because you know exactly which hook instance signed it, which means you know which agent (and therefore which LLM session) produced the malicious content. The signature creates accountability even when the content is bad.

---

## Sources

- `RESEARCH-hook-based-auth.md` — 38 references
- [Claude Code Hooks Reference](https://code.claude.com/docs/en/hooks)
- [Claude Code Hooks Guide](https://code.claude.com/docs/en/hooks-guide)
- RFC 8785 (JSON Canonicalization Scheme)
- RFC 9421 (HTTP Message Signatures) — model for the signing pattern
