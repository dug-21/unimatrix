# Bug Investigation Report: infra-004-mcp-infra-investigator

## Bug Summary
The cross-tenant isolation gate (multi-tenant-isolation-smoke.sh, job
`multi-tenant-isolation-amd64`, run 28292626314, branch `feature/infra-004`)
returned **INFRA** on the MCP write leg against `ghcr.io/dug-21/unimatrix:latest-amd64`.
All observe writes (warmup + matrix A/B) passed and were durable; the FIRST
`/v1/arch-research/mcp` `context_store` `tools/call` was classified
`INFRA: MCP context_store ... JSON-RPC error / no result`. The smoke logged only
the classification line, not the raw JSON-RPC body.

## Root Cause Analysis — VERDICT: Hypothesis 3 (gate-script / probe defect), latent-flaky

**The MCP `context_store` write was rejected by an intentional, long-standing
production content-scan (PII) guard because the gate's marker contains a
phone-number-shaped digit string. The observe leg does not run content scanning,
so it accepted the same marker — which is exactly why observe passed and MCP failed.**

Reproduced the raw JSON-RPC body (the missing diagnostic) by running the **actual
gate script** against the byte-identical current-`main` image `ghcr.io/dug-21/unimatrix:latest-arm64`
(arm64 build of the same `latest` multi-arch manifest as the amd64 image under test;
branch is test-only, no `crates/` change). Captured response:

```json
{"jsonrpc":"2.0","id":2,
 "error":{"code":-32006,
   "message":"Content rejected: phone number detected (PhoneNumber detected). Remove the flagged content and retry."}}
```

Failing marker: `infra003-mcp-a-18530-1782573915`. The marker embeds the gate's
`RUN` nonce `$$-$(date +%s)` = `<pid>-<10-digit-epoch>`. The production phone
regex `(?:\+?1[\s.-]?)?\(?[2-9]\d{2}\)?[\s.-]?\d{3}[\s.-]?\d{4}`
(`crates/unimatrix-server/src/infra/scanning.rs:300-304`) matches the substring
**`530-1782573`** inside `...18530-1782573915`. The `-` between the pid and epoch
is an allowed phone separator (`[\s.-]`), so the digits straddle the hyphen.

