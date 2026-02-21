# MCP Server Response Format Patterns Research

## Executive Summary

This document analyzes response format patterns, tool description conventions, and design decisions across real-world MCP server implementations. Sources include the 7 official reference servers (modelcontextprotocol/servers), the GitHub MCP server (github/github-mcp-server), the official MCP specification, Anthropic's tool use documentation, and community discussion.

---

## 1. Official MCP Specification: Tool Response Types

The MCP specification (2025-03-26) defines the `CallToolResult` structure:

```json
{
  "content": [
    { "type": "text", "text": "Tool result text" }
  ],
  "isError": false
}
```

Four content types are supported:

| Type | Structure | Use Case |
|------|-----------|----------|
| **TextContent** | `{ "type": "text", "text": "..." }` | Primary format. Plain text, JSON, or markdown |
| **ImageContent** | `{ "type": "image", "data": "base64...", "mimeType": "image/png" }` | Visual data |
| **AudioContent** | `{ "type": "audio", "data": "base64...", "mimeType": "audio/wav" }` | Audio data |
| **EmbeddedResource** | `{ "type": "resource", "resource": { "uri": "...", "text": "..." } }` | Subscribable/refetchable data |

Two error mechanisms exist:
- **Protocol errors**: JSON-RPC errors (unknown tool, invalid args)
- **Tool execution errors**: `isError: true` in content (API failures, invalid input)

---

## 2. Response Format Patterns Across Official Reference Servers

### 2.1 Filesystem Server (TypeScript)

**Response pattern**: Plain text, human-readable formatting.

Every handler returns:
```typescript
{
  content: [{ type: "text" as const, text: formattedString }],
  structuredContent: { content: formattedString }
}
```

Specific formats by tool:

| Tool | Response Format |
|------|----------------|
| `read_text_file` | Raw file content as text |
| `read_multiple_files` | `"{path}:\n{content}\n---\n{path}:\n{content}"` -- files separated by `---` |
| `write_file` | `"Successfully wrote to {path}"` |
| `edit_file` | Git-style unified diff wrapped in markdown code fences |
| `list_directory` | `"[DIR] dirname\n[FILE] filename"` per line |
| `list_directory_with_sizes` | `"[DIR] name    \n[FILE] name    1.23 KB"` with summary |
| `directory_tree` | `JSON.stringify(treeData, null, 2)` -- recursive JSON structure |
| `search_files` | Full paths joined by newlines, or `"No matches found"` |
| `get_file_info` | `"key: value\nkey: value"` key-value pairs |
| `move_file` | `"Successfully moved {src} to {dest}"` |
| `create_directory` | `"Successfully created directory {path}"` |
| `list_allowed_directories` | `"Allowed directories:\n{dir1}\n{dir2}"` |

Key observations:
- **No JSON wrapping** for simple operations; plain text confirmation messages
- **JSON only** for inherently structured data (directory trees)
- **Git-style diffs** for edit results -- a domain-appropriate format
- `structuredContent` mirrors the text content for SDK compatibility

### 2.2 Memory Server (TypeScript)

**Response pattern**: JSON-serialized structured data.

Every handler returns:
```typescript
{
  content: [{ type: "text" as const, text: JSON.stringify(result, null, 2) }],
  structuredContent: { /* typed structured data */ }
}
```

Tool responses:

| Tool | Return Data |
|------|-------------|
| `create_entities` | JSON array of created Entity objects |
| `create_relations` | JSON array of created Relation objects |
| `add_observations` | JSON array of `{ entityName, addedObservations }` |
| `delete_entities` | `{ success: true, message: "Entities deleted successfully" }` |
| `delete_relations` | `{ success: true, message: "..." }` |
| `delete_observations` | `{ success: true, message: "..." }` |
| `read_graph` | `{ entities: [...], relations: [...] }` |
| `search_nodes` | Filtered `{ entities: [...], relations: [...] }` |
| `open_nodes` | Named entities + their inter-relations |

Key observations:
- **All responses are pretty-printed JSON** (`JSON.stringify(result, null, 2)`)
- Mutation operations return structured success/data objects, not plain text confirmations
- Search returns the same graph structure as full read, just filtered
- The `structuredContent` field provides typed schema alongside the text JSON

### 2.3 Fetch Server (Python)

**Response pattern**: Prefixed plain text with pagination metadata.

```python
[TextContent(type="text", text=f"{prefix}Contents of {url}:\n{content}")]
```

