# FINDINGS: Strix Integration — Adversarial Security Testing for Unimatrix Development

**Spike**: ASS-059
**Date**: 2026-05-22
**Approach**: Investigation + measurement (partial — Docker unavailable in research environment)
**Confidence**: Directional

---

## Verdict: Conditional Go — DAST Mode Only

Strix adds value as a DAST scanner against the W2-2 HTTP transport. Source analysis mode is not worth the cost or noise for a Rust codebase with `#![forbid(unsafe_code)]`, sqlx parameterized queries, and an OWASP-aware security reviewer agent already in the loop.

Defer integration until W2-2 HTTP transport is implemented. Run one manual deep scan first to validate MCP-over-HTTP compatibility before committing to CI integration.

---

## Findings by Question

### Q1: Signal-to-Noise for Rust

**~30-40% of OWASP coverage remains relevant.** Rust eliminates the largest vulnerability classes.

| Vector | Relevant? | Why |
|--------|-----------|-----|
| Memory safety (A08) | No | All 9 crates use `#![forbid(unsafe_code)]`. Zero `unsafe` in production. |
| SQL injection (A03) | No | All queries use sqlx `?N` bind parameters. `format!` SQL in `read.rs:440` and `write_ext.rs:105` interpolates only internal enum-derived values, never user input. |
| Command injection (A03) | No | Two `Command::new("kill")` in `pidfile.rs` — args are literal `"-0"` and `pid.to_string()` (u32). No user-controlled strings reach shell commands. |
| Auth bypass (A01, A07) | **Yes** | W2-2 bearer token auth needs runtime testing: timing side channels, missing auth on endpoints, token handling. |
| SSRF/path traversal | No | No outbound HTTP. File ops limited to SQLite DB and PID files. |
| Race conditions | Marginal | Async concurrency patterns exist but SQLite-level TOCTOU is not Strix's strength. |
| Supply chain (A06) | No | `cargo audit` is strictly superior for Rust dependency scanning. |

**What our existing security review catches**: The `uni-security-reviewer` agent performs OWASP-aware review on every PR with full source file context. For source-level analysis, it is equivalent to or better than Strix source mode because it understands Rust's type system and can dismiss false positives Strix cannot.

**Strix source analysis would likely flag** the `format!` SQL patterns and `Command::new` invocations — all false positives given the type constraints.

**Incremental value**: DAST mode testing runtime auth behavior, HTTP header handling, and endpoint enumeration. Source analysis adds near-zero signal.

---

### Q2: Integration Model

**DAST mode only. Skip source analysis entirely.**

| Mode | Value | Duration (est.) | Recommendation |
|------|-------|-----------------|----------------|
| Source quick | Low — duplicates security reviewer with worse Rust understanding | 3-8 min | Skip |
| Source standard | Low | 30-60 min | Skip |
| Source deep | Low | 1-4 hours | Skip |
| DAST quick | Moderate — tests runtime auth, endpoints, headers | ~5-10 min + container startup | PR CI (server-crate changes only) |
| DAST standard | High — thorough endpoint + auth testing | ~30-60 min | Nightly |
| DAST deep | High — full penetration simulation | 1-4 hours | Pre-release |

Note: Codebase is ~197k lines across 9 crates (not 25k as originally scoped). Duration scales accordingly.

`--scope-mode diff` reduces source scan scope to changed files but is irrelevant for DAST (tests the running application regardless).

---

### Q3: Cost Model

| Mode | Est. Tokens (in+out) | Claude Sonnet 4.6 | GPT-5.4 | GPT-5.4 Mini |
|------|-----------------------|-------------------|---------|--------------|
| Quick | ~500K/~200K | ~$4.50 | ~$4.25 | ~$1.28 |
| Standard | ~2M/~800K | ~$18 | ~$17 | ~$5.10 |
| Deep | ~8M/~3M | ~$69 | ~$65 | ~$19.50 |

**Monthly budget at recommended cadence**: ~$300/month (5-10 quick PR scans/week + nightly standard + monthly deep).

**LLM backend recommendation**: GPT-5.4 or Claude Sonnet 4.6. For a security tool, false negatives matter more than cost — use a frontier model. GPT-5.4 Mini offers 70% savings but may miss subtle vulnerabilities. Local models (Ollama) eliminate API cost but require GPU infrastructure.

