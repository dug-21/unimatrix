# ASS-059: Strix Integration — Adversarial Security Testing for Unimatrix Development

**Date**: 2026-05-22  
**Tier**: 2 — informs development process, not core architecture  
**Feeds**: Wave 2 (W2-1 container packaging, W2-2 HTTPS transport), CI/CD pipeline  
**Related**: W2-1 (Dockerfile), W2-2 (HTTP transport + bearer auth), nan-005 (build/deploy)

---

## Background

Wave 2 moves Unimatrix from local-only UDS to HTTPS-exposed deployment. This is a qualitative increase in attack surface — the knowledge base, its integrity chain, and the admin tools become network-accessible. The current security posture relies on design-time review (security-reviewer agent, PR review) and Rust's compile-time guarantees. There is no automated adversarial testing.

[Strix](https://github.com/usestrix/strix) is an LLM-powered autonomous security testing tool (Apache-2.0, 25.5k stars, actively maintained). It simulates attacker behavior against source code and running applications, producing validated vulnerabilities with proof-of-concept exploits. It runs in headless CLI mode for CI integration and ships a GitHub Actions workflow.

This spike determines whether and how Strix fits into Unimatrix's development and deployment process — before W2-2 exposes the application to network traffic.

---

## The Questions

**Primary**: Is Strix an effective adversarial testing tool for a Rust/Axum HTTPS application backed by SQLite, and what is the practical integration model for our development workflow?

**Secondary**:

1. **Signal-to-noise for Rust**: Rust eliminates memory safety vulnerabilities (buffer overflow, use-after-free, dangling pointers). What fraction of Strix's OWASP Top 10 coverage remains relevant? Specifically: SQL injection (sqlx parameterized queries), command injection (no shell invocation), SSRF, auth bypass (bearer token validation), business logic flaws, race conditions (async concurrency). What does Strix find that our existing security review process does not?

2. **Integration model**: Two integration points — (a) source analysis against the codebase, (b) runtime DAST against a containerized instance after `docker compose up`. Which is higher value? Can both run in CI? What is the scan duration for each mode against a codebase of our size (~25k lines Rust)?

3. **Cost model**: Strix consumes LLM API tokens per scan. What is the per-scan cost against our codebase? Is this sustainable for per-PR CI runs, or should it be gated to release branches / nightly? Which LLM backend (OpenAI, Anthropic, Ollama) gives the best cost/quality tradeoff?

4. **Output format**: Is headless mode output machine-parseable (JSON, SARIF)? Can findings be used as CI gate conditions (fail PR on critical/high severity)? Can findings feed into GitHub Security tab or issue creation?

5. **Secret management**: Strix requires an LLM API key at runtime. How does this integrate with CI secret injection (GitHub Actions secrets, Docker build args)? Does the key need to be available inside the test container or only to the Strix process?

6. **Docker composition**: W2-1 will produce a Dockerfile and docker-compose.yml. Can Strix run as a sidecar or post-start step in the compose environment? What is the minimal test environment — does Strix need a seeded knowledge base, or can it test against an empty instance?

---

## Approach

1. **Install and run Strix locally** against the current Unimatrix codebase in source analysis mode. Record: findings count by severity, false positive rate, scan duration, token consumption.

2. **Build a minimal test container** (can reuse W2-1 Dockerfile if available, otherwise a basic `cargo build --release` + binary container). Run Strix in DAST mode against the containerized instance with bearer token auth enabled. Record: findings count, overlap with source analysis mode, runtime.

3. **Evaluate CI integration**: Review the shipped GitHub Actions workflow. Assess adaptation needs for our Rust/Docker setup. Determine gating strategy (per-PR vs. nightly vs. release).

4. **Cost analysis**: Run 3 scans with different LLM backends. Compare finding quality and token cost. Determine sustainable scan frequency.

---

## Out of Scope

- Implementing CI pipeline changes (that's delivery work)
- Evaluating alternative security tools (SAST/DAST market comparison)
- Fixing any vulnerabilities Strix discovers (file issues, don't fix)
- Strix integration with the Unimatrix knowledge engine itself (storing findings as entries — future opportunity, not this spike)

---

## Output

1. **Findings report**: What Strix found, severity breakdown, false positive assessment, comparison to what our existing security review catches
2. **Integration recommendation**: Source mode, DAST mode, or both. CI gating strategy. Cost/frequency tradeoff.
3. **Go/no-go verdict**: Is Strix worth integrating into our development process for Wave 2 and beyond?
4. **If go**: Draft issue for integration work with effort estimate and dependencies
