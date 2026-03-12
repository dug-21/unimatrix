# Component 4: Agent Scan Algorithm — Pseudocode

## Input
- Working directory (implicit)

## Algorithm

```
FUNCTION agent_scan():
    agent_files = Glob(".claude/agents/**/*.md")

    IF agent_files is empty:
        PRINT "No agent files found at .claude/agents/. Skipping agent scan."
        RETURN

    results = []

    FOR EACH file IN agent_files:
        content = Read(file)
        agent_name = extract_filename_without_extension(file)

        missing = []
        suggestions = []

        // Check 1: context_briefing
        IF content does NOT contain "context_briefing":
            missing.append("context_briefing")
            suggestions.append("Add orientation section: call context_briefing at session start for relevant knowledge")

        // Check 2: outcome reporting
        IF content does NOT contain "/record-outcome" AND
           content does NOT contain "context_store" with "outcome":
            missing.append("outcome reporting")
            suggestions.append("Add session end: invoke /record-outcome to capture what was learned")

        // Check 3: unimatrix-* skill reference
        IF content does NOT contain "unimatrix-" (as skill reference):
            missing.append("unimatrix-* skills")
            suggestions.append("Reference /unimatrix-init and /unimatrix-seed for onboarding new repos")

        IF missing is empty:
            results.append({agent_name, "fully wired", "none"})
        ELSE:
            results.append({agent_name, missing, suggestions})

    // Print report
    PRINT "Agent Orientation Report"
    PRINT "========================"
    PRINT header row: "Agent | Missing | Suggested Addition"
    PRINT separator

    FOR EACH result IN results:
        PRINT row: result.agent_name | result.missing | result.suggestions

    IF all results have empty missing:
        PRINT "All agents fully wired."
```

## Output Format (Terminal Only)

```
Agent Orientation Report
========================
Agent                    | Missing                   | Suggested Addition
-------------------------|---------------------------|------------------------------------------
custom-dev               | context_briefing          | Add orientation section: call context_briefing at session start
my-reviewer              | outcome reporting         | Add session end: invoke /record-outcome
another-agent            | unimatrix-* skills        | Reference /unimatrix-init and /unimatrix-seed
well-wired-agent         | fully wired               | none
```

## Design Rules

- Read-only: NO file writes to agent files (C-07, NFR-07)
- Terminal-only output (ADR-004)
- Checks are simple string presence, not structural parsing
- Recommendations use skill-level examples (/record-outcome), not raw MCP tool calls
- If `.claude/agents/` directory does not exist, skip gracefully
