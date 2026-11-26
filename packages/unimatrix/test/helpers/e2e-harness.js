"use strict";
// Assembled-path E2E harness (nan-024, test-plan OVERVIEW layer 2). Runs the real
// `bin/unimatrix.js init` into an isolated HOME that contains a SPACE, then executes
// the literal command read from the written config through real harness shells.
// Stubs are Node processes only (no Rust, no Unix-shape fakes) so Windows can run it.
const fs = require("fs");
const os = require("os");
const path = require("path");
const { assertTmpHome } = require("./home-guard");
const https = require("https");
const crypto = require("crypto");
const { spawn } = require("child_process");

const BIN = path.join(__dirname, "..", "..", "bin", "unimatrix.js");
const WIN = process.platform === "win32";

/** { certPem, keyPem, fp, certPath } from E2E_CERT_DIR (CI) or the openssl helper; null if neither. */
function loadCert() {
  const dir = process.env.E2E_CERT_DIR;
  let certPem, keyPem, certPath;
  if (dir && fs.existsSync(path.join(dir, "cert.pem"))) {
    certPath = path.join(dir, "cert.pem");
    certPem = fs.readFileSync(certPath);
    keyPem = fs.readFileSync(path.join(dir, "key.pem"));
  } else {
    const g = require("./mcp-stub-server.js").GEN;
    if (!g) return null;
    certPath = path.join(g.dir, "cert.pem");
    certPem = g.certPem;
    keyPem = g.keyPem;
  }
  const fp = "sha256:" + crypto.createHash("sha256").update(new crypto.X509Certificate(certPem).raw).digest("hex");
  return { certPem, keyPem, fp, certPath };
}

/** HTTPS recording stub: Ping -> Pong, other observe frames -> Ack, JSON-RPC on the mcp url. */
function startStub(cert) {
  const requests = [];
  const server = https.createServer({ cert: cert.certPem, key: cert.keyPem }, (req, res) => {
    req.on("error", () => {});
    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => {
      const body = Buffer.concat(chunks).toString("utf8");
      requests.push({ method: req.method, url: req.url, auth: req.headers.authorization, body });
      let j = null;
      try { j = JSON.parse(body); } catch (_e) { /* not json */ }
      const h = { "Content-Type": "application/json" };
      let out;
      if (j && j.jsonrpc) {
        if (j.method === "initialize") h["Mcp-Session-Id"] = crypto.randomUUID();
        out = j.id === undefined ? null : {
          jsonrpc: "2.0", id: j.id,
          result: j.method === "initialize"
            ? { protocolVersion: "2025-06-18", capabilities: { tools: {} }, serverInfo: { name: "e2e-stub", version: "0" } }
            : { tools: [] },
        };
      } else {
        out = j && j.type === "Ping" ? { type: "Pong", server_version: "e2e" } : { type: "Ack" };
      }
      if (out === null) { res.writeHead(202, h); res.end(); return; }
      res.writeHead(200, h);
      res.end(JSON.stringify(out));
    });
  });
  server.on("tlsClientError", () => {});
  return new Promise((resolve) => server.listen(0, "127.0.0.1", () => {
    const port = server.address().port;
    resolve({
      port, requests, base: "https://127.0.0.1:" + port,
      events: () => requests.filter((r) => r.url.endsWith("/observe") && !/"Ping"/.test(r.body)),
      close: () => new Promise((r) => { server.close(() => r()); if (server.closeAllConnections) server.closeAllConnections(); }),
    });
  }));
}

function bundleFor(stub, cert) {
  const j = { v: 2, mcp_url: stub.base + "/v1/s", observe_url: stub.base + "/v1/s/observe", token: "a".repeat(64), fp: cert.fp };
  return "unimatrix-bundle:" + Buffer.from(JSON.stringify(j)).toString("base64url");
}

/** Fixture: <root with space>/{"home dir", proj(.git), other(no .git), "codex home"}. */
function makeFixture() {
  const base = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), "uni e2e ")));
  const f = { root: base, home: path.join(base, "home dir"), proj: path.join(base, "proj"), other: path.join(base, "other"), codex: path.join(base, "codex home") };
  assertTmpHome(f.home);
  fs.mkdirSync(f.home);
  fs.mkdirSync(f.other);
  fs.mkdirSync(f.codex);
  fs.mkdirSync(path.join(f.proj, ".git"), { recursive: true });
  f.cleanup = () => { try { fs.rmSync(base, { recursive: true, force: true }); } catch (_e) { /* best effort */ } };
  return f;
}

