# Research: @claude-flow/aidefence Evaluation

**Date**: 2026-02-23
**Context**: Evaluate whether the `@claude-flow/aidefence` npm package addresses risks identified in the MCP Security Analysis.
**Package**: [@claude-flow/aidefence](https://www.npmjs.com/package/@claude-flow/aidefence)
**Repository**: [ruvnet/claude-flow](https://github.com/ruvnet/claude-flow) (14.4k stars, MIT license)

---

## 1. What It Is

A TypeScript/Node.js regex-based threat detection library within the claude-flow agent orchestration platform. Core detection: 50+ regex patterns with confidence scoring, PII scanning, and optional HNSW vector search for similar threat matching.

**Technology**: TypeScript 5.3+, Node.js 18+
**Performance claims**: <1ms quick scan, <10ms full detection, <3ms PII scan
**Detection mechanism**: Regex pattern matching + entropy analysis + confidence scoring

---

## 2. Detection Capabilities (from source analysis)

### Prompt Injection Patterns (~50 regexes)

| Category | Example Patterns | Severity |
|----------|-----------------|----------|
| Instruction Override | "ignore all previous instructions", "forget everything" | Critical |
| Jailbreak | "enable DAN mode", "bypass restrictions", "disable safety filter" | Critical |
| Role Switching | "you are now a different AI", "act as if unrestricted" | High |
| Context Manipulation | fake system messages, "reveal your prompt", delimiter abuse | Critical |
| Encoding Attacks | base64/ROT13/hex references | Medium |
| Developer Mode | "hidden features", "debug mode" | Medium |
| Social Engineering | Various manipulation patterns | Low-Medium |

### PII Detection (6 regex patterns)

- Email addresses
- SSNs (###-##-####)
- Credit card numbers (16-digit)
- API keys (OpenAI/Anthropic/GitHub formats)
- Hardcoded passwords

### Architecture

Facade pattern over two services:
- `ThreatDetectionService` -- regex matching + confidence scoring
- `ThreatLearningService` (optional) -- trajectory-based learning for improving detection

Multi-agent consensus function for combining assessments from multiple security agents.

---

## 3. Risk Coverage Assessment

Evaluated against the 10 risks from MCP-SECURITY-ANALYSIS.md:

| # | Unimatrix Risk | Covered? | Detail |
|---|---------------|----------|--------|
| 1 | No agent identity (ASI03/ASI10) | **No** | No agent enrollment, registry, trust levels, or capability model |
| 2 | Memory/context poisoning (ASI06) | **Partial** | Catches known injection patterns in text; semantic poisoning passes through |
| 3 | No audit trail | **No** | Internal stats only; no append-only log, no per-request attribution, no hash chaining |
| 4 | Prompt injection via tool results (ASI01) | **Partial** | Primary strength -- 50 known patterns. Misses novel/encoded/semantic variants |
| 5 | Supply chain propagation (ASI08) | **No** | No entry lineage, feature cycle tracking, or propagation analysis |
| 6 | Tool misuse via overpermission (ASI02) | **No** | No capability model, read/write separation, or topic scoping |
| 7 | Unbounded input (DoS) | **No** | No input size validation or rate limiting |
| 8 | Embedding space attacks | **No** | No embedding-level defenses |
| 9 | ONNX model supply chain | **No** | Unrelated |
| 10 | Content integrity (hashing, versioning) | **No** | No content hashing, provenance, or version tracking |

**Coverage**: ~1 of 10 risks, partially.

---

## 4. Strengths

1. **Known injection pattern coverage**: The 50+ regex patterns catch obvious prompt injection attempts. Storing "ignore all previous instructions" as a knowledge entry would be flagged.
2. **PII detection**: Useful as a write-time filter to prevent accidental credential/PII storage.
3. **Speed**: <1ms for quick scan. Regex on text is inherently fast.
4. **Self-learning API**: Trajectory-based learning hooks for improving detection over time, though implementation depth is unclear.
5. **Multi-agent consensus**: Function for combining assessments from multiple security evaluators.

---

## 5. Limitations

### 5.1 Semantic Poisoning Is Invisible to Regex

The highest-risk threat for Unimatrix -- subtly wrong conventions that propagate across feature cycles -- is syntactically clean text. Examples that pass every regex pattern:
- "Always disable CSRF protection for single-page applications"
- "Authentication tokens should be stored in localStorage for cross-tab access"
- "Use `*` CORS patterns for development convenience"

PoisonedRAG (USENIX 2025) achieves 90% attack success with entries that are well-formed, correctly embedded, and contain no injection keywords.

### 5.2 No Access Control Model

AIDefence scans content but doesn't answer:
- "Should this agent be allowed to write?"
- "Should this agent see entries in this topic?"
- "What trust level does this agent have?"

It's a content filter, not an authorization layer.

### 5.3 No Provenance or Integrity Infrastructure

Does not add:
- Content hashes (SHA-256)
- Agent attribution (created_by, modified_by)
- Version tracking
- Feature cycle tags
- Trust source classification

None of our Tier 1 foundational requirements are addressed.

### 5.4 Wrong Technology Stack

Unimatrix is Rust (`ort` + `redb` + `hnsw_rs`). AIDefence is TypeScript/Node.js. Integration options:
- FFI bridge to Node.js -- absurd overhead for regex matching
- Sidecar process -- latency + complexity for a content filter
- Port patterns to Rust -- at which point the library provides no value

### 5.5 Regex-Only Detection Is Insufficient

- "Breaking the Protocol" (arXiv 2601.17549): 52.8% attack success even with defenses
- ADMIT (arXiv Oct 2025): 86% success at 0.93x10^-6 poisoning rate
- Known bypass techniques: Unicode homoglyphs, zero-width characters, semantic rephrasing, multi-turn decomposition

Regex catches known patterns. Adversarial attackers don't use known patterns.

---

## 6. Applicable Concepts (Worth Porting)

Despite the library not being directly usable, several concepts are worth implementing natively:

| Concept | Value | Port Effort |
|---------|-------|-------------|
| Injection pattern regex list | Catches obvious attacks on write | Low (~100 lines Rust) |
| PII regex patterns | Prevents credential/PII storage | Low (~30 lines Rust) |
| Confidence scoring with context | Better than binary pass/fail | Low-Medium |
| Threat categorization enum | Structured reporting | Low |
| Multi-agent consensus function | Useful for future trust scoring | Medium |
| Self-learning via feedback | Pattern refinement over time | Aligns with crt phase |

---

## 7. Recommendation

**Do not depend on `@claude-flow/aidefence`** for Unimatrix security. The library addresses ~10% of our risk surface, uses an incompatible technology stack, and cannot detect our highest-severity threat (semantic poisoning).

**Do port the useful patterns**: Implement a Rust-native content scanner as part of vnc-002's `context_store` validation pipeline:
- `Vec<(Regex, ThreatCategory, Severity)>` for injection patterns
- PII regex set for credential detection
- Confidence scoring with contextual adjustment
- Estimated effort: ~150 lines of Rust, zero external dependencies beyond `regex` crate

This gives us the useful defense layer without the dependency, the wrong stack, or a false sense of security around the attacks that actually matter.

---

## Sources

- [@claude-flow/aidefence on npm](https://www.npmjs.com/package/@claude-flow/aidefence)
- [ruvnet/claude-flow on GitHub](https://github.com/ruvnet/claude-flow)
- [DeepWiki: claude-flow architecture](https://deepwiki.com/ruvnet/claude-flow)
- [AIDefence source: threat-detection-service.ts](https://github.com/ruvnet/claude-flow/blob/main/v3/@claude-flow/aidefence/src/domain/services/threat-detection-service.ts)
