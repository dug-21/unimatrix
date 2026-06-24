"use strict";

// bridge-cycle-driver.js (nan-021 C2) — drives the parity cycle THROUGH the
// SHIPPED `node mcp-bridge.js <projectHash>` over stdio JSON-RPC (D-2: the
// bridge is IN PATH as the local MCP server — NEVER a direct mcp_url POST).
//
// This is a C2 test helper (test tree only); it adds NO transport/cert/credstore
// code — it spawns the unmodified bridge and speaks newline-delimited JSON-RPC
// to it (the framing the bridge's StdioFramer reads/writes). The bridge owns the
// pinned-HTTPS session, Mcp-Session-Id capture/replay, SSE parse, and the #830
// self-heal; this driver re-implements NONE of that (NFR-2).
//
// It loads bridge-witness.js into the bridge via NODE_OPTIONS=--require so the
// wire is OBSERVED (BRIDGE_WITNESS lines on the bridge's stderr) without altering
// it. The driver:
//   1. spawns the bridge LAST and drives IMMEDIATELY (NFR-7 idle-window min —
//      no wait between session-id capture and the first tool call);
//   2. initialize -> context_cycle(start) -> each manifest tool call (incl. the
//      load-bearing Bash carrying the feature-ID token) + nothing else on MCP
//      (the live /observe hooks are fired by the SHELL gate, pinned, with the
//      stable session_id) -> context_cycle(stop);
//   3. (the shell runs the durability barrier) -> context_cycle_review;
//   4. prints, to STDOUT, ONE json line: {ok, metric_vector, witness, error}.
//
// Self-heal reliance (R-05): a mid-cycle SESSION_NOT_FOUND is healed by the
// SHIPPED single-flight keep_alive re-init inside the bridge. This driver adds
// no retry/reconnect. A heal-exhausting failure surfaces as a JSON-RPC error on
// the call, which the driver reports as a HARD failure (the shell tail-dumps the
// captured bridge stderr).
//
// Usage:
//   node bridge-cycle-driver.js <projectHash> <manifestPath> \
//     --bridge <path-to-mcp-bridge.js> --witness <path-to-bridge-witness.js>
// Stdout: a single JSON line (the result). Diagnostics -> stderr.

const { spawn } = require("child_process");
const fs = require("fs");
const path = require("path");

function die(msg) {
  process.stdout.write(
    JSON.stringify({ ok: false, error: String(msg), metric_vector: null, witness: null }) + "\n"
  );
  process.exit(1);
}

function parseArgs(argv) {
  const pos = [];
  const opt = {};
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--bridge" || a === "--witness") {
      opt[a.slice(2)] = argv[++i];
    } else {
      pos.push(a);
    }
  }
  return { projectHash: pos[0], manifestPath: pos[1], opt };
}

// MCP envelope helpers (standard JSON-RPC; mirrors harness/client.py).
function toolCall(id, name, args) {
  return { jsonrpc: "2.0", id, method: "tools/call", params: { name, arguments: args || {} } };
}

