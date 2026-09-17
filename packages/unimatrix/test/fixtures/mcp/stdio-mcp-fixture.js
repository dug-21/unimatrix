#!/usr/bin/env node
"use strict";

// Minimal, real stdio MCP server fixture (nan-023 C14 verifier, ADR-006).
//
// Purpose: a *spawnable* newline-delimited JSON-RPC-over-stdio MCP server the
// C14 verifier can execute as the target of a wired `manifest[].entry` when the
// production Rust binary / cloud bridge are unavailable in CI. It stands in for
// the real MCP server ONLY as an execution target — the verifier still spawns
// `entry.command`+`entry.args` VERBATIM (non-tautology, SR-09/R-02); this file
// never learns what the manifest says. It answers a `context_*` tool call with a
// NON-EMPTY return so the retrieval-returns assertion is behavioral, not a
// config-presence proxy.
//
// Framing is byte-for-byte the production stdio framer (newline-delimited JSON),
// so the SAME verifier client drives a real `mcp-bridge.js` unchanged in 3c.
// It ignores argv, so it serves both the local shape (`command=<binary>`) and
// the cloud bridge shape (`command="node", args=[bridge, hash]`).

const { StdioFramer } = require("../../../lib/hook-client/mcp-bridge/stdio-frame.js");

const framer = new StdioFramer(process.stdin, process.stdout);

function ok(id, result) {
  return { jsonrpc: "2.0", id: id, result: result };
}

framer.onMessage((msg) => {
  if (!msg || typeof msg !== "object") return;
  // Notifications (no id, e.g. notifications/initialized) get no response.
  if (msg.id === undefined || msg.id === null) return;

  const method = msg.method;
  if (method === "initialize") {
    framer.write(
      ok(msg.id, {
        protocolVersion: "2025-06-18",
        capabilities: { tools: {} },
        serverInfo: { name: "unimatrix", version: "c14-stdio-fixture" },
      })
    );
    return;
  }
  if (method === "tools/list") {
    framer.write(
      ok(msg.id, {
        tools: [
          { name: "context_status" },
          { name: "context_search" },
          { name: "context_get" },
        ],
      })
    );
    return;
  }
  if (method === "tools/call") {
    const name = msg.params && msg.params.name;
    if (typeof name === "string" && name.indexOf("context_") !== -1) {
      // A non-empty RETURN — this is what discharges AC-10 (not config presence).
      framer.write(
        ok(msg.id, {
          content: [
            { type: "text", text: "C14 stdio fixture: " + name + " returned" },
          ],
          isError: false,
        })
      );
      return;
    }
    framer.write(ok(msg.id, { content: [], isError: true }));
    return;
  }
  // Unknown method — respond so the client never hangs.
  framer.write({
    jsonrpc: "2.0",
    id: msg.id,
    error: { code: -32601, message: "method not found: " + String(method) },
  });
});

// Exit cleanly when the verifier closes stdin (teardown).
process.stdin.on("end", () => process.exit(0));
process.stdin.on("error", () => process.exit(0));