**Sustainability**: Comparable to a few hours of manual penetration testing. Far cheaper than professional pentest engagements ($5k-20k).

---

### Q4: Output Format

- **JSON**: Via `-o report.json` flag. Includes vulnerability details, severity, PoC exploit code, CVSS scores, remediation guidance.
- **SARIF**: No native support. Custom JSON-to-SARIF conversion needed for GitHub Security tab integration.
- **CI gating**: Exit code 0 = clean, exit code 2 = vulnerabilities found. Binary pass/fail. Severity-based gating (fail on critical/high, warn on medium) requires ~50 lines CI scripting to parse JSON output.
- **GitHub issues**: Not built-in. Custom `gh issue create` from parsed JSON. Straightforward.

**Recommendation**: Use exit code 2 as initial CI gate. Add JSON-to-SARIF + severity parsing as follow-on if needed.

---

### Q5: Secret Management

**Standard GitHub Actions secrets. API key stays in Strix process only.**

- Strix binary makes LLM API calls from the CI runner's environment
- Docker sandbox container receives only target URL/path and tool commands — no API key exposure
- Store `LLM_API_KEY` and `STRIX_LLM` as GitHub Actions repository secrets
- Use a dedicated CI API key (separate from development keys) with spend caps and rate limits
- Set LLM provider-side spend caps to prevent runaway costs from compromised CI or infinite-loop scans

---

### Q6: Docker Composition

**Post-`docker compose up` CI step, not sidecar.**

Strix is a host-level binary that orchestrates its own Docker sandbox (`ghcr.io/usestrix/strix-sandbox:0.1.13`). It targets a running instance via URL.

Workflow:
```
docker compose up -d
# wait for health check
strix -n -t https://localhost:8443 --scan-mode quick -o report.json
docker compose down
```

Provide bearer token via `--instruction` so Strix tests both authenticated and unauthenticated paths.

**Minimal test environment**: W2-2 container with HTTP transport on port 8443. Seed 3-5 entries to enable data endpoint testing (search, get, briefing) for information leakage. Empty instance is sufficient for auth and transport testing.

---

## Unanswered Questions

1. **Exact JSON output schema** — not publicly documented. Cannot determine SARIF conversion compatibility without a live scan. Blocked on Docker unavailability.

2. **Actual false positive rate against Rust** — "zero false positives" claim applies to DAST (PoC validation). Source mode likely has higher FP rate for Rust. Requires live scan.

3. **MCP-over-HTTP compatibility** — Strix's HTTP tools assume standard REST/HTML endpoints. MCP-over-HTTP uses JSON-RPC-like framing with tool invocation semantics. Unclear whether Strix can effectively probe MCP-specific vectors (tool parameter injection, malformed JSON-RPC, protocol-level DoS). Requires DAST scan against running W2-2 instance.

4. **Token consumption variance** — estimates are directional. Actual cost depends on agent loop count per scan. First live scan will calibrate.

---

## Out-of-Scope Discoveries

1. **SQL `format!` patterns are safe but fragile**: `read.rs:440` and `write_ext.rs:105` interpolate only internal values, but the pattern invites future injection risk if a developer adds user-derived values to the `conditions` or `sets` vectors. Consider a `# SAFETY` comment or `SafeSqlFragment` newtype.

2. **MCP-over-HTTP is a non-standard attack surface**: Generic DAST scanners may not probe MCP-specific vectors (tool parameter injection, malformed JSON-RPC, protocol-level DoS). A dedicated MCP protocol fuzzer may be more effective. **Warrants a separate spike.**

3. **`Command::new("kill")` container compatibility**: The `kill` binary dependency in `pidfile.rs` (used instead of `libc::kill` due to `#![forbid(unsafe_code)]`) may not be present in minimal container images. W2-1 packaging concern, not security.

---

## Recommendations Summary

| Area | Recommendation |
|------|---------------|
| Mode | DAST only. Skip source analysis. |
| PR CI | Quick DAST on PRs modifying `crates/unimatrix-server/` |
| Nightly | Standard DAST |
| Release | Deep DAST |
| LLM backend | GPT-5.4 or Claude Sonnet 4.6 |
| Budget | ~$300/month |
| CI gating | Exit code 2 (binary). Severity parsing as follow-on. |
| Timing | Defer until W2-2 HTTP transport exists. Manual deep scan first. |
| Follow-up spike | MCP protocol fuzzer evaluation |