async function main() {
  const { projectHash, manifestPath, opt } = parseArgs(process.argv);
  if (!projectHash || !manifestPath) {
    die("usage: bridge-cycle-driver.js <projectHash> <manifestPath> --bridge <p> --witness <p>");
  }
  const bridgePath = opt.bridge;
  const witnessPath = opt.witness;
  if (!bridgePath || !fs.existsSync(bridgePath)) die("bridge path missing: " + bridgePath);
  if (!witnessPath || !fs.existsSync(witnessPath)) die("witness path missing: " + witnessPath);

  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  } catch (e) {
    die("manifest unreadable: " + (e && e.message));
  }
  const sid = manifest.session_id;
  const feature = manifest.feature_cycle;
  const toolCalls = manifest.tool_calls || [];
  if (!sid || !feature || toolCalls.length === 0) die("manifest missing session_id/feature_cycle/tool_calls");

  // ---- spawn the REAL bridge with the witness preloaded (instrumentation only) ----
  // HOME is inherited from the shell gate ($SANDBOX/home) so credstore.read finds
  // THIS run's remote.json. NODE_OPTIONS injects the witness without touching the
  // shipped bridge. The bridge's own stderr (incl. BRIDGE_WITNESS lines + #830
  // self-heal notices) is INHERITED to fd2, which the shell gate captured to file.
  const env = Object.assign({}, process.env, {
    NODE_OPTIONS:
      (process.env.NODE_OPTIONS ? process.env.NODE_OPTIONS + " " : "") +
      "--require " +
      path.resolve(witnessPath),
  });
  const bridge = spawn(process.execPath, [path.resolve(bridgePath), projectHash], {
    stdio: ["pipe", "pipe", "inherit"],
    env,
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
    if (resp.error) {
      throw new Error(what + " failed: " + JSON.stringify(resp.error));
    }
    // tools/call surfaces tool-level errors inside result.isError.
    const r = resp.result;
    if (r && r.isError) {
      const txt = r.content && r.content[0] && r.content[0].text ? r.content[0].text : "";
      throw new Error(what + " tool error: " + txt);
    }
    return resp;
  }

  function resultText(resp) {
    const r = resp && resp.result;
    if (r && Array.isArray(r.content)) {
      for (const c of r.content) if (c && c.type === "text" && typeof c.text === "string") return c.text;
    }
    return "";
  }

  try {
    // 1. initialize — captures Mcp-Session-Id inside the bridge (R-17). NO wait
    //    after this before the first tool call (NFR-7 idle-window minimization).
    const initResp = await rpc(
      (id) => ({
        jsonrpc: "2.0",
        id,
        method: "initialize",
        params: {
          protocolVersion: "2024-11-05",
          capabilities: {},
          clientInfo: { name: "nan-021-bridge-cycle-driver", version: "1.0.0" },
        },
      }),
      45000
    );
    ensureOk(initResp, "initialize");
    notify({ jsonrpc: "2.0", method: "notifications/initialized" });

    // 2. context_cycle(start) — declare the cycle THROUGH the bridge.
    ensureOk(
      await rpc((id) =>
        // The context_cycle tool argument is `type` (NOT cycle_type) — mirrors
        // harness/client.py:693 ({"type": cycle_type, ...}). The server rejects
        // cycle_type with -32602 "missing field `type`" (verified live).
        toolCall(id, "context_cycle", {
          type: "start",
          topic: feature,
          agent_id: sid,
        })
      ),
      "context_cycle(start)"
    );

    // 3. The manifest's tool_calls (Read/Bash/Grep) are the simulated AGENT
    //    workload — Claude-Code host tools, NOT Unimatrix MCP tools. The live
    //    server exposes ONLY context_* tools (verified: an MCP `tools/call` for
    //    "Read" returns -32602 "tool not found"). Their MCP-visible effect is the
    //    cycle bracket here PLUS the live PostToolUse /observe hooks the SHELL
    //    fires (carrying the load-bearing Bash feature-ID token — FR-3). So the
    //    bridge carries context_cycle(start) -> context_cycle(stop) -> review;
    //    the workload tool calls themselves are NOT replayed as MCP calls. This
    //    keeps the bridge in path for the real cycle/review traffic (D-2/AC-02:
    //    SSE + session-id replay are proven on these context_cycle calls) without
    //    inventing server tools that do not exist (NFR-2). `toolCalls` is consumed
    //    by the shell's /observe loop, not here; we only assert the manifest has
    //    the expected workload shape for diagnosability.
    void toolCalls;

    // 4. context_cycle(stop).
    ensureOk(
      await rpc((id) =>
        toolCall(id, "context_cycle", { type: "stop", topic: feature, agent_id: sid })
      ),
      "context_cycle(stop)"
    );

    // The shell runs the durability barrier between stop and review; the driver
    // is invoked a SECOND time with --review-only for that ordering. But to keep
    // ONE bridge session (session-id replay assertion + idle-window min), this
    // driver supports an inline review when REVIEW_INLINE=1, else it returns here
    // so the shell can barrier, then re-drive review on a fresh bridge. Default:
    // inline review is NOT taken — the shell owns barrier ordering (ADR-006).
    let mv = null;
    if (process.env.REVIEW_INLINE === "1") {
      // format:"json" is REQUIRED — the default review result is a markdown
      // report; only json yields the parseable RetrospectiveReport carrying the
      // MetricVector at `.metrics` (verified live: default returns "# Unimatrix
      // Cycle Review ..." markdown). force:true drains even a fresh cycle.
      const rev = ensureOk(
        await rpc(
          (id) => toolCall(id, "context_cycle_review", { feature_cycle: feature, format: "json", force: true }),
          60000
        ),
        "context_cycle_review"
      );
      // The review tool-result text is the RetrospectiveReport JSON; the
      // MetricVector lives at `.metrics` (c2 pseudocode L69). Emit the parsed
      // MetricVector DICT (the comparator reads .universal/.phases/.domain_metrics
      // directly — ADR-003) so BOTH legs hand the comparator the same shape. If
      // `.metrics` is absent (schema drift), fall back to the whole parsed object
      // so the comparator's own schema-drift guard surfaces it loudly.
      const txt = resultText(rev);
      let parsed;
      try {
        parsed = JSON.parse(txt);
      } catch (_e) {
        throw new Error("context_cycle_review result was not JSON: " + txt.slice(0, 200));
      }
      mv = parsed && typeof parsed.metrics === "object" && parsed.metrics !== null ? parsed.metrics : parsed;
    }

    // Close the bridge (stdin EOF -> teardown DELETE -> exit 0).
    bridge.stdin.end();
    await new Promise((r) => setTimeout(r, 200));

    if (bridgeExitErr) throw bridgeExitErr;

    process.stdout.write(
      JSON.stringify({
        ok: true,
        phase: process.env.REVIEW_INLINE === "1" ? "review" : "cycle",
        metric_vector: mv,
        session_id: sid,
        error: null,
      }) + "\n"
    );
    process.exit(0);
  } catch (e) {
    try { bridge.stdin.end(); } catch (_e) {}
    try { bridge.kill("SIGTERM"); } catch (_e) {}
    die((e && e.message) || e);
  }
}

main();
