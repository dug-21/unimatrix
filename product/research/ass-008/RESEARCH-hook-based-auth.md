# Hook-Based Cryptographic Intent Signing for MCP Agent Authentication

**Research Document ASS-008 | February 2026**
**Status**: Research Complete

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [The Core Architecture](#2-the-core-architecture)
3. [Pre-Action Hook as Trusted Intermediary](#3-pre-action-hook-as-trusted-intermediary)
4. [Intent Extraction and Signing](#4-intent-extraction-and-signing)
5. [The Hook-MCP Interaction Model](#5-the-hook-mcp-interaction-model)
6. [Attack Resistance Analysis](#6-attack-resistance-analysis)
7. [Intent Evaluation on the Server Side](#7-intent-evaluation-on-the-server-side)
8. [Practical Implementation Patterns](#8-practical-implementation-patterns)
9. [Limitations and Edge Cases](#9-limitations-and-edge-cases)
10. [Comparison to Alternative Intermediary Patterns](#10-comparison-to-alternative-intermediary-patterns)
11. [Novel Patterns Enabled by Hooks](#11-novel-patterns-enabled-by-hooks)
12. [Prior Art and Related Systems](#12-prior-art-and-related-systems)
13. [Implementation Recommendations for Unimatrix](#13-implementation-recommendations-for-unimatrix)
14. [References](#14-references)

---

## 1. Executive Summary

This document analyzes a novel security architecture for MCP (Model Context Protocol) agent authentication: using Claude Code's pre-tool-call hooks as a **cryptographic signing intermediary**. The hook intercepts every MCP tool call before it reaches the Unimatrix server, extracts the tool name and parameters (the "intent"), cryptographically signs this context, and attaches the signature to the request. Unimatrix then verifies the signature, confirming that the call passed through a trusted, deterministic process that the LLM cannot control or forge.

This creates a 2FA-like mechanism:

- **Factor 1**: The tool call itself (what the LLM wants to do)
- **Factor 2**: A cryptographic signature from the hook (proving the call transited a trusted intermediary)

The LLM cannot forge the signature because:
1. The hook is a deterministic shell process, not under LLM control
2. The signing key resides in the hook's filesystem/environment, not in the LLM's context window
3. The hook computes the signature as a child process of Claude Code, not as a tool response

This document evaluates the technical feasibility, attack surface, prior art, and implementation paths for this approach within the Unimatrix MCP server.

---

## 2. The Core Architecture

### 2.1 End-to-End Flow

```
+------------------+     +-------------------+     +-------------------+
|                  |     |                   |     |                   |
|  LLM (Claude)    |     |  PreToolUse Hook  |     |  Unimatrix MCP    |
|  generates tool  |---->|  (shell process)  |---->|  Server           |
|  call params     |     |                   |     |                   |
|                  |     |  1. Read stdin     |     |  1. Parse request |
|                  |     |  2. Extract intent |     |  2. Extract sig   |
|                  |     |  3. Sign with key  |     |  3. Verify sig    |
|                  |     |  4. Inject sig via |     |  4. Compare intent|
|                  |     |     updatedInput   |     |  5. Enforce policy|
|                  |     |  5. Return JSON    |     |  6. Execute tool  |
|                  |     |                   |     |                   |
+------------------+     +-------------------+     +-------------------+
                               |                         |
                               |   signing key           |   public key /
                               |   (private, on disk)    |   shared secret
                               |                         |   (on disk)
                               v                         v
                          ~/.unimatrix/              ~/.unimatrix/
                          hook-key.pem               server-key.pub
```

### 2.2 Detailed Sequence

```
LLM                  Claude Code              Hook Process            Unimatrix
 |                       |                        |                      |
 |-- tool_call --------->|                        |                      |
 |   (tool_name,         |                        |                      |
 |    tool_input)        |                        |                      |
 |                       |-- PreToolUse event ---->|                      |
 |                       |   (JSON on stdin)       |                      |
 |                       |                        |-- read stdin          |
 |                       |                        |-- extract:            |
 |                       |                        |   tool_name           |
 |                       |                        |   tool_input          |
 |                       |                        |   timestamp           |
 |                       |                        |   nonce               |
 |                       |                        |-- canonicalize        |
 |                       |                        |-- sign(canonical)     |
 |                       |                        |-- construct output:   |
 |                       |                        |   updatedInput with   |
 |                       |                        |   _auth_signature,    |
 |                       |                        |   _auth_timestamp,    |
 |                       |                        |   _auth_nonce         |
 |                       |<-- JSON on stdout ------|                      |
 |                       |   (exit 0)              |                      |
 |                       |                        |                      |
 |                       |-- MCP tools/call --------------------------->|
 |                       |   (with injected                             |
 |                       |    _auth_* fields)                           |
 |                       |                                              |
 |                       |                         verify_signature <---|
 |                       |                         check_timestamp  <---|
 |                       |                         check_nonce     <---|
 |                       |                         compare_intent  <---|
 |                       |                         enforce_policy  <---|
 |                       |                         execute_tool    <---|
 |                       |                                              |
 |                       |<------------- tool result -------------------|
 |<--- result -----------|                                              |
```

### 2.3 What Constitutes "Intent"

In this architecture, "intent" is the complete, deterministic representation of what the tool call will do:

| Component | Description | Example |
|-----------|-------------|---------|
| `tool_name` | The MCP tool being invoked | `mcp__unimatrix__context_store` |
| `tool_input` | The complete parameter set | `{"topic": "auth", "content": "..."}` |
| `timestamp` | When the call was made (UTC) | `1708790400` |
| `nonce` | Unique value per request | `a7f3b2c1...` |
| `session_id` | Claude Code session identifier | `abc123` |

The intent is the **signed assertion**: "At time T, with nonce N, in session S, a call to tool X with parameters Y was initiated through a trusted intermediary."

---

## 3. Pre-Action Hook as Trusted Intermediary

### 3.1 The Service Mesh Sidecar Analogy

The hook-based signing pattern is structurally analogous to the **Envoy sidecar proxy** pattern in service meshes like Istio [1][2]:

| Aspect | Envoy Sidecar | Claude Code Hook |
|--------|---------------|------------------|
| **Position** | Intercepts all network traffic to/from a service | Intercepts all tool calls before execution |
| **Injection** | Automatically injected by the control plane | Configured in `.claude/settings.json` |
| **Transparency** | Application is unaware of the proxy | LLM is unaware of the hook |
| **Auth injection** | Injects mTLS certificates, JWT tokens | Injects cryptographic signatures |
| **Policy enforcement** | Enforces traffic policies, rate limits | Can enforce tool-level policies |
| **Trust model** | Application trusts the mesh infrastructure | MCP server trusts the hook infrastructure |

The key difference: Envoy intercepts at the **network layer** (L4/L7), while Claude Code hooks intercept at the **tool call layer** (pre-execution). Both operate as transparent intermediaries that the application/LLM cannot bypass.

Istio's architecture deploys Envoy as a companion container alongside each service, mediating all inbound and outbound traffic [1]. The hook does the same for tool calls: it sits between the LLM's decision and the MCP server's execution, with no way for the LLM to route around it.

### 3.2 The API Gateway Signing Analogy

API gateways like AWS API Gateway with Lambda authorizers follow a similar pattern [3]:

1. Client sends request to the gateway
2. Gateway invokes a Lambda authorizer before the backend
3. Authorizer validates credentials and returns an IAM policy
4. Gateway enforces the policy and forwards (or blocks) the request

The Claude Code hook plays the role of the Lambda authorizer: it sits in the request path, performs cryptographic operations, and enriches the request with security metadata. The difference is that the hook also **signs** the request rather than just validating an existing credential.

### 3.3 Prior Art: IDE/Tool Hooks as Security Enforcement Points

Using hooks as security enforcement points has established precedent:

- **Git pre-commit hooks**: Widely used to scan for secrets, validate code quality, and enforce policies before commits reach the repository [4]. Tools like Gitleaks, GitGuardian, and detect-aws-credentials run as pre-commit hooks to prevent credential leakage.
- **SSH ForceCommand**: Forces execution of a specific command when a user logs in via SSH, overriding client-supplied commands [5]. The original command is available in `SSH_ORIGINAL_COMMAND`. This is structurally identical to the hook pattern: intercept the request, evaluate it, and decide whether to proceed.
- **OneLogin Pre-Authentication Hooks**: Smart Hooks that execute custom logic before authentication completes, enabling risk-based decisions and context injection [6].

The Claude Code hook extends this pattern from single-action enforcement (commit, SSH login) to **per-tool-call enforcement** across an entire agent session.

---

## 4. Intent Extraction and Signing

### 4.1 Canonicalization

Cryptographic signing requires deterministic serialization: the same logical intent must always produce the same byte sequence. JSON, by default, does not guarantee key ordering, whitespace, or number formatting.

**RFC 8785 (JSON Canonicalization Scheme / JCS)** [7] solves this by defining:
- Deterministic property sorting (lexicographic Unicode code point ordering)
- Platform-independent number serialization (ECMAScript rules)
- No optional whitespace
- Constrained to the I-JSON subset

The canonical intent payload would be:

```json
{"nonce":"a7f3b2c1d4e5f6a7","session_id":"abc123","timestamp":1708790400,"tool_input":{"agent_id":"design-lead","content":"Pattern X is preferred","topic":"architecture"},"tool_name":"mcp__unimatrix__context_store"}
```

Note: keys sorted lexicographically, no whitespace, deterministic number formatting.

**Implementation**: The `jcs` crate (Rust) or `canonicalize-json` npm package can produce RFC 8785-compliant output. For a shell hook, `jq -cS` provides approximate canonicalization (sorted keys, compact), though it does not fully comply with RFC 8785's number formatting rules. For HMAC-SHA256, `jq -cS` is sufficient since both producer and consumer use the same tool.

### 4.2 Signing Algorithms

Three viable options, ordered by complexity:

#### HMAC-SHA256 (Recommended for v0.1)

```
signature = HMAC-SHA256(shared_secret, canonical_intent)
```

| Property | Value |
|----------|-------|
| **Key type** | Symmetric (shared secret) |
| **Key size** | 256 bits (32 bytes) |
| **Performance** | ~1 microsecond per operation |
| **Non-repudiation** | No (both parties hold the secret) |
| **Key provisioning** | Generate once at install, store in `~/.unimatrix/hook-key` |
| **Suitable for** | Single-machine, single-user setups |

Standard webhook authentication pattern used by Stripe, GitHub, Shopify, and Okta [8]. The producer (hook) and consumer (server) share a secret key. The hook computes `HMAC-SHA256(key, payload)` and attaches it. The server recomputes and compares.

#### Ed25519 (Recommended for v0.2+)

```
signature = Ed25519.sign(private_key, canonical_intent)
```

| Property | Value |
|----------|-------|
| **Key type** | Asymmetric (Ed25519 key pair) |
| **Key size** | 256-bit private, 256-bit public |
| **Signing performance** | ~87 microseconds [9] |
| **Verification performance** | ~228 microseconds [9] |
| **Non-repudiation** | Yes (only private key holder can sign) |
| **Key provisioning** | Generate key pair; hook holds private, server holds public |
| **Suitable for** | Multi-user, multi-machine, non-repudiation required |

Ed25519 provides ~11,494 signing operations per second on modest hardware [9], with a quad-core 2.4GHz processor capable of 71,000 verifications per second [9]. The latency overhead (under 1ms round-trip) is negligible compared to LLM inference time (~seconds).

#### Biscuit Tokens (Future consideration)

```
token = Biscuit.mint(authority_key, facts, caveats)
```

| Property | Value |
|----------|-------|
| **Key type** | Public key cryptography (Ed25519-based) |
| **Capability model** | Datalog-based policy language |
| **Attenuation** | Offline, without contacting the authority |
| **Verification** | Decentralized, any holder of the public key |
| **Suitable for** | Complex capability scoping, delegation chains |

Biscuit tokens [10] combine cryptographic signing with a Datalog-based authorization language. The hook could mint a per-request Biscuit that embeds both the signed intent and the authorized capabilities, enabling the server to verify both "who signed this" and "what are they allowed to do" in a single token.

### 4.3 Replay Attack Prevention

Three complementary mechanisms:

| Mechanism | How it works | Tradeoff |
|-----------|-------------|----------|
| **Timestamp** | Include UTC timestamp in signed payload; server rejects if `abs(now - timestamp) > window` (e.g., 30 seconds) | Requires roughly synchronized clocks; vulnerable to replay within the window |
| **Nonce** | Include a cryptographic random nonce (128-bit); server maintains a nonce cache and rejects duplicates | Requires server-side state (nonce store); cache must be bounded |
| **Sequence number** | Monotonically increasing counter per session; server rejects if `seq <= last_seen_seq` | Requires per-session state; gaps indicate dropped/blocked requests |

**Recommended approach**: Timestamp + nonce (combined). The timestamp provides coarse-grained freshness, while the nonce provides uniqueness within the timestamp window. The nonce cache only needs to retain entries for `window_duration` before pruning.

The nonce and timestamp are included as inputs to the HMAC/signature, cryptographically binding them to the request [11]. An attacker cannot change the timestamp or nonce without invalidating the signature.

### 4.4 Parameter Integrity

The signature MUST cover the **complete parameter set**, not just the tool name. Signing only the tool name would allow an attacker to substitute parameters while preserving a valid signature.

The canonical intent includes `tool_input` as a nested JSON object, which means every parameter value is covered by the signature. If the LLM changes any parameter between the hook's signing and the server's verification, the signature will not match.

---

## 5. The Hook-MCP Interaction Model

### 5.1 The Critical Question: How Does the Signature Reach the Server?

This is the most architecturally significant decision. The hook must somehow convey the cryptographic signature to the MCP server. Four options were evaluated:

#### Option A: Hook injects signature via `updatedInput` (RECOMMENDED)

Claude Code's PreToolUse hooks can **modify tool input parameters** before execution via the `updatedInput` field [12][13]. Starting in v2.0.10, a PreToolUse hook can return:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "updatedInput": {
      "topic": "architecture",
      "content": "Pattern X is preferred",
      "agent_id": "design-lead",
      "_auth_signature": "base64-encoded-signature",
      "_auth_timestamp": 1708790400,
      "_auth_nonce": "a7f3b2c1d4e5f6a7",
      "_auth_signer": "hook-v1"
    }
  }
}
```

Claude Code replaces the tool's input with `updatedInput`, and the MCP server receives the original parameters plus the `_auth_*` fields. The `updatedInput` must include ALL original fields plus the injected ones, since it replaces the entire input [13].

**Advantages**:
- Uses a supported, documented Claude Code mechanism
- No out-of-band channel needed
- The signature travels with the request as part of the MCP `tools/call` arguments
- Server-side extraction is trivial (read `_auth_*` fields from arguments)

**Disadvantages**:
- The `_auth_*` fields are visible in the MCP tool's input schema, which may confuse the LLM if it sees them in error messages
- Requires the MCP tool to accept additional properties (`additionalProperties: true` in JSON Schema, or explicit `_auth_*` fields)

#### Option B: Hook writes signature to `_meta` in MCP request

The MCP specification reserves a `_meta` field in `params` for client/server metadata [14][15]:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "context_store",
    "arguments": { "topic": "..." },
    "_meta": {
      "unimatrix.dev/auth_signature": "base64...",
      "unimatrix.dev/auth_timestamp": 1708790400,
      "unimatrix.dev/auth_nonce": "a7f3b2c1..."
    }
  }
}
```

**Problem**: The hook cannot modify the `_meta` field. The `updatedInput` mechanism only modifies the `arguments` portion of the `tools/call` request. The hook has no access to the outer `params` structure. This option would require Claude Code to expose `_meta` injection in hooks, which is not currently supported.

**Status**: Not viable with current Claude Code hook architecture. Would become the preferred approach if Claude Code adds `_meta` injection support.

#### Option C: Shared file sidecar

Hook writes the signature to a file (e.g., `~/.unimatrix/last-auth.json`), and the Unimatrix server reads it before processing the tool call.

**Problem**: TOCTOU (Time-of-Check-to-Time-of-Use) race condition [16]. Between the hook writing the file and the server reading it, a concurrent tool call could overwrite it. This is especially problematic with Claude Code's parallel hook execution and potential multi-agent setups.

**Mitigations**: Use unique filenames per request (e.g., `auth-{nonce}.json`) and include the nonce in the tool arguments so the server knows which file to read. This adds complexity without clear benefit over Option A.

**Status**: Viable but inferior to Option A.

#### Option D: Out-of-band Unix socket

Hook sends the signature to the Unimatrix server via a Unix domain socket, out-of-band from the MCP transport.

**Problem**: Unimatrix uses stdio transport (stdin/stdout) for MCP communication. Adding a Unix socket listener adds architectural complexity. Additionally, correlating the out-of-band signature with the in-band tool call requires a shared identifier (the nonce).

**Advantage**: Could use `SO_PEERCRED` [17] to verify the hook's process identity at the kernel level, adding another authentication factor. The kernel populates the `ucred` structure with PID, UID, and GID, which cannot be spoofed.

**Status**: Viable for enhanced security (see Section 8), but unnecessary for v0.1.

### 5.2 Recommended Approach: Option A with `_auth_*` fields

Option A is recommended because:
1. It uses documented, supported Claude Code mechanisms
2. The signature travels atomically with the request (no TOCTOU)
3. Server-side extraction requires no additional infrastructure
4. It works with stdio transport (no additional sockets or files)

The hook script receives the tool call on stdin, signs it, and returns `updatedInput` with the original parameters plus `_auth_*` fields. The `updatedInput` completely replaces `tool_input`, so the hook must preserve all original fields.

### 5.3 Hook Implementation Sketch

```bash
#!/bin/bash
# .claude/hooks/unimatrix-sign.sh
# PreToolUse hook for Unimatrix MCP tool call signing

set -euo pipefail

# Read hook input from stdin
INPUT=$(cat)

# Extract tool info
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name')
TOOL_INPUT=$(echo "$INPUT" | jq -c '.tool_input')

# Only sign Unimatrix MCP calls
if [[ "$TOOL_NAME" != mcp__unimatrix__* ]]; then
  exit 0  # Allow non-Unimatrix calls without signing
fi

# Generate nonce and timestamp
NONCE=$(openssl rand -hex 16)
TIMESTAMP=$(date +%s)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id')

# Construct canonical intent (sorted keys, compact)
CANONICAL=$(jq -ncS \
  --arg tool "$TOOL_NAME" \
  --arg ts "$TIMESTAMP" \
  --arg nonce "$NONCE" \
  --arg sid "$SESSION_ID" \
  --argjson input "$TOOL_INPUT" \
  '{nonce: $nonce, session_id: $sid, timestamp: ($ts|tonumber), tool_input: $input, tool_name: $tool}')

# Sign with HMAC-SHA256
KEY_FILE="$HOME/.unimatrix/hook-key"
SIGNATURE=$(echo -n "$CANONICAL" | openssl dgst -sha256 -hmac "$(cat "$KEY_FILE")" -binary | base64)

# Construct updatedInput: original params + auth fields
UPDATED_INPUT=$(echo "$TOOL_INPUT" | jq -c \
  --arg sig "$SIGNATURE" \
  --arg ts "$TIMESTAMP" \
  --arg nonce "$NONCE" \
  --arg signer "hook-v1" \
  '. + {_auth_signature: $sig, _auth_timestamp: ($ts|tonumber), _auth_nonce: $nonce, _auth_signer: $signer}')

# Return updatedInput to Claude Code
jq -nc \
  --argjson updated "$UPDATED_INPUT" \
  '{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "allow", updatedInput: $updated}}'

exit 0
```

### 5.4 Hook Configuration

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

The matcher `mcp__unimatrix__.*` ensures only Unimatrix MCP tool calls are signed, avoiding interference with other tools.

---

## 6. Attack Resistance Analysis

### 6.1 Threat Model

| # | Attack | Vector | Mitigation | Residual Risk |
|---|--------|--------|------------|---------------|
| A1 | **LLM forges signature** | LLM includes `_auth_signature` in its tool call parameters | Hook **replaces** all parameters via `updatedInput`, overwriting any LLM-provided `_auth_*` fields. The LLM's forged values are discarded. | None. The hook's `updatedInput` is authoritative. |
| A2 | **LLM calls tool without triggering hook** | LLM finds a way to bypass the hook | Hooks are enforced by Claude Code's runtime, not by the LLM. The LLM has no mechanism to bypass hooks. Claude Code captures a snapshot of hooks at startup and uses it throughout the session [12]. | Depends on Claude Code's hook enforcement guarantees. |
| A3 | **LLM reads signing key from filesystem** | LLM uses Bash tool to `cat ~/.unimatrix/hook-key` | Protect key file with `chmod 600`. Add a PreToolUse hook on Bash that blocks reads of `~/.unimatrix/`. The key path should not appear in any prompt or CLAUDE.md. | Medium. Requires defense-in-depth (file permissions + hook blocking). |
| A4 | **Replay attack** | Attacker captures a valid signed request and replays it | Timestamp window (30s) + nonce uniqueness check. Server maintains nonce cache with TTL. | Low. Requires interception within the 30s window AND before the nonce is consumed. |
| A5 | **Parameter tampering after signing** | Parameters modified between hook signing and server receipt | Not possible. The hook's `updatedInput` is the final input. Claude Code sends `updatedInput` directly as the MCP `arguments`. There is no intermediate step where parameters can be modified. | None, assuming Claude Code correctly forwards `updatedInput`. |
| A6 | **TOCTOU: parameters differ between what hook signs and what server receives** | Race condition between signing and sending | The hook signs `tool_input` from stdin and returns the SAME data (plus `_auth_*` fields) as `updatedInput`. The server receives exactly what the hook signed. No file-based intermediary. | None. The signed data IS the sent data. |
| A7 | **Hook process compromise** | Attacker replaces the hook script | Hook scripts are loaded at session startup. Mid-session modifications trigger a `ConfigChange` event and require user review before taking effect [12]. The script should be committed to the repo and protected by git integrity. | Medium. Requires local filesystem access. Mitigated by managed policy settings in enterprise environments. |
| A8 | **Prompt injection forces different parameters** | Injected prompt manipulates what parameters the LLM generates | The hook signs whatever parameters the LLM generated. If the LLM was manipulated into generating malicious parameters, those parameters will be signed. The signature proves the call went through the hook, NOT that the parameters are benign. | High. This is outside the scope of signing. Content scanning (vnc-002) addresses this. |
| A9 | **Key exfiltration via side channel** | LLM accesses key material indirectly | The key is never in the LLM's context window. The hook process is a separate child process with its own memory space. The LLM cannot access the hook's memory or environment variables. | Low. Standard process isolation. |
| A10 | **Clock skew exploitation** | Attacker manipulates system clock to extend replay window | NTP synchronization. For local (stdio) transport, client and server share the same system clock, making skew a non-issue. | None for stdio transport. |

### 6.2 What Hook-Based Signing Does NOT Protect Against

It is critical to understand the **scope limitations** of this mechanism:

1. **Malicious intent from the LLM**: If the LLM decides to store harmful content or access unauthorized data, the hook will faithfully sign that intent. The signature proves provenance, not benevolence.

2. **Social engineering of parameters**: If a prompt injection causes the LLM to change `agent_id: "admin"` in its parameters, the hook will sign those parameters. The server must independently verify agent identity through other means (process credentials, session binding).

3. **Denial of service**: An LLM could rapidly generate tool calls, each properly signed by the hook. Rate limiting must be enforced server-side.

4. **Content-based attacks**: The hook signs the structure, not the semantics. Content scanning (regex patterns, etc.) is a separate concern handled by Unimatrix's content scanning module (vnc-002).

### 6.3 Defense-in-Depth Stack

Hook-based signing is one layer in a multi-layer security architecture:

```
+----------------------------------------------------------+
| Layer 5: Content Scanning (vnc-002)                      |
|   Regex patterns, category allowlists, content policies  |
+----------------------------------------------------------+
| Layer 4: Capability Enforcement (vnc-002)                |
|   Agent trust levels, per-tool permissions               |
+----------------------------------------------------------+
| Layer 3: Intent Verification (THIS DOCUMENT)             |
|   Cryptographic signing, replay prevention, nonce check  |
+----------------------------------------------------------+
| Layer 2: Agent Identity Resolution (vnc-001)             |
|   Agent registry, auto-enrollment, trust levels          |
+----------------------------------------------------------+
| Layer 1: Transport Security                              |
|   stdio (process isolation), future: TLS for HTTP        |
+----------------------------------------------------------+
```

---

## 7. Intent Evaluation on the Server Side

### 7.1 Server-Side Verification Flow

When Unimatrix receives a tool call with `_auth_*` fields:

```
Receive tools/call request
    |
    v
Extract _auth_signature, _auth_timestamp, _auth_nonce, _auth_signer
    |
    v
Strip _auth_* fields from arguments -> clean_arguments
    |
    v
Reconstruct canonical intent from (tool_name, clean_arguments, timestamp, nonce, session_id)
    |
    v
Verify signature against canonical intent using stored key
    |
    +-- FAIL --> Reject request (log attempt in AUDIT_LOG)
    |
    v (PASS)
Check timestamp freshness: abs(now - _auth_timestamp) < 30s?
    |
    +-- FAIL --> Reject request (stale signature)
    |
    v (PASS)
Check nonce uniqueness: _auth_nonce not in nonce_cache?
    |
    +-- FAIL --> Reject request (replay detected)
    |
    v (PASS)
Add nonce to cache with TTL = 30s
    |
    v
Mark request as "hook-verified" in audit context
    |
    v
Proceed to normal tool execution pipeline
    (agent resolution -> capability check -> content scan -> execute)
```

### 7.2 What Unimatrix Can Do with Verified Intent

#### Anti-Tampering Verification
The server reconstructs the canonical intent from the received parameters (minus `_auth_*` fields) and verifies it matches the signature. If any parameter was modified in transit, the signature will not match.

#### Signer Identity Verification
The `_auth_signer` field identifies which signing entity produced the signature. Unimatrix can maintain a registry of known signers and their public keys, enabling:
- Multiple signing entities (different hooks for different environments)
- Key rotation (old key still valid for `rotation_grace_period`)
- Revocation (compromised key removed from registry)

#### Audit Provenance
Every tool call in the AUDIT_LOG can record:
- Whether the call was hook-verified (`hook_verified: true/false`)
- The signer identity
- The nonce (for correlation)
- The verification result

This creates an immutable audit trail distinguishing verified calls from unverified ones.

#### Intent-Based Access Control (IBAC)

The signed intent enables a novel access control pattern:

```
Traditional RBAC:    agent_id -> role -> permissions -> allow/deny
Intent-Based (IBAC): signed_intent -> {tool, params} -> policy evaluation -> allow/deny
```

With IBAC, the server can evaluate policies based on what the hook observed:
- "The hook verified this is a `context_search` call with `topic: architecture`" -> grant read access
- "The hook verified this is a `context_store` call" -> check if the signer is authorized for writes
- "No hook signature present" -> apply restricted fallback policy

This aligns with the Agent Integrity Framework's principle of shifting from "can this agent access this resource?" to "should this agent be accessing this resource right now, for this task?" [18].

#### Graduated Trust Levels

| Verification State | Trust Level | Capabilities |
|-------------------|-------------|--------------|
| Hook-signed + valid signature + fresh nonce | **Full** | All authorized capabilities |
| Hook-signed + valid signature + stale timestamp | **Degraded** | Read-only capabilities |
| No signature present | **Minimal** | Search only, no writes |
| Invalid signature | **Rejected** | Request denied, audit logged |

---

## 8. Practical Implementation Patterns

### 8.1 Pattern 1: HMAC-SHA256 Shared Secret (v0.1)

**Architecture**: Symmetric key shared between hook and server.

```
Installation:
  1. openssl rand -base64 32 > ~/.unimatrix/hook-key
  2. chmod 600 ~/.unimatrix/hook-key
  3. Unimatrix server reads same file on startup

Signing (hook):
  canonical = JCS(tool_name, tool_input, timestamp, nonce)
  signature = HMAC-SHA256(key, canonical)

Verification (server):
  expected = HMAC-SHA256(key, reconstructed_canonical)
  valid = constant_time_compare(signature, expected)
```

**Key provisioning**: Generated once during Unimatrix installation or first run. Stored at `~/.unimatrix/hook-key` with `600` permissions. Both the hook and the server read from the same file.

**Pros**:
- Simple implementation (~50 lines of shell for hook, ~30 lines of Rust for server)
- Fast: HMAC-SHA256 completes in ~1 microsecond
- No key management infrastructure needed
- Standard pattern used by every major webhook provider [8]

**Cons**:
- Symmetric: anyone who can read the key file can forge signatures
- No non-repudiation: the server could have produced the signature itself
- Single key: compromise requires full rotation

**Best for**: Single-developer, single-machine setups. The Unimatrix v0.1 target.

### 8.2 Pattern 2: Ed25519 Asymmetric Signing (v0.2)

**Architecture**: Hook holds private key, server holds public key.

```
Key generation:
  1. openssl genpkey -algorithm Ed25519 -out ~/.unimatrix/hook-key.pem
  2. openssl pkey -in ~/.unimatrix/hook-key.pem -pubout -out ~/.unimatrix/hook-key.pub
  3. chmod 600 ~/.unimatrix/hook-key.pem
  4. Server loads ~/.unimatrix/hook-key.pub

Signing (hook):
  canonical = JCS(tool_name, tool_input, timestamp, nonce)
  signature = Ed25519.sign(private_key, canonical)

Verification (server):
  valid = Ed25519.verify(public_key, canonical, signature)
```

**Pros**:
- Non-repudiation: only the private key holder can sign
- Supports multiple signers (multiple public keys in server registry)
- Public key can be distributed without security risk
- Still very fast: ~87us sign, ~228us verify [9]

**Cons**:
- Requires key pair management
- `openssl` Ed25519 operations may not be available on older systems
- Slightly more complex hook script (but manageable)

**Best for**: Multi-user environments, enterprise deployments, audit-required scenarios.

### 8.3 Pattern 3: Biscuit Token Minting (Future)

**Architecture**: Hook mints a per-request Biscuit token embedding both the signed intent and authorized capabilities.

```
Authority key setup:
  1. Generate Biscuit authority key pair
  2. Hook holds authority private key
  3. Server holds authority public key

Token minting (hook):
  facts = {
    tool("context_store"),
    param("topic", "architecture"),
    timestamp(1708790400),
    nonce("a7f3b2c1...")
  }
  caveats = {
    check if time($time), $time < 1708790430;  // 30s expiry
    check if tool($t), $t == "context_store";   // bound to this tool
  }
  token = Biscuit.mint(authority_key, facts, caveats)

Verification (server):
  authorizer = Authorizer.new()
  authorizer.add_fact(time(now))
  authorizer.add_fact(tool("context_store"))
  authorizer.add_policy("allow if tool($t)")
  result = authorizer.authorize(token)
```

Biscuit tokens [10] combine Ed25519 signing with a Datalog-based authorization language. The hook mints a token that is:
- **Cryptographically signed**: proving it was issued by the authority
- **Self-contained**: carries all the facts and caveats needed for authorization
- **Attenuable**: intermediate parties can add caveats (restrictions) without re-signing [19]
- **Time-bounded**: caveats can enforce expiry

The Macaroon [20] pattern is a predecessor: chained HMAC functions create tokens that can only be restricted, never expanded. Biscuit improves on Macaroons with Ed25519 (vs. HMAC chains) and a formal logic language (Datalog) for expressing authorization policies.

**Pros**:
- Combines signing and authorization in one token
- Rich policy language (Datalog)
- Offline attenuation (delegation without contacting the authority)
- Well-supported in Rust (`biscuit-auth` crate)

**Cons**:
- Requires Biscuit integration in both hook (shell or compiled binary) and server (Rust)
- More complex key management (authority keys + block keys)
- Over-engineered for single-developer setups

**Best for**: Multi-project, multi-agent environments with complex capability requirements.

### 8.4 Key Rotation Strategy

For all patterns:

```
Phase 1 (Rotation start):
  - Generate new key pair
  - Server loads BOTH old and new keys
  - Hook begins signing with new key

Phase 2 (Grace period):
  - Server accepts signatures from either key
  - Duration: 24 hours (configurable)

Phase 3 (Old key removal):
  - Remove old key from server's key registry
  - Delete old key file
```

This dual-key approach ensures zero downtime during rotation, following the same pattern used by Google, AWS, and other services for HMAC key rotation [21].

---

## 9. Limitations and Edge Cases

### 9.1 Client Compatibility

| MCP Client | Hook Support | Signing Feasible? | Fallback |
|------------|-------------|-------------------|----------|
| **Claude Code CLI** | Full (PreToolUse with `updatedInput`) | Yes | N/A |
| **Claude Code (API/SDK)** | Full (hooks in Agent SDK) [12] | Yes | N/A |
| **Cursor** | No native hook system | No | MCP proxy gateway |
| **Windsurf** | No native hook system | No | MCP proxy gateway |
| **Custom MCP clients** | Varies | Depends on implementation | MCP proxy gateway |
| **Direct MCP connections** | No hooks | No | Must use proxy |

For non-Claude-Code clients, the signing functionality must be moved from the hook to an **MCP proxy/gateway** that sits between the client and the server. This is architecturally equivalent (transparent intermediary with signing authority) but requires a different deployment model.

### 9.2 User Disables Hooks

If the user sets `"disableAllHooks": true` in their settings [12]:
- No signatures will be attached to tool calls
- Unimatrix receives unsigned requests
- **Policy decision**: Unimatrix can either:
  - **Reject** all unsigned requests (strict mode)
  - **Accept** unsigned requests with reduced capabilities (degraded mode)
  - **Accept** unsigned requests with full capabilities but log a warning (permissive mode)

Recommended: **Degraded mode** for v0.1 (unsigned requests get read-only access), with a configuration option to switch to strict mode.

### 9.3 Non-MCP Tool Calls

The hook-based signing pattern is specific to MCP tool calls (matched by `mcp__unimatrix__.*`). It does not protect:
- Bash tool calls (the LLM could theoretically interact with the MCP server directly via `curl` or a script)
- File system operations
- Web fetches

**Mitigation**: For stdio transport, the MCP server is only accessible via stdin/stdout of the Claude Code process. The LLM cannot bypass this by using Bash, because the MCP connection is managed by Claude Code's runtime, not by a network socket the LLM could address.

### 9.4 Performance

| Operation | Latency | Impact |
|-----------|---------|--------|
| Hook process spawn | ~5-10ms | Shell process startup |
| JSON parsing (jq) | ~1-2ms | Extracting fields |
| Canonicalization | ~0.5ms | Sorting and serializing |
| HMAC-SHA256 signing | ~0.001ms | Negligible |
| Ed25519 signing | ~0.087ms | Negligible |
| Total hook overhead | ~7-15ms | Negligible vs. LLM inference (~2-10s) |

The total overhead of ~7-15ms is **0.1-0.7%** of a typical tool call round-trip (including LLM inference, MCP processing, and response formatting). This is well within acceptable limits.

For high-frequency tool calls, the hook could be implemented as a compiled binary (Rust or Go) instead of a shell script, reducing process spawn overhead to ~1-2ms.

### 9.5 Streaming and Long-Running Tool Calls

MCP tool calls are request-response: the client sends a `tools/call` request and receives a result. There is no streaming in the tool call itself (streaming applies to LLM text generation, not tool execution). The signing pattern works naturally with request-response semantics.

For tools that take a long time to execute, the signature is verified at the **start** of execution, not at the end. The execution duration does not affect signature validity.

### 9.6 Multi-Round-Trip Tool Calls

MCP does not support multi-round-trip tool calls within a single `tools/call` invocation. If a logical operation requires multiple tool calls (e.g., search then store), each call is independently signed by the hook. The server can correlate them via the `session_id` included in the signed intent.

---

## 10. Comparison to Alternative Intermediary Patterns

### 10.1 Comparison Matrix

| Criterion | Hook-Based Signing | MCP Proxy/Gateway | Env Var Tokens | Unix Socket SO_PEERCRED | OAuth 2.1 |
|-----------|-------------------|-------------------|----------------|------------------------|-----------|
| **Deployment** | Config file entry | Separate process/container | Env var in shell | Separate socket listener | Auth server + token endpoint |
| **Per-request signing** | Yes (every call) | Yes (every call) | No (static token) | No (process identity only) | Per-token (not per-call) |
| **Replay prevention** | Nonce + timestamp | Nonce + timestamp | No (token is static) | N/A (connection-level) | Token expiry |
| **Parameter integrity** | Full (signs all params) | Full (signs all params) | None (token independent of params) | None (identity only) | None (token independent of params) |
| **LLM cannot forge** | Yes (key outside context) | Yes (key in proxy) | Partially (env var accessible via Bash) | Yes (kernel-level) | Partially (token in context) |
| **Non-repudiation** | Ed25519: yes; HMAC: no | Depends on signing algo | No | No | Yes (if using signed JWTs) |
| **Multi-client support** | Claude Code only | Any MCP client | Any MCP client | Unix only | Any HTTP client |
| **Setup complexity** | Low (shell script + key file) | Medium (proxy process) | Very low | Medium (socket listener) | High (auth server) |
| **Latency overhead** | ~7-15ms | ~5-20ms | ~0ms | ~0.1ms | ~50-200ms (token fetch) |
| **Standards-based** | RFC 8785 (JCS), RFC 9421 model | Vendor-specific | No standard | POSIX | RFC 6749/6750 |
| **Intent binding** | Strong (signs tool+params) | Strong (signs tool+params) | None | None | Weak (scoped to resource, not call) |

### 10.2 When to Use Each Pattern

| Pattern | Best For | Not For |
|---------|----------|---------|
| **Hook-based signing** | Claude Code users, single-machine, per-call integrity, low setup overhead | Multi-client environments, non-Claude-Code IDEs |
| **MCP proxy/gateway** | Multi-client environments, enterprise, centralized policy enforcement | Simple single-developer setups (over-engineered) |
| **Env var tokens** | Quick prototyping, low-security environments | Any production use (static token, no per-call binding) |
| **Unix SO_PEERCRED** | Process identity verification on Linux, high-assurance environments | Cross-platform, macOS (not supported), Windows |
| **OAuth 2.1** | HTTP transport, multi-tenant, standards-required environments | stdio transport, single-machine, per-call signing |

### 10.3 Hook-Based Signing vs. MCP Proxy/Gateway

The MCP gateway pattern [22][23] deploys a separate process (e.g., Envoy AI Gateway, Acuvity Minibridge) between clients and MCP servers:

```
Hook-Based:
  LLM -> [Claude Code + Hook] -> MCP Server

Gateway-Based:
  LLM -> MCP Client -> [Gateway/Proxy] -> MCP Server
```

Key differences:

| Aspect | Hook-Based | Gateway |
|--------|-----------|---------|
| **Deployment** | Config in settings.json | Separate process/container |
| **Scope** | Per-client-instance | Centralized for all clients |
| **Signing authority** | Hook process | Gateway process |
| **Client coupling** | Tightly coupled to Claude Code | Client-agnostic |
| **Maintenance** | Shell script in repo | Infrastructure component |

**Recommendation**: Use hook-based signing for Claude Code-centric deployments (Unimatrix's primary target). Provide gateway support as an alternative for non-Claude-Code clients.

### 10.4 Hook-Based Signing vs. OAuth 2.1

OAuth 2.1 is now part of the MCP specification (June 2025) [24] for HTTP-based transports. However:

- Unimatrix uses **stdio transport**, where the MCP spec explicitly says "implementations using STDIO transport SHOULD NOT follow [the OAuth] specification, and instead retrieve credentials from the environment" [14].
- OAuth tokens are **session-scoped**, not **per-call-scoped**. A valid OAuth token authorizes all calls within its scope, while hook-based signing verifies each individual call.
- OAuth does not provide **parameter integrity**: the token proves identity, not that the parameters haven't been tampered with.

Hook-based signing and OAuth are **complementary**, not competing. OAuth handles identity at the session level; hook signing handles integrity at the call level.

### 10.5 Relationship to RFC 9421 (HTTP Message Signatures)

RFC 9421 [25] defines a mechanism for creating digital signatures over HTTP message components (headers, method, path, body). The hook-based signing pattern draws direct inspiration from RFC 9421's approach:

| RFC 9421 Concept | Hook-Based Equivalent |
|-----------------|----------------------|
| **Covered components** (which HTTP fields to sign) | **Canonical intent** (tool_name, tool_input, timestamp, nonce) |
| **Signature base** (deterministic string to sign) | **JCS-canonicalized JSON** |
| **Signature parameters** (algorithm, keyid, created, nonce) | **_auth_signer, _auth_timestamp, _auth_nonce** |
| **Signature label** | **_auth_signature** |

RFC 9421 supports selective signing (only certain headers), which could be adapted to sign only security-critical parameters while leaving others unsigned (e.g., sign `agent_id` and `content` but not `format`).

---

## 11. Novel Patterns Enabled by Hooks

The hook-based intermediary pattern enables several novel security patterns beyond basic signing:

### 11.1 Capability Budget

The hook maintains state across calls (via a file or embedded database) to track resource consumption:

```bash
# Pseudocode
BUDGET_FILE="$HOME/.unimatrix/budget-$SESSION_ID.json"
WRITES_DONE=$(jq '.writes' "$BUDGET_FILE")
MAX_WRITES=50

if [[ "$TOOL_NAME" == *"context_store"* ]] && (( WRITES_DONE >= MAX_WRITES )); then
  echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Write budget exhausted (50/50)"}}'
  exit 0
fi

# Increment counter
jq ".writes += 1" "$BUDGET_FILE" > "$BUDGET_FILE.tmp" && mv "$BUDGET_FILE.tmp" "$BUDGET_FILE"
```

This enables per-session or per-agent write quotas, enforced deterministically outside the LLM's control. The hook becomes a **spending limiter** for tool invocations [26].

### 11.2 Time-Based Restrictions

```bash
HOUR=$(date +%H)
if (( HOUR < 9 || HOUR > 17 )); then
  # Outside business hours: deny write operations
  if [[ "$TOOL_NAME" == *"context_store"* || "$TOOL_NAME" == *"context_correct"* ]]; then
    echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Write operations restricted to 09:00-17:00"}}'
    exit 0
  fi
fi
```

### 11.3 Content-Based Pre-Screening

The hook can scan content before signing, refusing to sign tool calls with suspicious content:

```bash
CONTENT=$(echo "$TOOL_INPUT" | jq -r '.content // empty')
if echo "$CONTENT" | grep -qiE '(password|secret|api.key|private.key|BEGIN RSA)'; then
  # Refuse to sign: content contains potential secrets
  echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Content contains potential secrets. Review before storing."}}'
  exit 0
fi
# Otherwise, proceed with signing...
```

This creates a **pre-server content gate**: suspicious content is blocked before it even reaches Unimatrix, adding a layer of defense before the server's own content scanning (vnc-002).

### 11.4 Dead Man's Switch

A "dead man's switch" pattern [27] where the hook's health determines tool call availability:

```bash
HEARTBEAT_FILE="$HOME/.unimatrix/hook-heartbeat"
# Update heartbeat on every call
date +%s > "$HEARTBEAT_FILE"
```

Server-side, a background thread checks the heartbeat file. If no heartbeat for >60 seconds, the server assumes the hook is not running and switches to restricted mode. This implements a **fail-closed** architecture: if the trusted intermediary stops functioning, capabilities are reduced rather than expanded.

### 11.5 Escalation Detection

The hook maintains a sliding window of recent calls and detects anomalous patterns:

```bash
HISTORY_FILE="$HOME/.unimatrix/call-history-$SESSION_ID.jsonl"
echo "{\"tool\":\"$TOOL_NAME\",\"ts\":$(date +%s)}" >> "$HISTORY_FILE"

# Count writes in last 60 seconds
RECENT_WRITES=$(jq -s "[.[] | select(.ts > (now - 60)) | select(.tool | contains(\"store\"))] | length" "$HISTORY_FILE")

if (( RECENT_WRITES > 20 )); then
  # Burst write detected: escalate to user
  echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask","permissionDecisionReason":"Unusual burst: 20+ writes in 60 seconds. Approve?"}}'
  exit 0
fi
```

The `permissionDecision: "ask"` escalates to the human user for confirmation, creating a **behavioral circuit breaker** that triggers on anomalous patterns.

---

## 12. Prior Art and Related Systems

### 12.1 Git Hooks (Pre-Commit, Pre-Push)

Git hooks [4] are the most widely deployed pre-action enforcement pattern in development tooling:

- **Pre-commit hooks**: Run before a commit is finalized. Used for secret scanning (Gitleaks, GitGuardian), linting, formatting, and policy enforcement.
- **Pre-push hooks**: Run before code is pushed to a remote. Used for CI validation and access control.
- **Server-side pre-receive hooks**: Run on the Git server before accepting a push. Cannot be bypassed by the developer.

**Key lesson**: Client-side hooks (pre-commit, pre-push) are powerful but bypassable (`--no-verify`). Server-side hooks (pre-receive) are authoritative. In the Unimatrix model, the hook is client-side, but the **server rejects unsigned requests**, creating the equivalent of a server-side enforcement point that depends on client-side signing.

**Limitation learned from git**: Git pre-commit hooks are "fully local with every developer required to install, configure and run the pre-commit code on their local machine, and developers can easily bypass them" [4]. Unimatrix must handle the case where hooks are bypassed (see Section 9.2).

### 12.2 SSH ForceCommand

SSH ForceCommand [5] is a server-side directive that forces execution of a specific command regardless of what the client requested. The original command is available in `$SSH_ORIGINAL_COMMAND`:

```
# sshd_config
ForceCommand /usr/local/bin/restricted-shell.sh
```

The restricted shell can:
1. Parse the original command
2. Check it against an allowlist
3. Log the attempt
4. Execute or reject

**Analogy**: The hook is like ForceCommand for tool calls. It intercepts the command (tool call), inspects it, and decides whether to proceed. Unlike SSH ForceCommand (server-side, mandatory), the Claude Code hook is client-side and optional.

### 12.3 AWS Lambda Authorizers

AWS API Gateway Lambda authorizers [3] evaluate every API request before it reaches the backend:

1. Client sends request with credentials
2. Gateway invokes Lambda authorizer
3. Authorizer returns an IAM policy (allow/deny + context)
4. Gateway caches the policy and enforces it
5. Backend receives the request enriched with authorizer context

**Key insight**: The Lambda authorizer can **enrich** the request with context from the authorization check. Similarly, the hook enriches the MCP request with `_auth_*` fields and can inject `additionalContext` for the LLM via the `hookSpecificOutput`.

### 12.4 Kubernetes Admission Controllers

Kubernetes ValidatingWebhookConfiguration [28] intercepts API requests before they are persisted:

1. User sends `kubectl apply` (create/update/delete)
2. API server authenticates and authorizes the request
3. Mutating webhooks can modify the object
4. Validating webhooks can reject the object
5. Object is persisted to etcd

**Analogy mapping**:

| Kubernetes Concept | Hook-Based Signing Equivalent |
|-------------------|------------------------------|
| Mutating webhook | Hook with `updatedInput` (modifies the request) |
| Validating webhook | Hook with `permissionDecision: "deny"` (blocks the request) |
| Admission review request | Hook stdin JSON |
| Admission review response | Hook stdout JSON |
| `failurePolicy: Fail` | Server rejects unsigned requests |
| `failurePolicy: Ignore` | Server accepts unsigned requests with degraded capabilities |

The most significant parallel: Kubernetes admission controllers have a **threat model document** [28] that analyzes what happens when webhooks are compromised, bypassed, or misconfigured. The same threat analysis applies to Claude Code hooks.

### 12.5 Envoy AI Gateway MCP Support

The Envoy AI Gateway [22] now provides first-class MCP support (v0.4.0, 2026):

- Acts as a reverse proxy between MCP clients and servers
- Applies authentication (OAuth 2.1), authorization, and rate limiting
- Creates "signed JWT wristbands" containing permitted tools [22]
- Records complete invocation audit logs

This is the gateway equivalent of the hook-based pattern. The gateway and the hook serve the same function (trusted intermediary with signing authority) but deploy differently (separate process vs. in-process hook).

### 12.6 Acuvity Minibridge and Agent Integrity Framework

Acuvity's Minibridge [18] is a backend-to-frontend bridge that secures MCP communication:

- TLS termination between agent and MCP server
- Authentication and threat detection
- Integration with policy engines (Rego-based)
- Content scanning and covert instruction detection

The Agent Integrity Framework [18] defines a maturity model where Level 5 ("Full Runtime Enforcement") includes "inline IBAC and real-time semantic privilege escalation blocking." The hook-based signing pattern directly enables this level by providing per-call intent verification.

### 12.7 Sigstore for Agent Identity

Sigstore's `sigstore-a2a` project [29] applies cryptographic signing to Agent2Agent (A2A) protocol Agent Cards:

- **Keyless signing**: Short-lived certificates tied to OIDC identity tokens
- **Transparency log**: All signatures recorded in an immutable log
- **Supply chain verification**: Verifiable chain of custody from source to deployed agent

While Sigstore targets agent-to-agent identity verification, the hook-based signing pattern could use Sigstore's transparency log to record hook signatures, creating an externally auditable record of all tool call attestations.

### 12.8 CoSAI MCP Security Whitepaper

The Coalition for Secure AI (CoSAI) released a comprehensive MCP security taxonomy [30] identifying 12 core threat categories and ~40 distinct threats. Key relevant findings:

- End-to-end agent identity and traceability are essential
- Input/data sanitization and strict allowlists are mandatory at each trust boundary
- Code signing is mandatory to prevent supply chain attacks
- MCP servers must operate with least privilege using fine-grained authorization

The hook-based signing pattern directly addresses CoSAI's identity and traceability requirements by providing cryptographically verified provenance for every tool call.

---

## 13. Implementation Recommendations for Unimatrix

### 13.1 Phase 1: HMAC-SHA256 with `updatedInput` (v0.1)

**Scope**: Minimum viable hook-based signing for single-developer use.

**Components**:

1. **Key generation** (install time):
   ```
   openssl rand -base64 32 > ~/.unimatrix/hook-key
   chmod 600 ~/.unimatrix/hook-key
   ```

2. **Hook script** (`.claude/hooks/unimatrix-sign.sh`):
   - Read stdin JSON
   - Match only `mcp__unimatrix__*` tools
   - Canonicalize intent with `jq -cS`
   - Sign with `openssl dgst -sha256 -hmac`
   - Return `updatedInput` with `_auth_*` fields

3. **Hook configuration** (`.claude/settings.json`):
   ```json
   {
     "hooks": {
       "PreToolUse": [{
         "matcher": "mcp__unimatrix__.*",
         "hooks": [{
           "type": "command",
           "command": "\"$CLAUDE_PROJECT_DIR\"/.claude/hooks/unimatrix-sign.sh",
           "timeout": 5
         }]
       }]
     }
   }
   ```

4. **Server-side verification** (in `unimatrix-server`):
   - Extract `_auth_*` fields from tool arguments
   - Reconstruct canonical intent
   - Verify HMAC-SHA256 signature
   - Check timestamp freshness (30s window)
   - Check nonce uniqueness (in-memory cache with TTL)
   - Record verification result in AUDIT_LOG

5. **Fallback policy**: Accept unsigned requests with degraded capabilities (read-only).

**Estimated effort**: 2-3 days (hook script + server verification module + tests).

### 13.2 Phase 2: Ed25519 + Key Management (v0.2)

**Additions**:
- Ed25519 key pair generation and management
- Multiple signer support (signer registry in server)
- Key rotation protocol (dual-key grace period)
- Compiled hook binary (Rust) for reduced latency
- Strict mode option (reject all unsigned requests)

### 13.3 Phase 3: Biscuit Tokens + Intent-Based Access Control (v0.3)

**Additions**:
- Biscuit token minting in hook
- Datalog policy engine in server
- Capability budget enforcement via Biscuit caveats
- Delegation chains (attenuated tokens for sub-agents)

### 13.4 Server-Side Schema Changes

The `_auth_*` fields should be handled transparently by the server:

```rust
/// Authentication context extracted from tool call arguments.
/// These fields are injected by the PreToolUse hook and stripped
/// before normal argument processing.
pub struct HookAuth {
    pub signature: String,        // _auth_signature (base64)
    pub timestamp: u64,           // _auth_timestamp (Unix epoch)
    pub nonce: String,            // _auth_nonce (hex)
    pub signer: String,           // _auth_signer (identifier)
}

/// Extract and strip _auth_* fields from tool arguments.
pub fn extract_hook_auth(args: &mut serde_json::Map<String, Value>) -> Option<HookAuth> {
    let sig = args.remove("_auth_signature")?.as_str()?.to_string();
    let ts = args.remove("_auth_timestamp")?.as_u64()?;
    let nonce = args.remove("_auth_nonce")?.as_str()?.to_string();
    let signer = args.remove("_auth_signer")?.as_str()?.to_string();
    Some(HookAuth { signature: sig, timestamp: ts, nonce, signer })
}
```

The MCP tool input schemas should allow additional properties (`"additionalProperties": true`) to accommodate the `_auth_*` fields, or the fields should be explicitly declared in the schema with `"description": "Injected by hook. Do not set manually."`.

### 13.5 Security Hardening Checklist

- [ ] Key file permissions: `chmod 600 ~/.unimatrix/hook-key`
- [ ] Key path not in CLAUDE.md, .env, or any prompt-visible file
- [ ] PreToolUse hook on Bash to block reads of `~/.unimatrix/` directory
- [ ] Nonce cache with TTL-based eviction (prevent unbounded memory growth)
- [ ] Constant-time signature comparison (prevent timing attacks)
- [ ] Hook script committed to repo (integrity via git)
- [ ] Managed policy settings for enterprise hook enforcement [12]
- [ ] AUDIT_LOG records verification results for all tool calls
- [ ] Timestamp validation uses monotonic clock source where available
- [ ] Graceful degradation when hooks are disabled (not hard failure)

---

## 14. References

[1] Istio Architecture. https://istio.io/latest/docs/ops/deployment/architecture/

[2] Microservices Patterns with Envoy Sidecar Proxy. Christian Posta. https://blog.christianposta.com/microservices/00-microservices-patterns-with-envoy-proxy-series/

[3] Use API Gateway Lambda authorizers. AWS Documentation. https://docs.aws.amazon.com/apigateway/latest/developerguide/apigateway-use-lambda-authorizer.html

[4] Git Hooks: Prevent Secrets Exposure with Pre-Commit and Pre-Receive Protection. Orca Security. https://orca.security/resources/blog/git-hooks-prevent-secrets/

[5] The SSH ForceCommand. Pierce Bartine. https://pbar.dev/blog/20230725-ssh-forcecommand

[6] Top 5 Reasons to Use a Pre-Authentication Hook. OneLogin Developer Blog. https://developers.onelogin.com/blog/5-reasons-preauthentication-smart-hook

[7] RFC 8785: JSON Canonicalization Scheme (JCS). IETF. https://www.rfc-editor.org/rfc/rfc8785

[8] How to Secure Webhook Endpoints with HMAC. Prismatic. https://prismatic.io/blog/how-secure-webhook-endpoints-hmac/

[9] Ed25519: High-speed high-security signatures. Daniel J. Bernstein et al. https://ed25519.cr.yp.to/

[10] Biscuit: Delegated, decentralized, capabilities-based authorization token. Eclipse Foundation. https://www.biscuitsec.org/

[11] How do you prevent replay attacks when using HMAC for authentication? LinkedIn Advice. https://www.linkedin.com/advice/0/how-do-you-prevent-replay-attacks-when-using-hmac-authentication

[12] Hooks reference. Claude Code Docs. https://code.claude.com/docs/en/hooks

[13] Feature Request: Enhance PreToolUse Hooks to Modify Tool Inputs. GitHub Issue #4368. https://github.com/anthropics/claude-code/issues/4368

[14] Overview - Model Context Protocol Specification (2025-06-18). https://modelcontextprotocol.io/specification/2025-06-18/basic

[15] Tools - Model Context Protocol Specification (draft). https://modelcontextprotocol.io/specification/draft/server/tools

[16] Time-of-check to time-of-use. Wikipedia. https://en.wikipedia.org/wiki/Time-of-check_to_time-of-use

[17] unix(7) - Linux manual page. SO_PEERCRED. https://man7.org/linux/man-pages/man7/unix.7.html

[18] The Agent Integrity Framework: The New Standard for Securing Autonomous AI. Acuvity. https://acuvity.ai/the-agent-integrity-framework-the-new-standard-for-securing-autonomous-ai/

[19] Macaroons: Cookies with Contextual Caveats for Decentralized Authorization in the Cloud. Google Research. https://research.google/pubs/macaroons-cookies-with-contextual-caveats-for-decentralized-authorization-in-the-cloud/

[20] Macaroons Escalated Quickly. Fly.io Blog. https://fly.io/blog/macaroons-escalated-quickly/

[21] HMAC keys. Google Cloud Storage Documentation. https://cloud.google.com/storage/docs/authentication/hmackeys

[22] Announcing Model Context Protocol Support in Envoy AI Gateway. https://aigateway.envoyproxy.io/blog/mcp-implementation/

[23] 7 top MCP gateways for enterprise AI infrastructure. MintMCP Blog. https://www.mintmcp.com/blog/enterprise-ai-infrastructure-mcp

[24] Understanding OAuth 2.1 in MCP. Composio. https://composio.dev/blog/oauth-2-1-in-mcp

[25] RFC 9421: HTTP Message Signatures. IETF. https://datatracker.ietf.org/doc/rfc9421/

[26] Rate Limiting and Throttling for AI Agents. NeuralTrust. https://neuraltrust.ai/blog/rate-limiting-throttling-ai-agents

[27] Dead man's switch. Wikipedia. https://en.wikipedia.org/wiki/Dead_man%27s_switch

[28] Kubernetes Admission Control Threat Model. Kubernetes SIG Security. https://github.com/kubernetes/sig-security/blob/main/sig-security-docs/papers/admission-control/kubernetes-admission-control-threat-model.md

[29] Building Trust in the AI Agent Economy: Sigstore Meets Agent2Agent. Luke Hinds, DEV Community. https://dev.to/lukehinds/building-trust-in-the-ai-agent-economy-sigstore-meets-agent2agent-44f5

[30] Securing the AI Agent Revolution: A Practical Guide to Model Context Protocol Security. Coalition for Secure AI. https://www.coalitionforsecureai.org/securing-the-ai-agent-revolution-a-practical-guide-to-mcp-security/

[31] From Auth to Action: The Complete Guide to Secure and Scalable AI Agent Infrastructure. Composio. https://composio.dev/blog/secure-ai-agent-infrastructure-guide

[32] Advanced authentication and authorization for MCP Gateway. Red Hat Developer. https://developers.redhat.com/articles/2025/12/12/advanced-authentication-authorization-mcp-gateway

[33] CWE-367: Time-of-check Time-of-use (TOCTOU) Race Condition. MITRE. https://cwe.mitre.org/data/definitions/367.html

[34] The JSON Canonicalisation Scheme (RFC 8785) in action and how to secure JSON objects with HMAC. Connect2id. https://connect2id.com/blog/how-to-secure-json-objects-with-hmac

[35] Securing the Model Context Protocol: A Comprehensive Guide. DasRoot. https://dasroot.net/posts/2026/02/securing-model-context-protocol-oauth-mtls-zero-trust/

[36] Intercept and control agent behavior with hooks. Claude API Docs (Agent SDK). https://platform.claude.com/docs/en/agent-sdk/hooks

[37] From runtime risk to real-time defense: Securing AI agents. Microsoft Security Blog. https://www.microsoft.com/en-us/security/blog/2026/01/23/runtime-risk-realtime-defense-securing-ai-agents/

[38] MCP Server vs MCP Gateway: Architecture Comparison. SkyWork AI. https://skywork.ai/blog/mcp-server-vs-mcp-gateway-comparison-2025/
