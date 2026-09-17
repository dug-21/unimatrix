"use strict";

// C14 verifier — the manifest-executing spine (nan-023, ADR-006).
//
// THE NON-TAUTOLOGY CONTRACT (SR-09, R-02, #4177): this harness EXECUTES what
// the `WireLeg` manifest says — it spawns `manifest[].command` verbatim and
// connects the MCP target described by `manifest[].entry`. It NEVER string-
// matches a command, re-asserts a literal, or reconstructs a path. Config-
// presence is forbidden as the discharge. Every behavioral AC carries a
// mutation control that FAILS when the wired artifact is broken but its manifest
// string is intact — that failure is the proof the harness runs the artifact
// rather than inspecting it.
//
// Shared helpers only (CJS module, not a .test.js): the c14-*.test.js suites
// wire up fixtures/stubs and make the assertions. Zero new deps — node core +
// the existing test/helpers.

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const net = require("net");
const path = require("path");
const { spawn } = require("child_process");

const config = require("../lib/hook-client/config.js");
const { startStubServer } = require("./helpers/stub-server.js");

const STDIO_FIXTURE = path.resolve(__dirname, "fixtures", "mcp", "stdio-mcp-fixture.js");
const DEFAULT_TIMEOUT_MS = 5000;

// ── command-string parsing (quote-aware) ─────────────────────────────────
//
// The wired hook command is exactly `node <clientPath|"quoted path"> <EVENT>
// --provider codex-cli` (merge-settings.buildHookClientCommand). We split it
// respecting double-quotes so a project path containing spaces re-parses to the
// intended argv (R-10 command-injection fixture).

function splitCommand(commandString) {
  const argv = [];
  let cur = "";
  let inQuote = false;
  let sawToken = false;
  for (let i = 0; i < commandString.length; i++) {
    const c = commandString[i];
    if (c === '"') {
      inQuote = !inQuote;
      sawToken = true;
      continue;
    }
    if (!inQuote && (c === " " || c === "\t")) {
      if (sawToken) {
        argv.push(cur);
        cur = "";
        sawToken = false;
      }
      continue;
    }
    cur += c;
    sawToken = true;
  }
  if (sawToken) argv.push(cur);
  return argv;
}

/** The EVENT token from a wired hook command (argv = [node, clientPath, EVENT, ...]). */
function eventOf(commandString) {
  const argv = splitCommand(commandString);
  return argv[2] || "";
}

// ── MCP stdio client (newline-delimited JSON-RPC, production framing) ─────

/** Normalize any wired entry shape into a spawn descriptor.
 *  claude/codex:  { command: "<bin>", args?: [...] }
 *  opencode:      { command: ["node", bridge, hash] | ["<bin>"], environment? } */
function spawnDescriptorFor(entry) {
  if (!entry || typeof entry !== "object") {
    throw new Error("entry missing");
  }
  let cmd;
  let args;
  if (Array.isArray(entry.command)) {
    if (entry.command.length === 0) throw new Error("entry.command array empty");
    cmd = entry.command[0];
    args = entry.command.slice(1);
  } else if (typeof entry.command === "string") {
    cmd = entry.command;
    args = Array.isArray(entry.args) ? entry.args.slice() : [];
  } else {
    throw new Error("entry.command not a string or array");
  }
  const env = Object.assign({}, process.env, entry.env || entry.environment || {});
  return { cmd, args, env };
}

/**
 * Spawn the MCP target from a wired entry VERBATIM, run the JSON-RPC handshake,
 * call one read-only `context_*` tool, and resolve its result. Rejects on spawn
 * error, protocol error, or timeout. Always tears the child down.
 */
