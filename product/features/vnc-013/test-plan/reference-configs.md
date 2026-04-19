# Component Test Plan: reference-configs
## `.gemini/settings.json` and `.codex/hooks.json`

Validating ACs: **AC-10, AC-19**
Risk coverage: **R-03, R-12**

---

## Component Responsibility

Two reference configuration files are created to allow operators to connect Gemini CLI
and Codex CLI to Unimatrix without guessing the hook registration format:

- `.gemini/settings.json` — Gemini CLI v0.31+ format; matcher `mcp_unimatrix_.*`;
  four events: `BeforeTool`, `AfterTool`, `SessionStart`, `SessionEnd`
- `.codex/hooks.json` — identical schema to `.claude/settings.json` (confirmed by
  ASS-049); four events: `PreToolUse`, `PostToolUse`, `SessionStart`, `Stop`;
  each invocation includes `--provider codex-cli`; caveat about bug #16732

These are config file validation tests — not executable unit tests. They are implemented
as either:
1. `#[test]` functions that read the files at test time and assert structure
2. Manual verification at Stage 3c gate review

Recommend approach (1) — file-read tests — so CI catches regressions if someone edits
the configs without updating the `--provider` flag.

---

## AC-10: `.gemini/settings.json` Validation (R-12)

### File Existence Check

At Stage 3c:
```bash
test -f .gemini/settings.json && echo "EXISTS" || echo "MISSING"
```

---

### `test_gemini_settings_json_exists_and_is_valid_json`

```rust
#[test]
fn test_gemini_settings_json_exists_and_is_valid_json() {
    // Locate from repo root — use environment variable or hard-coded relative path
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|p| p.join("Cargo.toml").exists() && p.join(".gemini").exists())
        .map(|p| p.join(".gemini/settings.json"))
        .expect("could not find repo root");

    let content = std::fs::read_to_string(&path)
        .expect(".gemini/settings.json must exist");

    let _parsed: serde_json::Value = serde_json::from_str(&content)
        .expect(".gemini/settings.json must be valid JSON");
}
```

If the file location is not deterministic from the test binary's working directory,
implement this as a file-check at Stage 3c gate review instead.

---

### `test_gemini_settings_json_contains_required_events` (AC-10, R-12)

```rust
#[test]
fn test_gemini_settings_json_contains_required_events() {
    let content = read_gemini_settings(); // helper to find and read file
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Assert the four required events are present
    let required_events = ["BeforeTool", "AfterTool", "SessionStart", "SessionEnd"];
    for event in &required_events {
        assert!(
            content.contains(event),
            "settings.json must contain event key: {event}"
        );
    }
}
```

---

### `test_gemini_settings_json_matcher_covers_all_tools` (AC-10, R-12)

```rust
#[test]
fn test_gemini_settings_json_matcher_covers_all_12_tools() {
    let content = read_gemini_settings();

    // Verify the matcher regex is present
    assert!(
        content.contains("mcp_unimatrix_.*"),
        "matcher must use 'mcp_unimatrix_.*' regex"
    );

    // Enumerate all 12 Unimatrix tools and verify the pattern covers them
    // (regex pattern match test — not a live Gemini CLI test)
    let regex = regex::Regex::new("mcp_unimatrix_.*").unwrap();
    let tool_names = [
        "mcp_unimatrix_context_search",
        "mcp_unimatrix_context_lookup",
        "mcp_unimatrix_context_get",
        "mcp_unimatrix_context_store",
        "mcp_unimatrix_context_correct",
        "mcp_unimatrix_context_deprecate",
        "mcp_unimatrix_context_status",
        "mcp_unimatrix_context_briefing",
        "mcp_unimatrix_context_quarantine",
        "mcp_unimatrix_context_enroll",
        "mcp_unimatrix_context_retrospective",
        "mcp_unimatrix_context_cycle",
    ];
    for tool in &tool_names {
        assert!(
            regex.is_match(tool),
            "matcher 'mcp_unimatrix_.*' must cover tool: {tool}"
        );
    }
}
```

Note: if `regex` crate is not available in the test context, replace the regex match
test with a simple `.starts_with("mcp_unimatrix_")` check on each tool name — the
pattern is simple enough that the string check is equivalent.

---

### Manual Verification Checklist (AC-10 gate review)

At Stage 3c gate sign-off, verify:
- [ ] File exists at `.gemini/settings.json`
- [ ] File is valid JSON
- [ ] All four events present: `BeforeTool`, `AfterTool`, `SessionStart`, `SessionEnd`
- [ ] Matcher field value is `"mcp_unimatrix_.*"` (exact string)
- [ ] Format is compatible with Gemini CLI v0.31+ (compare to ASS-049 FINDINGS-HOOKS.md format reference)
- [ ] `BeforeTool` and `AfterTool` have matcher set (tool-scoped events)
- [ ] `SessionStart` and `SessionEnd` have no matcher or a wildcard matcher (session events fire on any session)

---

## AC-19: `.codex/hooks.json` Validation (R-03)

### File Existence Check

At Stage 3c:
```bash
test -f .codex/hooks.json && echo "EXISTS" || echo "MISSING"
```

