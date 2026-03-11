# Security Review: vnc-011-security-reviewer

## Risk Level: low

## Summary
The vnc-011 changes add a markdown formatter as a pure function consuming an in-memory struct, plus a new `format` parameter on `RetrospectiveParams` with handler dispatch logic. No new external inputs beyond the validated `format` string, no new dependencies, no file I/O, no deserialization of untrusted data. The change is well-scoped to the formatting layer. Two low-severity informational findings; no blocking issues.

## Findings

### Finding 1: format parameter not validated in validate_retrospective_params
- **Severity**: low
- **Location**: crates/unimatrix-server/src/infra/validation.rs:345-353
- **Description**: The `validate_retrospective_params` function was not updated to validate the new `format` field. However, this is not a real gap because: (a) the format string is validated at the handler dispatch level via an explicit match arm that returns `ERROR_INVALID_PARAMS` for unrecognized values, and (b) the format string is never interpolated into commands, SQL, or file paths -- it is only compared against string literals. The error message does include the user-supplied format string in its text, but this goes back via MCP protocol as structured error data, not to a browser or template engine, so there is no injection risk.
- **Recommendation**: Informational only. Consider adding format validation to `validate_retrospective_params` for consistency with the validation-at-boundary pattern, but this is not a security concern.
- **Blocking**: no

### Finding 2: Large diff includes ~60% formatting-only changes
- **Severity**: low
- **Location**: crates/unimatrix-server/src/mcp/response/mod.rs (throughout)
- **Description**: The majority of changes in `response/mod.rs` are rustfmt reformatting (import reordering, line breaks on assert macros, struct literal formatting). While not a security concern, this pattern increases the risk of a behavioral change hiding in formatting noise. I verified the full diff -- all changes in `mod.rs` are purely cosmetic except for the new `mod retrospective` declaration and `pub use retrospective::format_retrospective_markdown` re-export, both correctly gated behind `#[cfg(feature = "mcp-briefing")]`.
- **Recommendation**: Informational. The behavioral changes are confined to: (1) new `retrospective.rs` module, (2) `format` field on `RetrospectiveParams`, (3) dispatch logic in `tools.rs`. All verified clean.
- **Blocking**: no

### Finding 3: User-supplied format value echoed in error message
- **Severity**: low
- **Location**: crates/unimatrix-server/src/mcp/tools.rs:1492-1498 (approximate)
- **Description**: When an unknown format value is provided, the error message includes the raw user input: `format!("Unknown format '{}'. Valid values: ...", format)`. In a web context this could be an XSS vector, but in the MCP stdio transport context, this string is returned as structured error data to the calling LLM agent. No browser rendering, no HTML, no template injection risk. The format value originates from serde JSON deserialization of MCP tool parameters, which already constrains it to a valid JSON string.
- **Recommendation**: No action needed. The echo pattern is standard for developer-facing error messages in MCP tools.
- **Blocking**: no

## Blast Radius Assessment

**Worst case if the fix has a subtle bug**: The markdown formatter produces incorrect or misleading retrospective output (wrong severity ordering, missing findings, incorrect event counts). Impact is limited to informational analytics output consumed by LLM agents. No data corruption, no privilege escalation, no denial of service. The JSON path is explicitly preserved unchanged -- any bug in the formatter cannot affect existing JSON consumers since the dispatch uses separate code paths.

**Failure mode**: Safe. The formatter is a pure function (no side effects, no I/O, no state mutation). If it panics (e.g., on an unexpected None), the MCP server catches the panic at the tokio task boundary and returns an error to the caller. The server remains operational.

**Default format change**: The default output format changes from JSON to markdown. This is an intentional behavioral change. Existing callers that relied on the default format being JSON will now receive markdown. However, the implementation brief and issue confirm this is by design, and the integration tests explicitly pass `format="json"` for all pre-existing retrospective tests, confirming they are not affected.

## Regression Risk

1. **Default format change**: Callers that previously received JSON by default now receive markdown. This is intentional per the design. All existing integration tests were already passing `format="json"` explicitly, so they are unaffected. Risk: low (by design, tested).

2. **evidence_limit on JSON path**: The JSON path preserves `unwrap_or(3)` exactly as before. The markdown path ignores `evidence_limit` entirely. No regression on existing behavior. Risk: none.

3. **Cached report path**: The cached report path in `context_retrospective` also routes through the new format dispatch. This is correct -- cached reports should respect the format parameter. Risk: low (tested).

## OWASP Assessment

| Check | Assessment |
|-------|-----------|
| Input validation | `format` parameter validated via match arm; unrecognized values return error. No SQL, shell, or path operations. |
| Path traversal | No file path operations in any changed code. |
| Injection | No shell commands, SQL, or template strings. Format string in error message is not an injection vector in MCP context. |
| Deserialization | No new deserialization of untrusted data. `RetrospectiveParams` uses serde with `Option<String>` -- safe. |
| Error handling | Errors return structured `ErrorData` via MCP protocol. No panics in production paths (all `write!` results are discarded with `let _ =`, which is correct for `String` writes that cannot fail). |
| Access control | Unchanged. Existing identity resolution and capability checks preserved. |
| Dependencies | No new crate dependencies. No Cargo.toml changes. |
| Secrets | No hardcoded secrets, API keys, or credentials in any changed file. |

## PR Comments
- Posted 1 approval comment on PR #197
- Blocking findings: no
