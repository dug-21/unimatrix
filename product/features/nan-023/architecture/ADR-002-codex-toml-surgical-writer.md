## ADR-002: Codex `.codex/config.toml` — Minimal In-House Surgical Block Writer, Not a Round-Trip TOML Library

### Context

Codex MCP config lives in `.codex/config.toml` as `[mcp_servers.unimatrix]` (TOML, snake_case;
`command` key = stdio, `url` key = HTTP). This is the JS installer's **first non-JSON writer** — all
current writers are JSON. AC-06/SR-01 make foreign-table survival a first-class acceptance assertion:
`[mcp_servers.*]` tables, other keys, comments, and formatting must survive **byte-for-byte**. This is
an open question in SCOPE ("library vs. minimal in-house writer").

Two options:

- **Option A — round-trip library** (`@iarna/toml`, `smol-toml`): parse the whole file to an object,
  mutate, re-serialize. Problem: full serialization **normalizes** — it does not preserve comments,
  blank lines, key order, inline vs. table style, or quoting of foreign content. It cannot guarantee
  byte-for-byte foreign-key preservation (the exact SR-01/SR-06-class regression). It also adds a new
  runtime dependency (supply-chain surface flagged in vnc-049/ass-106; install-footprint sensitivity).
- **Option B — minimal in-house surgical block writer**: treat the file as text; locate, upsert, or
  read **only** the single owned table `[mcp_servers.unimatrix]`; leave every other byte untouched.

### Decision

**Option B.** `lib/codex-install.js` owns a minimal, dependency-free surgical TOML block writer that
manages exactly one table — the one Unimatrix owns.

- `readTomlTable(raw, "mcp_servers.unimatrix") -> {present, body}` — finds the region from the header
  line `[mcp_servers.unimatrix]` to the next top-level `[` (a line whose first non-whitespace char is
  `[`) or EOF.
- `upsertTomlTable(raw, "mcp_servers.unimatrix", bodyLines) -> {text, changed}` — if absent, append
  the table (preceded by exactly one blank-line separator if the file is non-empty and does not end in
  one); if present, replace **only** that region's body; if the desired body already equals the current
  body, return `changed:false` (idempotence, AC-04). Every byte outside the owned region is copied
  verbatim — foreign `[mcp_servers.*]` tables, comments, and formatting are preserved by construction
  (AC-06).
- Fail-safe: unreadable/absent file handled as empty; a file the region-scanner cannot parse
  unambiguously (e.g. no clear table boundary) → `skipped-malformed`, file preserved, leg skipped
  (AC-15) — never a partial write.
- Body content: local → `command = "<absolute binaryPath>"`; cloud → `command`/`args` mirroring the
  token-free `.mcp.json` bridge shape (`command="node"`, `args=["<mcp-bridge.js>","<hash>"]`), or a
  `url = "<mcpUrl>"` entry — resolved per ARCHITECTURE open question #1. String values are TOML-escaped.

Scope guard: the writer only ever manages the **one** table it owns. It is not a general TOML editor;
it never rewrites or reorders foreign tables, so the normalization risk of Option A is structurally
absent.

### Consequences

Easier: byte-for-byte foreign-key/comment preservation is guaranteed by construction, not hoped for
(SR-01); zero new dependency (no supply-chain/install-footprint cost); the same "mutate minimally"
discipline as `opencode-install.js`, so the mental model is consistent; idempotence is a body-equality
check. Harder: table-boundary detection must be correct for nested/quoted edge cases (dotted keys,
`[mcp_servers.unimatrix]` vs a stray `[mcp_servers]` parent, CRLF, a table header inside a multi-line
string) — these are the acceptance/edge inventory the tester must enumerate (mirrors the parity-corpus
edge discipline, Unimatrix #4751); the writer reads/writes only stdio-vs-HTTP command shapes it knows,
so a future codex MCP schema change touches this one module. If a genuinely malformed TOML file is
encountered, the leg is skipped loudly rather than risking corruption. Cross-refs ADR-001 (writer
returns a `WireLeg`), ADR-003 (codex hooks are separate, JSON).
