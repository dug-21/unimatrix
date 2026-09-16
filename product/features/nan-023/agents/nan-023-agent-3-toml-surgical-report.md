# Agent Report — nan-023-agent-3-toml-surgical (Wave A / A1)

Role: uni-js-dev · Component: Surgical TOML helper (ADR-002)

## Scope delivered

Created `packages/unimatrix/lib/codex-install.js` containing **only** the Wave A
surgical TOML helpers, with a clearly marked Wave B export seam. Implemented the
integration-surface signatures exactly:

- `readTomlTable(raw, tablePath) -> { present, body, malformed? }`
- `upsertTomlTable(raw, tablePath, bodyLines) -> { text, changed, malformed? }`
- `tomlString(s) -> string` — TOML basic-string escaper (see Decision below)

Internals (module-private): `splitKeepEndings`, `stripLineEnding`,
`dominantLineEnding`, `endsWithBlankLine`, `updateTripleState`, `scanTables`,
`findOwned`, `ownedRegion`, `normalizeBody`, `bodyEquals`.

Design: pure string functions, no fs, no throw on normal input; foreign bytes
preserved by construction (region-slice + verbatim re-join, never parse/re-serialize
the whole file); idempotence is a normalized body-equality check; ambiguous files
(unterminated header / multi-line string) return `{malformed:true}` for the caller
to skip. Boundary scanner is multi-line-string aware (`"""`/`'''`) and treats an
unescaped `#` outside a string as comment-to-EOL. Owned header matched by exact
bracket-content equality so `[mcp_servers]` parent and `[mcp_servers.other]` sibling
never false-match.

## Results

1. **Files modified:**
   - `/workspaces/unimatrix/packages/unimatrix/lib/codex-install.js` (new, 268 lines)
   - `/workspaces/unimatrix/packages/unimatrix/test/codex-toml-surgical.test.js` (new)
2. **Tests:** `node --test test/codex-toml-surgical.test.js` → **27 pass / 0 fail**.
   Covers every test-plan row: readTomlTable present/absent/stop-at-next/parent-distinction;
   insert (blank-separator, empty-file, ending-newline off-by-one); update (region-only
   replace, idempotence, round-trip stabilization); R-04 foreign preservation (foreign
   tables + interleaved comments + non-alpha order, adjacent comments above/below, CRLF,
   `[` inside a multi-line string, parent/sibling non-match); R-11 malformed fail-safe
   (unterminated string/header preserved, garbage-never-throws); R-10 injection (metachar
   path escapes + re-parses via a minimal TOML basic-string read-back decoder, backslash/
   control/tab/newline round-trip, cloud args-array shape).
3. **Size gate:** `check-hook-client-size.js` → **OK** (stripped 105243/110000, raw
   191416/200000). `codex-install.js` lives in `lib/`, not `lib/hook-client/`, so it is
   not counted by the gate; gate unaffected. Zero-deps check OK; package.json /
   package-lock.json unchanged.
4. **Issues / blockers:** none.

## Decisions & flags for Wave B (codex-install.js writers)

- **`tomlString` implemented here, not in Wave B.** R-10 (injection quoting/escaping) is
  assigned to THIS component's test plan, but the escaper is where the protection lives.
  I implemented the pure TOML basic-string escaper `tomlString` in this module and
  exported it. **Wave B's `buildTomlBody` MUST build every string value through
  `tomlString`** — never naive-concatenate a raw path into `command = ...`. This is
  called out in a header comment and at the export seam. Slight scope extension beyond
  "the two helpers," justified because it keeps the security-critical escaper testable in
  Wave A and prevents Wave B from re-deriving an unescaped concatenation.
- **Export seam:** `module.exports` lists the Wave A helpers plus `OWNED_TABLE`, with a
  commented `Wave B export seam` marking where `maybeWireCodex`, `writeCodexMcpToml`,
  `writeCodexHooks` are added. Wave B extends this object without touching Wave A.
- **`readTomlTable` returns an extra `malformed:true` field** on scan error (pseudocode
  §readTomlTable). Wave B's `writeCodexMcpToml` should branch on `existing.malformed`
  before the intent gate, per codex-install.md.
- **Region boundary reminder for Wave B:** the owned region includes any trailing blank
  line and any comment that sits between the last owned key and the next `[` header — an
  update replaces the whole body, so a comment placed *below* the owned command line
  (inside the region) is part of the replaced body. Comments *above* the header, and all
  foreign tables, are preserved. Tests document this boundary.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-002 (#5764) and schema/idempotence
  decisions; no prior surgical-TOML pattern existed. `context_search(category:pattern)` for
  surgical/format-preserving TOML — no relevant prior.
- Stored: entry #5772 "Surgical TOML table writer: scan header boundaries by line, replace only
  the owned region as text" via `/uni-store-pattern`.
