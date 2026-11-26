# nan-024-agent-6-firetime-order Report

Unimatrix entries consulted: #5901 (ADR-004), #5900 (ADR-002), #5904, briefing list (#5775 context)

## Report
Order now identical in lib/shim-entry.js and lib/hook-route.js: isUnimatrixProject -> read-only installed decision -> trust -> (post-trust not-installed outcome) -> pointer/launcher load -> resolveRuntime -> client.main.
- Not a project: silent 0, nothing read.
- Not installed with no install.json (Linux) or not installed on macOS/Windows: hint (Linux) / silent 0 (mcp-stdio: hint + exit 1); trust never runs.
- Installed, or Linux with install.json present: trust over the full set (trustSet unchanged: install.json always a member; missing -> TrustError exit 1) before any packageRoot code loads. Linux install.json present but invalid ({} / packageRoot gone): trust first (deleted packageRoot -> TrustError exit 1 per ADR-004 "invalid pointer"), then hint if merely not installed.
- lib/trust.js unchanged (still self-contained).

Files modified: packages/unimatrix/lib/shim-entry.js, lib/hook-route.js, test/shim-entry.test.js (install.json-missing test rewritten, plus credential+missing install.json -> trust exit 1), test/trust.test.js (trustSet requires install.json). Created: test/fire-time-parity.test.js (shim-entry vs `bin/unimatrix.js hook`: not-a-project, project+no-install, installed ok, packageRoot deleted, tampered; asserts same exit/stdout/stderr).
Tests: 167 pass, 0 fail over shim-entry, launcher-hook, launcher-status, trust, trust-selfcontained, project-parity, fire-time-parity, cli-routing, shim.
Issues: (1) Not a literal ADR-text conflict: ADR-004 prescribes no trust-vs-installed order. Decision-first is used (read-only, loads no package code). (2) Pure "trust only when installed" would turn a deleted packageRoot into a silent hint (isInstalled requires isDir(packageRoot)); hence the Linux install.json-present trust rule. (3) Tampered install.json that becomes invalid JSON yields a hint after trust passes only if perms ok; trust still runs on it. (4) Parity test covers hook only on Linux host; darwin/win32 shim-entry parity not exercised.

## Knowledge Stewardship
- Queried: context_briefing, ADR-004/002 context_get.
- Stored: nothing novel to store -- cross-reference of existing #5904 (decide installed-ness before trust) suffices.