function mcpCallFromEntry(entry, opts) {
  const options = opts || {};
  const tool = options.tool || "context_status";
  const timeoutMs = options.timeoutMs || DEFAULT_TIMEOUT_MS;
  const desc = spawnDescriptorFor(entry);

  return new Promise((resolve, reject) => {
    let child;
    try {
      child = spawn(desc.cmd, desc.args, { env: desc.env, stdio: ["pipe", "pipe", "pipe"] });
    } catch (e) {
      reject(new Error("spawn failed: " + e.message));
      return;
    }

    let buffer = "";
    let settled = false;
    let nextId = 1;
    const pending = new Map();

    const timer = setTimeout(() => finish(new Error("mcp call timed out")), timeoutMs);
    if (timer.unref) timer.unref();

    function finish(err, value) {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      try { child.stdin.end(); } catch (_e) {}
      try { child.kill(); } catch (_e) {}
      if (err) reject(err);
      else resolve(value);
    }

    function send(method, params) {
      const id = nextId++;
      const msg = { jsonrpc: "2.0", id, method };
      if (params !== undefined) msg.params = params;
      return new Promise((res, rej) => {
        pending.set(id, { res, rej });
        try {
          child.stdin.write(JSON.stringify(msg) + "\n");
        } catch (e) {
          pending.delete(id);
          rej(new Error("write failed: " + e.message));
        }
      });
    }

    function notify(method, params) {
      const msg = { jsonrpc: "2.0", method };
      if (params !== undefined) msg.params = params;
      try { child.stdin.write(JSON.stringify(msg) + "\n"); } catch (_e) {}
    }

    child.on("error", (e) => finish(new Error("child error: " + e.message)));
    child.on("exit", (code) => {
      if (!settled) finish(new Error("target exited before returning (code " + code + ")"));
    });
    child.stderr.on("data", () => {});
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      buffer += chunk;
      let idx;
      while ((idx = buffer.indexOf("\n")) !== -1) {
        const line = buffer.slice(0, idx);
        buffer = buffer.slice(idx + 1);
        if (line.trim() === "") continue;
        let msg;
        try { msg = JSON.parse(line); } catch (_e) { continue; }
        if (msg.id === undefined || msg.id === null) continue;
        const waiter = pending.get(msg.id);
        if (!waiter) continue;
        pending.delete(msg.id);
        if (msg.error) waiter.rej(new Error("rpc error: " + JSON.stringify(msg.error)));
        else waiter.res(msg.result);
      }
    });

    // Handshake → call.
    (async () => {
      try {
        await send("initialize", {
          protocolVersion: "2025-06-18",
          capabilities: {},
          clientInfo: { name: "c14-verifier", version: "1" },
        });
        notify("notifications/initialized");
        const result = await send("tools/call", { name: tool, arguments: {} });
        finish(null, result);
      } catch (e) {
        finish(e);
      }
    })();
  });
}

/** A response counts as a RETURN iff it carries non-empty content and isn't an error. */
function isNonEmptyReturn(result) {
  if (!result || typeof result !== "object") return false;
  if (result.isError === true) return false;
  const content = result.content;
  if (!Array.isArray(content) || content.length === 0) return false;
  return content.some(
    (c) => c && typeof c === "object" && typeof c.text === "string" && c.text.length > 0
  );
}

/**
 * AC-10 / AC-05 — retrieval-returns. Connect the MCP target from `leg.entry`
 * (the exact wired command/args) and assert a NON-EMPTY return. Throws on any
 * failure — a broken target (mutation control) therefore FAILS here, which is
 * the non-tautology proof (a presence check would still pass).
 */
async function assertRetrievalReturns(leg, opts) {
  assert.ok(leg && typeof leg === "object", "leg present");
  assert.ok(leg.entry, "retrieval/mcp leg carries entry (verifier spawns from it)");
  const result = await mcpCallFromEntry(leg.entry, opts);
  assert.ok(isNonEmptyReturn(result), "context_* returned NON-EMPTY (not a presence proxy)");
  return result;
}

// ── synthetic hook events (stdin JSON per event) ─────────────────────────

const CYCLE_TOOL = "mcp__unimatrix__context_cycle";

/** A minimal, realistic stdin event for the given canonical event name. The
 *  PreToolUse case carries a context_cycle tool so it exercises the codex cycle
 *  matcher (the matched, firing case). */
function buildSyntheticEventFor(event, sessionId) {
  const sid = sessionId || "c14-session";
  switch (event) {
    case "PreToolUse":
      return JSON.stringify({
        session_id: sid,
        tool_name: CYCLE_TOOL,
        tool_input: { type: "start", topic: "nan-023" },
      });
    case "PostToolUse":
      return JSON.stringify({
        session_id: sid,
        tool_name: "Bash",
        tool_response: { ok: true },
      });
    case "UserPromptSubmit":
      return JSON.stringify({ session_id: sid, prompt: "hello from c14" });
    case "SessionStart":
      return JSON.stringify({ session_id: sid, source: "startup" });
    case "Stop":
      return JSON.stringify({ session_id: sid });
    case "PreCompact":
      return JSON.stringify({ session_id: sid, trigger: "auto" });
    case "SubagentStart":
      return JSON.stringify({ session_id: sid, extra: { agent_type: "uni-js-dev" } });
    default:
      return JSON.stringify({ session_id: sid });
  }
}