function baseEnv(f, cert, extra) {
  const env = Object.assign({}, process.env);
  for (const k of Object.keys(env)) if (/^UNIMATRIX_/.test(k) || k === "CODEX_HOME") delete env[k];
  assertTmpHome(f.home);
  Object.assign(env, { HOME: f.home, USERPROFILE: f.home, NODE_EXTRA_CA_CERTS: cert.certPath }, extra || {});
  return env;
}

/** Async spawn (never spawnSync: stub runs in this process, #4774). */
function run(cmd, args, o) {
  return new Promise((resolve) => {
    let out = "", err = "", done = false;
    const fin = (r) => { if (!done) { done = true; clearTimeout(t); resolve(r); } };
    let p;
    try { p = spawn(cmd, args, Object.assign({ stdio: ["pipe", "pipe", "pipe"], windowsHide: true }, o.spawn)); }
    catch (e) { return resolve({ code: null, out, err: String(e) }); }
    const t = setTimeout(() => { try { p.kill(); } catch (_e) { /* gone */ } }, o.timeoutMs || 30000);
    p.stdout.on("data", (c) => { out += c; if (o.onStdout) o.onStdout(out, p); });
    p.stderr.on("data", (c) => { err += c; });
    p.on("error", (e) => { err += String(e); fin({ code: null, out, err }); });
    p.on("close", (code) => fin({ code, out, err }));
    p.stdin.on("error", () => {});
    if (o.input !== undefined) p.stdin.end(o.input);
  });
}

function runInit(f, env, args) {
  return run(process.execPath, [BIN, "init"].concat(args || []), { spawn: { cwd: f.proj, env }, timeoutMs: 120000 });
}

function findBash() {
  const c = WIN
    ? [process.env.GIT_BASH, "C:\\Program Files\\Git\\bin\\bash.exe"]
    : ["/bin/bash", "/usr/bin/bash", "/usr/local/bin/bash", "/opt/homebrew/bin/bash"];
  return c.find((p) => p && fs.existsSync(p)) || null;
}

/** Harness shells that run a literal command line. [{name, file, args(line), verbatim}] */
function shells() {
  const out = [];
  if (WIN) {
    const comspec = process.env.ComSpec || "cmd.exe";
    out.push({ name: "cmd.exe /C <line>", file: comspec, args: (l) => ["/C", l], verbatim: true });
    out.push({ name: 'cmd.exe /C "<line>" (Codex wrapping)', file: comspec, args: (l) => ["/C", '"' + l + '"'], verbatim: true });
    const gb = findBash();
    if (gb) {
      out.push({ name: "Git Bash -c", file: gb, args: (l) => ["-c", l] });
      out.push({ name: "Git Bash -lc", file: gb, args: (l) => ["-lc", l] });
    }
  } else {
    out.push({ name: "sh -c", file: "/bin/sh", args: (l) => ["-c", l] });
    const b = findBash();
    if (b) out.push({ name: "bash -lc", file: b, args: (l) => ["-lc", l] });
    const login = process.env.SHELL;
    if (login && !/\/(ba)?sh$/.test(login) && fs.existsSync(login)) out.push({ name: "$SHELL -lc", file: login, args: (l) => ["-lc", l] });
  }
  return out;
}

function runLine(sh, line, cwd, env, input) {
  return run(sh.file, sh.args(line), { spawn: { cwd, env, windowsVerbatimArguments: !!sh.verbatim }, input });
}

function event(f, sessionId, name) {
  return JSON.stringify({ session_id: sessionId, hook_event_name: name || "PostToolUse", cwd: f.proj, tool_name: "Read", tool_input: { file_path: "x" }, tool_response: "ok" });
}

function hookCommand(file, eventName) {
  const j = JSON.parse(fs.readFileSync(file, "utf8"));
  const e = j.hooks && j.hooks[eventName];
  if (!e || !e[0] || !e[0].hooks || !e[0].hooks[0]) throw new Error("no " + eventName + " hook in " + file);
  return e[0].hooks[0].command;
}

/** Spawn the written MCP entry (shell:false, like Claude Code/Codex) and run `initialize`. */
function mcpInitialize(entry, cwd, env) {
  const msg = JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "e2e", version: "0" } } }) + "\n";
  const e = Object.assign({}, env, entry.env || {});
  return run(entry.command, entry.args || [], {
    spawn: { cwd, env: e },
    onStdout: (out, p) => { if (out.includes("\n")) { try { p.stdin.end(); } catch (_e) { /* closed */ } } },
    input: msg,
    timeoutMs: 30000,
  });
}

module.exports = { BIN, WIN, loadCert, startStub, bundleFor, makeFixture, baseEnv, run, runInit, shells, runLine, event, hookCommand, mcpInitialize, findBash };
