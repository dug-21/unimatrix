"use strict";

// bridge-single-call-driver.js (#915/#916) — drive ONE stateless context_* tools/call
// THROUGH the SHIPPED `node mcp-bridge.js <projectHash>` over stdio JSON-RPC. This is
// the client-works proof (C15/#916): it proves the ATTACHED client can perform a REAL
// context_* op over the pinned-TLS bundle path — not merely that /health is up.
//
// It is a thin test helper (test tree only): it adds NO transport/cert/credstore code —
// it spawns the UNMODIFIED shipped bridge and speaks newline-delimited JSON-RPC to it
// (the framing the bridge's StdioFramer reads/writes). The bridge owns the pinned-HTTPS
// session, credstore read (keyed by the projectHash arg + HOME), Mcp-Session-Id
// capture/replay, and SSE parse; this driver re-implements NONE of that (reuse-as-is,
// pattern #5129 — never a native http MCP entry).
//
// Distinct from bridge-cycle-driver.js: that drives the full context_cycle parity
// workload (start -> tools -> stop -> review) and needs a manifest. This driver makes
// exactly ONE tools/call (default context_status — stateless, embed-free) so the fast
// posture smoke never perturbs the shared-lane gate-8 credstore/cycle nor eats the
// embed-retry window.
//
// Usage:
//   node bridge-single-call-driver.js <projectHash> <toolName> --bridge <path-to-mcp-bridge.js>
// Stdout: ONE json line: {ok, tool, error}. Diagnostics -> stderr.
// Exit: 0 on a non-error tool result; 1 otherwise. The SHELL gate wraps this whole
// process in `timeout` (bounded) so a bridge hang cannot eat the blocking lane's job
// timeout; the internal rpc timeouts below are defense-in-depth.

const { spawn } = require("child_process");
const fs = require("fs");
const path = require("path");

function emit(o) {
  process.stdout.write(JSON.stringify(o) + "\n");
}
function die(msg) {
  emit({ ok: false, tool: null, error: String(msg) });
  process.exit(1);
}

function parseArgs(argv) {
  const pos = [];
  const opt = {};
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--bridge") {
      opt.bridge = argv[++i];
    } else {
      pos.push(a);
    }
  }
  return { projectHash: pos[0], tool: pos[1], opt };
}

async function main() {
  const { projectHash, tool, opt } = parseArgs(process.argv);
  if (!projectHash || !tool) {
    die("usage: bridge-single-call-driver.js <projectHash> <toolName> --bridge <p>");
  }
  const bridgePath = opt.bridge;
  if (!bridgePath || !fs.existsSync(bridgePath)) die("bridge path missing: " + bridgePath);

  // ---- spawn the REAL bridge; HOME is inherited (the shell gate sets $SANDBOX/home) ----
  // so credstore.read finds THIS run's remote.json keyed by projectHash. The bridge's
  // own stderr (incl. #830 self-heal notices) is INHERITED to fd2, which the shell gate
  // captured to a file it tail-dumps ON FAILURE ONLY.
  const bridge = spawn(process.execPath, [path.resolve(bridgePath), projectHash], {
    stdio: ["pipe", "pipe", "inherit"],
    env: process.env,
  });

  let bridgeExitErr = null;
  bridge.on("error", (e) => { bridgeExitErr = e; });
  bridge.on("exit", (code, signal) => {
    if (code !== 0 && code !== null) bridgeExitErr = new Error("bridge exited code=" + code);
    else if (signal) bridgeExitErr = new Error("bridge killed signal=" + signal);
  });

  // ---- newline-delimited JSON-RPC over the bridge's stdio (StdioFramer) ----
  const pending = new Map(); // id -> {resolve,reject}
  let rbuf = "";
  bridge.stdout.setEncoding("utf8");
  bridge.stdout.on("data", (chunk) => {
    rbuf += chunk;
    let idx;
    while ((idx = rbuf.indexOf("\n")) !== -1) {
      const line = rbuf.slice(0, idx);
      rbuf = rbuf.slice(idx + 1);
      if (line.trim() === "") continue;
      let msg;
      try { msg = JSON.parse(line); } catch (_e) { continue; }
      const h = msg && msg.id != null ? pending.get(msg.id) : null;
      if (h) { pending.delete(msg.id); h.resolve(msg); }
    }
  });

  let nextId = 1;
  function send(obj) {
    bridge.stdin.write(JSON.stringify(obj) + "\n");
  }
  function rpc(buildEnvelope, timeoutMs) {
    const id = nextId++;
    return new Promise((resolve, reject) => {
      const t = setTimeout(() => {
        pending.delete(id);
        reject(new Error("timeout waiting for JSON-RPC id=" + id));
      }, timeoutMs || 30000);
      pending.set(id, {
        resolve: (m) => { clearTimeout(t); resolve(m); },
        reject: (e) => { clearTimeout(t); reject(e); },
      });
      send(buildEnvelope(id));
    });
  }
  function notify(obj) { send(obj); }

  function ensureOk(resp, what) {
    if (!resp) throw new Error(what + ": no response");
    if (resp.error) throw new Error(what + " failed: " + JSON.stringify(resp.error));
    // tools/call surfaces tool-level errors inside result.isError.
    const r = resp.result;
    if (r && r.isError) {
      const txt = r.content && r.content[0] && r.content[0].text ? r.content[0].text : "";
      throw new Error(what + " tool error: " + txt);
    }
    return resp;
  }

  try {
    const initResp = await rpc(
      (id) => ({
        jsonrpc: "2.0",
        id,
        method: "initialize",
        params: {
          protocolVersion: "2024-11-05",
          capabilities: {},
          clientInfo: { name: "claim-floor-single-call-driver", version: "1.0.0" },
        },
      }),
      45000
    );
    ensureOk(initResp, "initialize");
    notify({ jsonrpc: "2.0", method: "notifications/initialized" });

    // The ONE real context_* op — a non-error tool result is the client-works proof.
    ensureOk(
      await rpc((id) => ({
        jsonrpc: "2.0",
        id,
        method: "tools/call",
        params: { name: tool, arguments: {} },
      })),
      tool
    );

    bridge.stdin.end(); // EOF -> teardown DELETE -> bridge exit 0
    await new Promise((r) => setTimeout(r, 200));
    if (bridgeExitErr) throw bridgeExitErr;

    emit({ ok: true, tool, error: null });
    process.exit(0);
  } catch (e) {
    try { bridge.stdin.end(); } catch (_e) {}
    try { bridge.kill("SIGTERM"); } catch (_e) {}
    die((e && e.message) || e);
  }
}

// Run main() ONLY when invoked as the entry script (so off-Docker tests can require this
// module for parseArgs without spawning a bridge / calling exit).
if (require.main === module) {
  main();
}

module.exports = { parseArgs };
