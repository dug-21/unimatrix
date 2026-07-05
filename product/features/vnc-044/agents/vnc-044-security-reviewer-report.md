# Security Review: vnc-044-security-reviewer

## Risk Level: low

## Summary
vnc-044 splits `context_graph`'s `format` parameter into two axes (serialization + verbosity) with a lean node projection. Fresh-context review of the full `main...HEAD` diff, ARCHITECTURE.md, and RISK-TEST-STRATEGY.md found no security defects. The highest-risk surface — `content_preview` truncation of attacker-influenceable stored `content` — is implemented as a total function that cannot panic or emit invalid UTF-8. No blocking findings.

## Findings

### F-1: `content_preview` UTF-8 flooring — cleared (primary DoS surface)
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/mcp/response/verbosity.rs` (`content_preview`)
- **Description**: The design flagged this as the highest-blast-radius issue (request-triggered panic / DoS on multibyte content straddling byte 256, or invalid-UTF-8 emission). Implementation uses the safe codebase idiom: early return on `content.len() <= CONTENT_PREVIEW_BYTES` (byte compare), then `while end > 0 && !content.is_char_boundary(end) { end -= 1; }`, then `&content[..end]`. Because a Rust `&str` is always valid UTF-8 (byte 0 is a boundary; a boundary is at most 3 bytes back), the loop terminates and the slice index is always a valid char boundary. The function is total — no panic, no invalid UTF-8. `content_truncated` is the byte-length compare, decoupled from the flooring index (257B ASCII → `true` even though `end` floors to 256). Boundary and 2/3/4-byte multibyte-straddle tests present.
- **Recommendation**: none.
- **Blocking**: no

### F-2: `resolve_graph_output` input validation — cleared
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/mcp/graph_read.rs` (`resolve_graph_output`, `handle_graph`)
- **Description**: Untrusted `format`/`detail` strings are matched against a closed set (lowercased, exact); any other value returns `ERROR_INVALID_PARAMS`, never a panic. Resolution runs once at the top of `handle_graph` before mode dispatch, so `markdown` rejection, legacy `format=summary`+`detail` conflict, and bad-value rejection are uniform across all seven modes. Capability check (`require_cap`) runs upstream in `tools.rs` before `handle_graph`, so unauthenticated callers never reach the resolver and error strings leak no state pre-auth.
- **Recommendation**: none.
- **Blocking**: no

### F-3: Graph-local containment of shared types — confirmed
- **Severity**: informational
- **Location**: `graph_read_projection.rs`, `response/mod.rs`, `graph_read.rs`
- **Description**: `EntryRecord`/`EdgeRecord`/`ResponseFormat`/`parse_format` are untouched — no `skip_serializing_if` added; `NodeSummary` and the edge projection are distinct types / `serde_json::Value` builders. `verbosity` is added to `response/mod.rs` as `pub mod` with no re-export, so the shared response surface for non-graph tools is unchanged. `serialize_detail<T>` is bounded on `GraphSummaryProjection`, which `NeighborsResponse`/`PathResponse` do not implement — edge-only modes physically cannot route through the summary projection (compile-time guarantee they remain accept-and-ignore). `Detail::Full` serializes the original envelope, byte-identical to pre-vnc-044 output.
- **Recommendation**: none.
- **Blocking**: no

### OWASP pass
- **Injection**: none — output is `serde_json`-encoded, no SQL/command/template sink.
- **Access control**: unchanged; Read capability enforced upstream. Summary reveals strictly less than full — no new information-disclosure surface.
- **Deserialization**: `detail` is an additive `Option<String>` on the existing `GraphParams`; no new deserialization boundary.
- **Dependencies**: no `Cargo.toml`/`Cargo.lock` changes; no new dependencies.
- **Secrets**: none in the diff.

## Blast Radius Assessment
Bounded to `context_graph` output shape. The change is read-only serialization — no mutation, no path traversal, no deserialization of untrusted bytes, no injection sink. Worst case of a subtle bug is a malformed or over/under-verbose graph response, not data corruption, privilege escalation, or DoS (the one panic vector, `content_preview`, is proven total). The type-level bound on `serialize_detail` prevents edge-only modes from silently gaining projection behavior.

## Regression Risk
One accepted behavior change: the default `full`→`summary` flip, scoped to `context_graph` only and disclosed in the tool description (including divergence from tools not yet migrated). Failure mode is safe — a caller expecting full records gets a lean shape flagged by `content_truncated`/`content_preview`, restorable with `detail=full`; no silent data-corruption path. No other tool's default output changes. Shared serializers are untouched, so non-graph callers are unaffected.

## PR Comments
- Posted 1 review comment on PR #920 (state: COMMENTED).
- Blocking findings: no.

## Knowledge Stewardship
- Nothing novel to store — the char-boundary-floor / byte-length-truncated-flag pattern and the wire-enum blast-radius discipline are already captured in Unimatrix (referenced in RISK-TEST-STRATEGY.md as #3706, #4350, #4831, #3426, #3337). This PR is a correct application of existing knowledge, not a new anti-pattern. Security findings specific to this PR remain in the PR comment.