When content is truncated:
```
<error>Content truncated. Call the fetch tool with a start_index of {N} to get more content.</error>
```

Key observations:
- **Inline pagination guidance** embedded directly in the response text
- Uses `<error>` XML tags to communicate truncation status
- The tool description itself addresses the LLM's prior beliefs: "Although originally you did not have internet access...this tool now grants you internet access"

### 2.4 Git Server (Python)

**Response pattern**: Labeled plain text sections.

| Tool | Format |
|------|--------|
| `git_status` | `"Repository status:\n{output}"` |
| `git_diff_unstaged` | `"Unstaged changes:\n{diff}"` |
| `git_diff_staged` | `"Staged changes:\n{diff}"` |
| `git_commit` | `"Changes committed successfully with hash {sha}"` |
| `git_add` | `"Files staged successfully"` |
| `git_log` | `"Commit history:\n"` + formatted entries |
| `git_show` | Commit details + patch diff |

Key observations:
- **Labeled sections**: Each response starts with a human-readable label
- No JSON wrapping -- raw git output with contextual prefixes
- Success messages are concise and descriptive

### 2.5 Sequential Thinking Server (TypeScript)

**Response pattern**: JSON with both text and structured output.

Returns processed thought data with:
```typescript
{
  content: [{ type: "text", text: jsonString }],
  structuredContent: {
    thoughtNumber: number,
    totalThoughts: number,
    nextThoughtNeeded: boolean,
    branches: string[],
    thoughtHistoryLength: number
  }
}
```

Key observation: Uses **structured output schema** to enable programmatic consumption alongside text.

---

## 3. GitHub MCP Server (Go) -- Production Scale

The GitHub MCP server (github/github-mcp-server) provides 40+ tools organized into toolsets.

### Response formatting patterns:

```go
// Standard JSON response
utils.NewToolResultText(string(jsonBytes))

// Error response
utils.NewToolResultError(err.Error())

// Resource with content
utils.NewToolResultResource(message, result)

// Resource link (for large files)
utils.NewToolResultResourceLink(message, link)

// GitHub API error
ghErrors.NewGitHubAPIErrorResponse(ctx, message, resp, err)
```

### Key patterns:

1. **Minimal response objects**: Converts full API responses to minimal structures before serialization. For example, `convertToMinimalIssue`, `convertToMinimalCommit`, `convertToMinimalBranch` strip unnecessary fields.

2. **Paginated responses** include cursor metadata:
```json
{
  "issues": [...],
  "pageInfo": {
    "hasNextPage": true,
    "hasPreviousPage": false,
    "startCursor": "...",
    "endCursor": "..."
  },
  "totalCount": 42
}
```

3. **Action confirmations** return structured results, not just success strings:
```json
{
  "message": "successfully assigned copilot to issue...",
  "issue_number": 123,
  "issue_url": "https://...",
  "pull_request": { "number": 456, "url": "...", "title": "...", "state": "open" }
}
```

4. **Sanitization**: All output passes through `sanitize.Sanitize()` to prevent injection.

5. **Tool descriptions are brief**: Most descriptions are 1 sentence (e.g., "Get details for a commit from a GitHub repository"). This contrasts with Anthropic's recommendation for 3-4+ sentences.

---

## 4. Tool Description Patterns Analysis

### 4.1 Anthropic's Official Guidance

From Anthropic's tool use documentation:

> "Provide extremely detailed descriptions. This is by far the most important factor in tool performance."