Verified empirically:
- `infra003-mcp-a-18530-1782573915` → regex **MATCH** `530-1782573`
- `infra003-obs-a-18530-1782573915` → also MATCH (but observe doesn't scan → passed)
- a "rounder" epoch `...1782500000` → **no match**
- my hand-probe nonce `repro-...-12751` → **no match** (which is why my first manual
  probe of the identical JSON-RPC sequence SUCCEEDED: `Stored #1`)

This makes the gate **latent-flaky**: pass/fail depends on whether the random
`$$-<epoch>` happens to contain a `[2-9]\d{2}[\s.-]?\d{3}[\s.-]?\d{4}` substring.
That is why infra-003 GREENED earlier on `:783-smoke` — that run's nonce simply
didn't form a phone shape — not because of any image/code difference.

### Code Path Trace
- Gate: `multi-tenant-isolation-smoke.sh:derive_markers` (RUN=`$$-$(date +%s)`,
  builds `M_MCP_A=infra003-mcp-a-${RUN}`)
  → `run_cells` → `write_then_barrier mcp` → `isolation-probe-lib.sh:mcp_write`
  → `tools/call context_store {content:marker, topic:marker}`
- Server: rmcp `StreamableHttpService` → `McpAdapter` (`http/router.rs`)
  → `mcp/tools.rs` context_store handler (documented order
  `identity -> capability -> validation -> category -> scanning`,
  delegate at `tools.rs:851` "StoreService (scanning, embedding, dup-check, insert)")
  → `ContentScanner` (`infra/scanning.rs`) → PhoneNumber pattern hit
  → `ERROR_CONTENT_SCAN_REJECTED = -32006` (`error.rs:28`, message `error.rs:262`).
- Gate classification: `mcp_write` sees `"error"` in parsed SSE data → `infra_fail` (exit 2).

### Why It Fails
`context_store` runs the production content/PII scanner; the observe `RecordEvent`
ingestion path does not. The marker's epoch digits form a phone-number substring,
so only the MCP leg is rejected. The smoke correctly classifies a JSON-RPC `error`
as INFRA (non-verdict), so the gate exits 2 rather than RED/GREEN.

## Affected Files and Functions
| File | Function | Role in Bug |
|------|----------|-------------|
| product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh | `derive_markers` / `warmup_barrier` | Builds markers from `RUN=$$-$(date +%s)`; 10-digit epoch forms phone-shaped digits |
| product/test/infra-001/scripts/isolation-probe-lib.sh | `mcp_write` | Sends marker as `context_store` content/topic; sees JSON-RPC error → INFRA |
| crates/unimatrix-server/src/infra/scanning.rs:300-304 | PhoneNumber pattern | Production guard (intended) that matches the marker substring |
| crates/unimatrix-server/src/error.rs:28,262 | ERROR_CONTENT_SCAN_REJECTED | Maps rejection to JSON-RPC -32006 |
| crates/unimatrix-server/src/mcp/tools.rs:851 | context_store → StoreService | Runs scanning on the MCP write leg (observe does not) |

## Proposed Fix Approach (in the infra-003 gate scripts — NOT server code)
1. Make the gate's marker derivation **PII-shape-safe**, not just charset-safe.
   The MCP `context_store` surface enforces an *additional* acceptance rule
   (content scanning) the observe surface does not; the marker contract must
   satisfy BOTH surfaces. Note `-` is an allowed phone separator, so hyphenation
   does NOT help — the fix must prevent long digit runs from forming
   phone/SSN/email/apikey shapes.
2. Concretely: derive `RUN` so numeric runs cannot form a `[2-9]\d{2}…\d{3}…\d{4}`
   substring — e.g. base36/hex-encode the epoch (letter-dominant), or interleave a
   letter every ≤2 digits. Keep the existing `[a-z0-9-]` (R-12) and pairwise
   non-substring (R-18) invariants.
3. Add a marker self-check (alongside the existing R-12 charset and R-18
   non-substring guards in `assert_markers_distinct` / `warmup_barrier`) that
   rejects/regenerates any marker matching the production scanner patterns
   (phone, SSN, email, bearer/api-key). This makes the gate self-defending against
   the whole PII-collision class and removes the latent flakiness deterministically.

### Why This Fix
The server behavior is correct and intentional (content scanning has existed since
the v0.1 tool handlers, #12 — far predating the `:783-smoke` image and the
vnc-038→041 routing era). The defect is in the test's marker contract, which only
satisfied the observe surface's acceptance rule. Fixing markers is minimal, keeps
the load-bearing MCP-write probe, and converts a probabilistic flake into a
deterministic guarantee.

## Risk Assessment
- **Blast radius**: Marker derivation is shared by C3 observe, C4 MCP, C5/C6
  read-as-barrier, and warmup. A marker-format change must preserve charset +
  non-substring invariants and the `sqlite3` predicate matching (LIKE/equality).
- **Regression risk**: Low. No server change. Risk is limited to re-breaking the
  R-12/R-18 invariants or the read predicates if markers are reshaped carelessly —
  mitigated by the existing self-checks plus the new PII self-check.
- **Confidence**: **High.** Raw -32006 body captured; exact substring match proven;
  observe-vs-MCP scanning divergence confirmed; cold MCP write proven working when
  the marker is not phone-shaped.

## Hypotheses — disposition
- **H1 (cold MCP-path warmup gap, in-scope OQ-WB-1): REJECTED.** A cold first MCP
  `context_store` succeeded (`Stored #1`) when the marker was not phone-shaped.
  Extending the warmup barrier to the MCP surface would NOT fix this and would
  surface the same -32006 during warmup.
- **H2 (MCP write-path regression on current `main` vs `:783-smoke`): REJECTED.**
  Content scanning predates `:783-smoke` (#12); the served MCP write works on
  current `main`. No routing/contract regression.
- **H3 (infra-003 probe construction defect): CONFIRMED — refined.** Not the
  request shape/headers; the **marker value** trips the production PII scanner on
  the MCP leg only. Latent-flaky by epoch digits.

## SCOPE CLASSIFICATION
**OUT of scope for infra-004 (warmup-surface extension).** This is not a cold-path
or warmup-coverage gap; warming the MCP surface cannot fix a content-scan
rejection. It is an **infra-003 gate-script defect** (marker derivation) →
**file its own GH issue** against the infra-003 isolation gate. infra-004 may
proceed on its warmup-barrier work independently; this finding does not validate
or invalidate the warmup mechanism (the warmup leg is observe-only and passed).

## Missing Test
A deterministic check that the gate's derived markers (the four cell markers +
warmup marker) never match the production `ContentScanner` patterns
(phone/SSN/email/api-key). Implementable off-Docker via the existing `SMOKE_*`
stub seam or a tiny standalone assertion running the same regexes. This converts
the current probabilistic exposure into a deterministic, always-on guard and would
have caught the defect at gate-logic test time rather than on a CI dispatch.

## Reproduction Scenario
Deterministic given a phone-shaped nonce:
1. Boot `ghcr.io/dug-21/unimatrix:latest-{arch}`, register `arch-research` +
   `isolation-b`, restart (as the smoke does).
2. MCP handshake on `/v1/arch-research/mcp`; `tools/call context_store` with
   `content`/`topic` containing a `[2-9]\d{2}[\s.-]?\d{3}[\s.-]?\d{4}` substring
   (e.g. `infra003-mcp-a-18530-1782573915`) → JSON-RPC error -32006.
3. Same call with a non-phone-shaped marker (e.g. `...-repro-12751`) → `Stored #1`.
Underlying CI flakiness is governed by whether `$$-<epoch>` forms a phone substring.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-003 #5343 (bidirectional
  MCP-write probe / per-route session) and #5131 (rmcp session-setup logging);
  neither covered content-scan PII rejection of test markers.
- Stored: entry #5355 "Isolation-gate test markers must be PII-shape-safe: MCP
  context_store scans content, observe does not" via /uni-store-lesson (topic:
  testing; category: lesson-learned; tags incl. caused_by_feature:vnc-012). Note:
  the first store attempt was itself rejected by the same -32006 phone scanner
  because it quoted the offending digits — confirming the root cause in vivo.
- Per project rule "bugs are GH issues, not lessons," the concrete gate defect goes
  to a GH issue (infra-003 gate marker derivation); the stored lesson captures only
  the generalizable trap (MCP write leg PII-scans, observe leg does not).