// ── hook command execution + ingress observation ─────────────────────────

/** Spawn a wired hook command VERBATIM with synthetic stdin and env. Resolves
 *  when the client exits (exit code is always 0 by contract — we do not assert
 *  on it). `node` maps to this test runner's node for CI portability while
 *  preserving argv verbatim. */
function runHookCommand(commandString, stdinStr, spawnOpts) {
  const options = spawnOpts || {};
  const argv = splitCommand(commandString);
  assert.ok(argv.length >= 3, "hook command has [node, clientPath, EVENT]");
  const cmd = argv[0] === "node" ? process.execPath : argv[0];
  const args = argv.slice(1);
  const env = Object.assign({}, process.env, options.env || {});
  delete env.UNIMATRIX_REMOTE_URL;
  delete env.UNIMATRIX_REMOTE_TOKEN;
  if (options.env) {
    if (options.env.UNIMATRIX_REMOTE_URL) env.UNIMATRIX_REMOTE_URL = options.env.UNIMATRIX_REMOTE_URL;
    if (options.env.UNIMATRIX_REMOTE_TOKEN) env.UNIMATRIX_REMOTE_TOKEN = options.env.UNIMATRIX_REMOTE_TOKEN;
  }
  const timeoutMs = options.timeoutMs || DEFAULT_TIMEOUT_MS;

  return new Promise((resolve, reject) => {
    let child;
    try {
      child = spawn(cmd, args, { cwd: options.cwd, env, stdio: ["pipe", "pipe", "pipe"] });
    } catch (e) {
      reject(new Error("spawn hook command failed: " + e.message));
      return;
    }
    const timer = setTimeout(() => {
      try { child.kill(); } catch (_e) {}
      reject(new Error("hook command timed out"));
    }, timeoutMs);
    if (timer.unref) timer.unref();
    child.on("error", (e) => { clearTimeout(timer); reject(e); });
    child.stdout.on("data", () => {});
    child.stderr.on("data", () => {});
    child.on("close", (code) => { clearTimeout(timer); resolve({ code }); });
    child.stdin.on("error", () => {});
    child.stdin.end(Buffer.from(stdinStr === undefined ? "" : stdinStr, "utf8"));
  });
}

/**
 * AC-09 — hook-fires. Execute the exact `leg.command` with a synthetic event on
 * stdin, then observe INGRESS at the JS hook client via `ingress` (the real
 * effect: a frame reaching the transport stub). Returns { fired, frame,
 * provider } — it does NOT throw when nothing fires, so the same call powers the
 * per-event RECORD (fires vs wired-inactive) and the mutation control (which
 * asserts fired === false on a broken artifact).
 *
 * Asserts on `provider`, NEVER `source_domain` (ingress forces claude-code for
 * source_domain, #5748 — asserting it would false-fail). R-05.
 */
async function observeHookFire(leg, ingress, opts) {
  const options = opts || {};
  assert.ok(typeof leg.command === "string" && leg.command.length > 0, "hooks leg carries command");
  const event = eventOf(leg.command);
  const stdin = options.stdin || buildSyntheticEventFor(event, options.sessionId);
  const before = ingress.frameCount();
  await runHookCommand(leg.command, stdin, {
    cwd: ingress.cwd,
    env: ingress.env,
    timeoutMs: options.timeoutMs,
  });
  // Give the FNF POST a beat to land after the client's event loop drains.
  await ingress.settle();
  const frames = ingress.framesSince(before);
  const fired = frames.length > 0;
  const frame = fired ? frames[frames.length - 1] : null;
  return { fired, frame, provider: frame ? frame.provider : undefined, event };
}

// ── ingress observers (real transports the client actually posts to) ─────

function sleep(ms) {
  return new Promise((r) => {
    const t = setTimeout(r, ms);
    if (t.unref) t.unref();
  });
}

