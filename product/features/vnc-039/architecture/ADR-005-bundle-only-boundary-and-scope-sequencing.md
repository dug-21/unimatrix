## ADR-005: Cloud MCP is bundle-only; Scope B (credential relocation) is independent and lands first (SR-05, SR-06, OQ-2)

### Context

Two boundary decisions shape delivery.

**Bundle-only (OQ-2 resolved → option c, #773).** The legacy `--remote`/`--token` (env-HTTPS) path has **no** `fp` pin (#773). Bridging it would write the bearer over an unverified TLS connection — a security regression — and adding MCP to the legacy path violates vnc-038's principle "keep legacy working, don't extend it." SR-06 warns the failure mode is a **silent skip** leaving users a dead `context_*` surface with no signal.

**Scope coupling (SR-05).** This feature folds two issues (#775 bridge, #776 creds) and resolves #773. #774 has since **merged** (PR #778, commit 913b78cb, 2026-06-18): rmcp `allowed_hosts` is wired from `UNIMATRIX_PUBLIC_URL`, so remote MCP requests no longer 403 and Scope A is **live-validatable**. Scope A is therefore no longer blocked for live validation. B still ships as a standalone deliverable without a reachable cloud and lands first — but now by **risk and independence**, not by a #774 block: B is the independent, lower-risk fix that removes a current live commit-leak vector, so it leads (SR-05); A follows and is validated live in the same delivery.

### Decision

**Cloud MCP is bundle-only.** On the **bundle path** (`--bundle <v:2>`), `init` writes the credential store (Scope B) **and** the stdio `.mcp.json` bridge entry (Scope A). On the **legacy path** (`--remote`/`--token`), `init` writes **no** bridge entry and emits a **loud, deterministic** message: cloud MCP requires a `v:2` bundle (exact wording + non-zero/exit behavior to be a testable AC, SR-06 — not prose, not a silent skip). The legacy observe path continues unchanged (it has always been unpinned; this feature does not extend it). This resolves #773 by **deprecating env-HTTPS for cloud-attach MCP**.

Note: Scope B's relocation is **universal** — no remote path writes an in-tree credential. The legacy credential is written to the store with `fingerprint: null`; the hook client's pin stays off for legacy (ADR-004), preserving today's behavior. The bundle credential carries a real `fingerprint`, so the bundle observe path becomes correctly pinned.

**Scope A/B independence + B-first sequencing.** Component boundaries enforce it:
- **Scope B** = `credstore.js` (C1), the hook-client `resolve()` repoint (C5), the store-write half of `initRemote` (C4), and the legacy-message half (C6). It ships **without a reachable cloud**: validate by writing the store on `init --bundle`, asserting nothing token-bearing lands in the repo tree (`git status`/`git add -A` surface no secret), and asserting both consumers resolve from the store (hook client pins + posts; bridge config-loads). **No #774 dependency. Lands first** (AC-11).
- **Scope A** = `mcp-bridge.js` (C2), the `mcp-bridge` entrypoint (C3, ADR-002), and the `.mcp.json`-write half of `initRemote` (C4). It depends on B for its config source. With #774 merged its **live** validation is now available — remote MCP requests reach the rmcp endpoint instead of 403ing, so the bridge round-trip and the SSE-skip probe run live in delivery (AC-03). The "not-validated-live-until-#774" caveat (SR-04) no longer applies.

B leads on **risk and independence**, not on an A-side block: B is independent (no reachable cloud) and lower-risk (it removes a current live credential-leak vector), so it merges first; A follows in the same delivery, now live-validatable. Sequence: **B → A.**

### Consequences

**Easier:** the independent, fully-validatable security fix (B, removing the commit-leak vector at root) still ships first regardless of the bridge; with #774 merged, Scope A is now live-validatable in the same delivery (no trailing live-validation debt); the bundle-only boundary keeps the unpinned legacy path from ever carrying a bearer over unverified TLS; #773 is resolved by deprecation, not by adding a fragile pin flag to legacy.

**Harder:** delivery is still two merges, not one (B first by risk/independence, A after) — more coordination, but the explicit boundary keeps the credential-relocation fix decoupled from any bridge regression; the legacy path must emit (and test) a precise loud-unsupported message rather than reusing the old silent skip (SR-06). The prior "not-validated-live-until-#774" caveat (SR-04) no longer applies now that #774 has merged — A's ACs are validated live, against the real rmcp wire, in delivery.

Related: ADR-001 (this feature, the bridge that is bundle-only), ADR-003/ADR-004 (this feature, the store B owns), ADR-002 vnc-038 #5081 (`v:2` bundle carries `mcp_url`/`fp` — the bundle-path prerequisite). Resolves #773 (deprecate env-HTTPS for cloud MCP). #774 (rmcp `allowed_hosts`) merged (PR #778, commit 913b78cb, 2026-06-18) — Scope A live-validatable.