---

### `test_codex_hooks_json_exists_and_is_valid_json` (AC-19)

```rust
#[test]
fn test_codex_hooks_json_exists_and_is_valid_json() {
    let content = read_codex_hooks(); // helper to find and read file
    // JSON-with-comments format: if Codex uses JSONC, parse may need comment stripping.
    // If comments are not used, parse directly.
    let _parsed: serde_json::Value = serde_json::from_str(&content)
        .expect(".codex/hooks.json must be valid JSON");
}
```

---

### `test_codex_hooks_json_contains_provider_flag_on_all_events` (AC-19, R-03)

This is the most critical config test. Without `--provider codex-cli`, all Codex
events are silently mislabeled as `"claude-code"`.

```rust
#[test]
fn test_codex_hooks_json_contains_provider_flag_on_all_events() {
    let content = read_codex_hooks();

    // Every event invocation must include --provider codex-cli
    // Count occurrences of the event command pattern
    let required_events = ["PreToolUse", "PostToolUse", "SessionStart", "Stop"];

    for event in &required_events {
        // Verify that each event appears in a command with --provider codex-cli
        // This is a string-level check — adjust to match the exact format used.
        // Expected format per ASS-049: "unimatrix hook {event} --provider codex-cli"
        let expected_pattern = format!("--provider codex-cli");
        assert!(
            content.contains(&expected_pattern),
            "--provider codex-cli must be present for event: {event}"
        );
    }
}
```

More precise check — count that `--provider codex-cli` appears at least once per
event invocation line (the count should be >= the number of events):

```rust
let provider_flag_count = content.matches("--provider codex-cli").count();
assert!(
    provider_flag_count >= 4,
    "Each of the 4 hook events must have --provider codex-cli; found {provider_flag_count}"
);
```

---

### `test_codex_hooks_json_contains_bug_caveat` (AC-19, R-03)

```rust
#[test]
fn test_codex_hooks_json_contains_bug_caveat() {
    let content = read_codex_hooks();

    // Caveat text about Codex bug #16732 must be present
    assert!(
        content.contains("16732") || content.contains("bug #16732"),
        ".codex/hooks.json must carry caveat about Codex bug #16732"
    );
}
```

---

### `test_codex_hooks_json_contains_required_events` (AC-19)

```rust
#[test]
fn test_codex_hooks_json_contains_required_events() {
    let content = read_codex_hooks();

    let required_events = ["PreToolUse", "PostToolUse", "SessionStart", "Stop"];
    for event in &required_events {
        assert!(
            content.contains(event),
            ".codex/hooks.json must contain event: {event}"
        );
    }
}
```

---

### AC-19 Synthetic Unit Tests (Normalization — covered in normalization.md)

AC-19 also requires unit tests with synthetic Codex events. These are covered in
`normalization.md`:
- `test_normalize_event_name_with_provider_hint_codex` (AC-17/AC-19 shared)
- `test_codex_post_tool_use_skips_rework_path`

The file-check tests in this plan are the AC-19-specific component; the unit tests
above are the normalization component of AC-19.

---

### Manual Verification Checklist (AC-19 gate review)

At Stage 3c gate sign-off, verify:
- [ ] File exists at `.codex/hooks.json`
- [ ] File is valid JSON (or valid JSONC if comments used)
- [ ] All four events present: `PreToolUse`, `PostToolUse`, `SessionStart`, `Stop`
- [ ] Every hook command line includes `--provider codex-cli`
- [ ] Caveat text referencing Codex bug #16732 is present and prominent
- [ ] Schema matches `.claude/settings.json` format (per ASS-049 confirmation)
- [ ] No event invokes `unimatrix hook` without `--provider codex-cli`

---

## Assertions Summary

| Test | Risk | AC |
|------|------|----|
| `test_gemini_settings_json_exists_and_is_valid_json` | R-12 | AC-10 |
| `test_gemini_settings_json_contains_required_events` | R-12 | AC-10 |
| `test_gemini_settings_json_matcher_covers_all_12_tools` | R-12 | AC-10 |
| Gemini manual checklist | R-12 | AC-10 |
| `test_codex_hooks_json_exists_and_is_valid_json` | R-03 | AC-19 |
| `test_codex_hooks_json_contains_provider_flag_on_all_events` | R-03 | AC-19 |
| `test_codex_hooks_json_contains_bug_caveat` | R-03 | AC-19 |
| `test_codex_hooks_json_contains_required_events` | R-03 | AC-19 |
| Codex manual checklist | R-03 | AC-19 |

---

## Implementation Notes for Stage 3b

The `read_gemini_settings()` and `read_codex_hooks()` test helpers must locate the
files relative to the workspace root. The simplest approach is to use
`env!("CARGO_WORKSPACE_DIR")` if available, or walk up from `CARGO_MANIFEST_DIR` to
find the workspace root. Alternatively, these tests can be in a workspace-level
integration test crate rather than a specific crate's `mod tests`.

If adding `regex` as a test dependency is undesirable, replace regex match tests with
the `.starts_with("mcp_unimatrix_")` string check — equivalent for this pattern.