/**
 * HTTP ingress — the "cloud" command-level path. Sets UNIMATRIX_REMOTE_URL/TOKEN
 * so the client posts every frame to a scriptable stub; each recorded request
 * body is a wire frame carrying `provider`. Deployment-representative and CI-
 * robust (no binary, no trust).
 */
async function makeHttpIngress() {
  const stub = await startStubServer();
  stub.respondWith({ status: 204 });
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "c14-http-home-"));
  return {
    kind: "cloud-http",
    cwd: home,
    env: {
      UNIMATRIX_REMOTE_URL: stub.url,
      UNIMATRIX_REMOTE_TOKEN: "c14-token",
      HOME: home,
      USERPROFILE: home,
    },
    frameCount() { return stub.requests.length; },
    framesSince(n) {
      return stub.requests.slice(n).map((r) => {
        try { return JSON.parse(r.body.toString("utf8")); } catch (_e) { return {}; }
      });
    },
    settle() { return sleep(150); },
    async close() {
      await stub.close();
      try { fs.rmSync(home, { recursive: true, force: true }); } catch (_e) {}
    },
  };
}

/**
 * UDS ingress — the "local" (Rust binary / UDS) path. With no remote env the
 * client falls to UDS mode at ~/.unimatrix/<hash>/unimatrix.sock; we listen at
 * that exact derived path and capture framed request bodies (4-byte BE len +
 * JSON). Same frames, same `provider` field — deployment parity for AC-09 local.
 */
async function makeUdsIngress() {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "c14-uds-home-"));
  const projectRoot = fs.mkdtempSync(path.join(os.tmpdir(), "c14-uds-proj-"));
  fs.mkdirSync(path.join(projectRoot, ".git"), { recursive: true });
  const root = config.walkToProjectRoot(projectRoot);
  const hash = config.computeProjectHash(root);
  const socketPath = path.join(home, ".unimatrix", hash, "unimatrix.sock");
  fs.mkdirSync(path.dirname(socketPath), { recursive: true });

  const frames = [];
  const sockets = new Set();
  const server = net.createServer({ allowHalfOpen: true }, (s) => {
    sockets.add(s);
    s.on("error", () => {});
    s.on("close", () => sockets.delete(s));
    const chunks = [];
    let received = 0;
    let declaredLen = null;
    s.on("data", (chunk) => {
      chunks.push(chunk);
      received += chunk.length;
      if (declaredLen === null && received >= 4) {
        declaredLen = Buffer.concat(chunks).readUInt32BE(0);
      }
      if (declaredLen !== null && received >= 4 + declaredLen) {
        const body = Buffer.concat(chunks).subarray(4, 4 + declaredLen);
        try { frames.push(JSON.parse(body.toString("utf8"))); } catch (_e) { frames.push({}); }
        try { s.end(); } catch (_e) {}
      }
    });
  });

  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(socketPath, resolve);
  });

  return {
    kind: "local-uds",
    cwd: projectRoot,
    env: { HOME: home, USERPROFILE: home },
    socketPath,
    frameCount() { return frames.length; },
    framesSince(n) { return frames.slice(n); },
    settle() { return sleep(150); },
    async close() {
      for (const s of sockets) { try { s.destroy(); } catch (_e) {} }
      await new Promise((res) => server.close(() => res()));
      for (const d of [home, projectRoot]) {
        try { fs.rmSync(d, { recursive: true, force: true }); } catch (_e) {}
      }
    },
  };
}

// ── temp project fixtures (shared with the wire tests' idiom) ─────────────

function makeTempProject(extraDirs) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "c14-proj-"));
  fs.mkdirSync(path.join(dir, ".git"), { recursive: true });
  for (const rel of extraDirs || []) {
    fs.mkdirSync(path.join(dir, rel), { recursive: true });
  }
  return dir;
}

function cleanupDir(dir) {
  try { fs.rmSync(dir, { recursive: true, force: true }); } catch (_e) {}
}

module.exports = {
  STDIO_FIXTURE,
  splitCommand,
  eventOf,
  spawnDescriptorFor,
  mcpCallFromEntry,
  isNonEmptyReturn,
  assertRetrievalReturns,
  buildSyntheticEventFor,
  runHookCommand,
  observeHookFire,
  makeHttpIngress,
  makeUdsIngress,
  makeTempProject,
  cleanupDir,
};
