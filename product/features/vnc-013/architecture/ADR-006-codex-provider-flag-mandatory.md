## ADR-006: `--provider codex-cli` Mandatory in Codex Reference Config

### Context

Codex CLI uses identical event names to Claude Code (`PreToolUse`, `PostToolUse`,
`SessionStart`, `Stop`). Without a `--provider` flag, `normalize_event_name()` cannot
distinguish Codex from Claude Code by event name alone — both share the same strings.
The fallback is `"claude-code"`, which correctly preserves backward compatibility for
existing Claude Code installations but produces incorrect `source_domain` attribution
on the write path for Codex events.

SR-01 from the risk assessment identifies this as a High/High risk: the fallback
silently mislabels Codex events, and there is no runtime signal that this has occurred.

Two approaches for the Codex reference config:

**Option A — Fallback documented, flag optional**: Codex config does not include
`--provider codex-cli`. Events are labeled `"claude-code"`. Document this as a known
limitation. Simple for operators; incorrect attribution.

**Option B — Flag mandatory in reference config**: Codex reference config includes
`--provider codex-cli` on every hook invocation line. The config comment explains
that the flag is required for correct `source_domain` attribution and that omitting it
degrades to `"claude-code"` labeling (normalization correctness unaffected; only
attribution is wrong). SR-01 risk is mitigated by the reference config making the flag
the obvious default.

Option B is chosen because Codex is a distinct client whose usage patterns and
behavioral signals should be attributed separately. Shipping a reference config that
produces wrong attribution by default is not acceptable for a reference document.

Note: Codex bug #16732 (MCP tool calls do not fire hooks) means the reference config
is non-functional for live MCP hooks until the upstream bug is resolved. Unit tests use
synthetic Codex events. The config must carry a caveat that live end-to-end testing is
blocked by #16732.

### Decision

The `.codex/hooks.json` reference configuration includes `--provider codex-cli` on
every hook command invocation. A prominent comment in the config explains:
1. The flag is mandatory for correct `source_domain` attribution.
2. Omitting the flag causes Codex events to be labeled `"claude-code"` (a known
   limitation documented in ARCHITECTURE.md).
3. Live MCP hook support is blocked by Codex bug #16732 — the config is non-functional
   until the upstream bug is resolved.

The `normalize_event_name()` function documents `"claude-code"` as the fallback for
shared event names when `provider_hint` is `None` — this is explicit and reviewed,
not accidental.

The `Hook` subcommand description in `main.rs` is updated to mention that
`--provider` is required for Codex CLI:

```
Hook {
    /// The hook event name (e.g., SessionStart, Stop, BeforeTool).
    event: String,

    /// Provider identity. Required for Codex CLI (codex-cli) to distinguish
    /// Codex events from Claude Code events. Defaults to "claude-code" inference
    /// for Claude Code; Gemini events (BeforeTool, AfterTool, SessionEnd) are
    /// inferred automatically without this flag.
    #[arg(long)]
    provider: Option<String>,
}
```

### Consequences

Easier: operators following the reference config get correct attribution from day one;
the flag makes provider identity an explicit configuration decision rather than a
hidden default; future providers with shared event names have a clear precedent for
how to disambiguate.

Harder: operators who manually configure Codex hooks without the reference config may
produce mislabeled records; this is a documentation/UX concern, not an architectural
one. The fallback behavior (label as `"claude-code"`) is safe from a data integrity
perspective — no crash, no data loss, only incorrect label.