Recommendations:
- Aim for **at least 3-4 sentences** per tool, more for complex tools
- Explain **what the tool does**
- Explain **when it should be used** (and when it shouldn't)
- Explain **what each parameter means**
- Note **important caveats or limitations**
- Describe **what information the tool returns**

**Good example** (from Anthropic docs):
```json
{
  "name": "get_stock_price",
  "description": "Retrieves the current stock price for a given ticker symbol. The ticker symbol must be a valid symbol for a publicly traded company on a major US stock exchange like NYSE or NASDAQ. The tool will return the latest trade price in USD. It should be used when the user asks about the current or most recent price of a specific stock. It will not provide any other information about the stock or company."
}
```

**Bad example**:
```json
{
  "name": "get_stock_price",
  "description": "Gets the stock price for a ticker."
}
```

### 4.2 Directive Language in Descriptions

The **Fetch server** uses the most notable directive language pattern:

> "Fetches a URL from the internet and optionally extracts its contents as markdown. Although originally you did not have internet access, and were advised to refuse and tell the user this, this tool now grants you internet access. Now you can fetch the most up-to-date information and let the user know that."

This description:
- Explicitly overrides the LLM's training-time knowledge
- Tells the LLM to change its behavior ("let the user know that")
- Addresses anticipated refusal patterns

The **PubMed MCP server** (available in this conversation) uses extensive directive language:

> "IMPORTANT: PubMed Database Scope: This server provides access to PubMed, which ONLY indexes biomedical and life sciences literature..."
> "PubMed does NOT contain papers from these fields (use other databases)..."
> "Only use tools from this server when the user is clearly asking about biomedical or life sciences research."

This pattern:
- Uses CAPS for emphasis ("IMPORTANT", "ONLY", "NOT")
- Defines scope boundaries explicitly
- Tells the LLM when NOT to use the tool
- Repeated identically across all tools in the server for reinforcement

The **Claude Code tools** (loaded in this conversation) demonstrate aggressive directive language:
- `Grep`: "ALWAYS use Grep for search tasks. NEVER invoke `grep` or `rg` as a Bash command."
- `Bash`: "IMPORTANT: This tool is for terminal operations... DO NOT use it for file operations"
- `WebFetch`: "IMPORTANT: WebFetch WILL FAIL for authenticated or private URLs. Before using this tool, check if the URL points to an authenticated service..."

### 4.3 Filesystem Server Description Style

The filesystem server uses **medium-length functional descriptions** (2-3 sentences):

```
"Read the complete contents of a file from the file system as text. Handles various text
encodings and provides detailed error messages if the file cannot be read. Use this tool
when you need to examine the contents of a single file."
```

```
"Get a detailed listing of all files and directories in a specified path. Results clearly
distinguish between files and directories with [FILE] and [DIR] prefixes. This tool is
essential for understanding directory structure and finding specific files within a directory."
```

Pattern: **What it does + How results look + When to use it**.

### 4.4 Description Length Spectrum

| Server | Avg Description Length | Style |
|--------|----------------------|-------|
| GitHub MCP (Go) | ~10 words | Minimal, imperative |
| Git server (Python) | ~8 words | Very terse |
| Memory server (TS) | ~12 words | Brief functional |
| Filesystem server (TS) | ~40 words | Medium, instructive |
| Fetch server (Python) | ~50 words | Directive, behavioral |
| PubMed MCP | ~100+ words | Extensive, scoping |
| Claude Code tools | ~100+ words | Aggressive directives |

### 4.5 Proactive Tool Use Triggers

Patterns that correlate with Claude using tools proactively:

1. **"Use this tool when..."** - Explicit triggering conditions
2. **"This tool is essential for..."** - Urgency/importance signaling
3. **"ALWAYS use X"** / **"NEVER use Y"** - Hard behavioral rules
4. **Scope negation** - "This tool does NOT..." helps avoid misuse
5. **Capability expansion** - "this tool now grants you..." overrides default behavior
6. **Context-dependent routing** - "Only use this when the user is clearly asking about..."

---

## 5. Response Format Decision Framework

### 5.1 When to Use Plain Text

- Simple success/failure confirmations: `"Successfully wrote to {path}"`
- Raw domain content: file contents, git diffs, command output
- Human-readable labels improve LLM comprehension: `"Repository status:\n{output}"`

**Advantages**: Token-efficient, natural for LLM consumption, no parsing overhead.

### 5.2 When to Use JSON

- Structured data with relationships: knowledge graphs, issue lists
- Data that will be referenced programmatically: IDs, URLs, counts
- Paginated results needing cursor metadata
- When `structuredContent` schema is defined for client applications

**Advantages**: Precise, parseable, supports structured output schemas.

### 5.3 When to Use Markdown

- Mixed content (text + tables + code blocks)
- Documentation or explanatory responses
- Git-style diffs (wrapped in code fences)

**Advantages**: Rich formatting, familiar to LLMs.

### 5.4 Community Consensus

From MCP community discussion (#529):
- **Text is the default** for most tool responses
- **JSON as TextContent** works well -- LLMs handle JSON in text blocks reliably
- **Consistency matters** -- mixing formats confuses the LLM about expected output format
- **Minimize response size** -- return only what's needed, not full API objects

---

## 6. Large Result Set Handling

| Strategy | Server | Implementation |
|----------|--------|----------------|
| **Cursor pagination** | GitHub MCP | `pageInfo.endCursor` in response |
| **Character-offset pagination** | Fetch | `start_index` parameter + truncation error message |
| **Response minimization** | GitHub MCP | `convertToMinimal*` converters strip unnecessary fields |
| **MCP protocol pagination** | Spec | `tools/list` supports cursor-based pagination |
| **No pagination** | Memory, Git | Returns everything (assumes small datasets) |

The fetch server's pagination pattern is notable -- it embeds pagination instructions directly in the error content:
```
<error>Content truncated. Call the fetch tool with a start_index of 5000 to get more content.</error>
```

This teaches the LLM how to paginate without requiring any system prompt guidance.

---

## 7. Error Response Patterns

### Simple text errors (most servers):
```typescript
{ content: [{ type: "text", text: "Error - file not found" }], isError: true }
```

### Structured error guidance (recommended):
```json
{
  "content": [{ "type": "text", "text": "Error: Invalid filter value 'xyz'. Valid options: 'open', 'closed', 'all'" }],
  "isError": true
}
```

### GitHub MCP pattern (rich errors):
```go
ghErrors.NewGitHubAPIErrorResponse(ctx, "Failed to get issue", resp, err)
// Includes HTTP status, error message, and context
```

Best practice from community: Include **suggested remedial actions** in error responses so the LLM can self-correct.

---

## 8. Rust MCP SDK Patterns

The official Rust SDK (`modelcontextprotocol/rust-sdk`) uses procedural macros for tool definition:

```rust
#[tool(description = "Calculate the sum of two numbers")]
fn sum(&self, Parameters(SumRequest { a, b }): Parameters<SumRequest>) -> String {
    (a + b).to_string()
}
```

Response types:
- Return `String` for automatic `TextContent` wrapping
- Return `Result<CallToolResult, McpError>` for explicit content blocks
- `CallToolResult` wraps `Content::text("...")` for text responses

The Rust pattern favors **returning plain strings** that the SDK wraps automatically.

---

## 9. Key Findings and Recommendations

### Finding 1: Plain text dominates for tool responses
Every reference server uses `TextContent` as the primary (usually only) content type. JSON is serialized into text strings, not returned as a separate content type.

### Finding 2: Dual-layer responses are the modern pattern
Recent TypeScript servers return both `content` (TextContent array) and `structuredContent` (typed object) for backward compatibility + programmatic access.

### Finding 3: Tool descriptions are the highest-leverage optimization
Anthropic states this is "by far the most important factor in tool performance." The gap between terse (GitHub MCP: ~10 words) and detailed (Claude Code: ~100+ words) descriptions is significant for tool selection accuracy.

### Finding 4: Directive language in descriptions works
Patterns like "ALWAYS", "NEVER", "Use this when...", "This does NOT..." demonstrably influence LLM tool selection behavior.

### Finding 5: Response minimization reduces token cost
The GitHub MCP server's pattern of converting full API responses to minimal structures (`convertToMinimal*`) before returning is a proven approach for large datasets.

### Finding 6: Inline pagination guidance is effective
The Fetch server's pattern of embedding pagination instructions in truncation error messages teaches the LLM to paginate without system prompt overhead.

### Finding 7: Error responses should enable self-correction
Including valid options, suggested actions, and context in error messages allows the LLM to retry intelligently.

---

## Sources

- [Official MCP Reference Servers](https://github.com/modelcontextprotocol/servers)
- [MCP Specification: Tools](https://modelcontextprotocol.io/specification/2025-03-26/server/tools)
- [GitHub MCP Server](https://github.com/github/github-mcp-server)
- [Anthropic: How to Implement Tool Use](https://platform.claude.com/docs/en/agents-and-tools/tool-use/implement-tool-use)
- [Anthropic: Advanced Tool Use](https://www.anthropic.com/engineering/advanced-tool-use)
- [MCP Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)
- [MCP Discussion: Natural Language vs JSON Responses (#529)](https://github.com/orgs/modelcontextprotocol/discussions/529)
- [MCP Discussion: Suggested Response Format (#315)](https://github.com/modelcontextprotocol/modelcontextprotocol/discussions/315)
- [MCP Server Best Practices](https://blog.codonomics.com/2025/08/best-practices-to-building-mcp-server.html)
- [MCP Example Servers](https://modelcontextprotocol.io/examples)
- [@modelcontextprotocol/sdk (npm)](https://www.npmjs.com/package/@modelcontextprotocol/sdk)
