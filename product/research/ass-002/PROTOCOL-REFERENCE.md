# MCP Protocol Reference (Technical)

Quick-reference for implementors. Full spec: https://modelcontextprotocol.io/specification/2025-11-25

---

## Wire Format

JSON-RPC 2.0 over newline-delimited stdio. Each message is one line (no embedded newlines).

### Request
```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_search","arguments":{"query":"auth pattern"}}}
```

### Response (Success)
```json
{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"..."}]}}
```

### Response (Tool Error)
```json
{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"No project initialized"}],"isError":true}}
```

### Response (Protocol Error)
```json
{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}
```

### Notification (No Response Expected)
```json
{"jsonrpc":"2.0","method":"notifications/initialized"}
```

---

## Initialization Sequence

### 1. Client → Server: initialize
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2025-11-25",
    "capabilities": {
      "roots": { "listChanged": true },
      "sampling": {}
    },
    "clientInfo": {
      "name": "claude-code",
      "version": "1.x.x"
    }
  }
}
```

### 2. Server → Client: initialize response
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2025-11-25",
    "capabilities": {
      "tools": { "listChanged": true },
      "resources": { "subscribe": true, "listChanged": true },
      "prompts": { "listChanged": true },
      "logging": {}
    },
    "serverInfo": {
      "name": "unimatrix",
      "version": "0.1.0"
    },
    "instructions": "Unimatrix provides project memory. Search memory before starting tasks to find existing patterns, conventions, and decisions."
  }
}
```

### 3. Client → Server: initialized notification
```json
{"jsonrpc":"2.0","method":"notifications/initialized"}
```

---

## Tool Operations

### tools/list
```json
// Request
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}

// Response
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "tools": [
      {
        "name": "memory_search",
        "title": "Search Memory",
        "description": "Search project memory for relevant knowledge...",
        "inputSchema": {
          "type": "object",
          "properties": {
            "query": { "type": "string", "description": "Natural language search query" },
            "k": { "type": "integer", "description": "Max results (default: 10)" }
          },
          "required": ["query"]
        },
        "annotations": {
          "readOnlyHint": true,
          "destructiveHint": false,
          "openWorldHint": false
        }
      }
    ]
  }
}
```

### tools/call
```json
// Request
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "memory_search",
    "arguments": { "query": "authentication pattern", "k": 5 }
  }
}

// Success Response
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "## Search Results\n\n### 1. JWT Authentication Pattern (0.94)\nUse jsonwebtoken crate with RS256..."
      }
    ],
    "isError": false
  }
}

// Error Response (application level)
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "No project initialized. Call memory_init to set up project memory first."
      }
    ],
    "isError": true
  }
}
```

---

## Resource Operations

### resources/list
```json
// Request
{"jsonrpc":"2.0","id":4,"method":"resources/list","params":{}}

// Response
{
  "jsonrpc": "2.0",
  "id": 4,
  "result": {
    "resources": [
      {
        "uri": "memory://conventions",
        "name": "Project Conventions",
        "description": "Active coding conventions for this project",
        "mimeType": "text/markdown"
      }
    ]
  }
}
```

### resources/read
```json
// Request
{"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":"memory://conventions"}}

// Response
{
  "jsonrpc": "2.0",
  "id": 5,
  "result": {
    "contents": [
      {
        "uri": "memory://conventions",
        "mimeType": "text/markdown",
        "text": "## Project Conventions\n\n- Use snake_case for functions\n- Error handling via anyhow..."
      }
    ]
  }
}
```

---

## Prompt Operations

### prompts/list
```json
// Request
{"jsonrpc":"2.0","id":6,"method":"prompts/list","params":{}}

// Response
{
  "jsonrpc": "2.0",
  "id": 6,
  "result": {
    "prompts": [
      {
        "name": "recall",
        "description": "Search project memory with guided filters",
        "arguments": [
          { "name": "topic", "description": "What to search for", "required": true }
        ]
      }
    ]
  }
}
```

### prompts/get
```json
// Request
{"jsonrpc":"2.0","id":7,"method":"prompts/get","params":{"name":"recall","arguments":{"topic":"auth"}}}

// Response
{
  "jsonrpc": "2.0",
  "id": 7,
  "result": {
    "description": "Search project memory",
    "messages": [
      {
        "role": "user",
        "content": {
          "type": "text",
          "text": "Search Unimatrix memory for everything related to 'auth'. Include conventions, decisions, and patterns. Summarize what you find."
        }
      }
    ]
  }
}
```

---

## Notifications

### Server → Client: Tool list changed
```json
{"jsonrpc":"2.0","method":"notifications/tools/list_changed"}
```

### Server → Client: Resource updated
```json
{"jsonrpc":"2.0","method":"notifications/resources/updated","params":{"uri":"memory://conventions"}}
```

### Server → Client: Log message
```json
{
  "jsonrpc": "2.0",
  "method": "notifications/message",
  "params": {
    "level": "info",
    "logger": "unimatrix",
    "data": "Indexed 42 new memories"
  }
}
```

### Progress (Either Direction)
```json
{
  "jsonrpc": "2.0",
  "method": "notifications/progress",
  "params": {
    "progressToken": "import-123",
    "progress": 75,
    "total": 100,
    "message": "Importing memories: 75/100"
  }
}
```

---

## Error Codes

| Code | Name | Meaning |
|------|------|---------|
| -32700 | Parse Error | Invalid JSON |
| -32600 | Invalid Request | Not valid JSON-RPC |
| -32601 | Method Not Found | Unknown method |
| -32602 | Invalid Params | Bad parameters / unknown tool |
| -32603 | Internal Error | Server internal error |

---

## Capability Flags Reference

### Server Capabilities (what Unimatrix declares)

```json
{
  "tools": { "listChanged": true },
  "resources": { "subscribe": true, "listChanged": true },
  "prompts": { "listChanged": true },
  "logging": {}
}
```

### Client Capabilities (what Claude Code declares)

```json
{
  "roots": { "listChanged": true },
  "sampling": {},
  "elicitation": { "form": {}, "url": {} }
}
```
