"use strict";

// vnc-050 C11 — scripted local smoke (AC-12, local AC-13, AC-16, AC-17).
// Boots the cargo-built server with a CA-issued leaf (local CA), mints headless
// bundles through the real CLI, and executes the docs/client-setup.md recipe
// blocks VERBATIM (fences tagged ```sh smoke``` / ```json mcp-config```). Any edit
// to the documented commands is exercised automatically. Live GH Actions and
// Tailscale runs are post-delivery operator checks, not covered here.
//
// `npx -y @dug-21/unimatrix` in the recipe would fetch the published package,
// which predates this build; the smoke puts a temp `npx` shim first on PATH
// that drops `-y @dug-21/unimatrix` and runs THIS checkout's bin/unimatrix.js.

const { describe, it, before, after } = require("node:test");
const assert = require("node:assert/strict");
const { spawn, execFileSync } = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");

const { startRealServer, resolveServerBinary } = require("./helpers/real-server");
const { makeCaFixture } = require("./helpers/ca-fixture");
const { decodeBundle } = require("../lib/hook-client/bundle");

const DOC = path.resolve(__dirname, "../../../docs/client-setup.md");
const BIN = path.resolve(__dirname, "../bin/unimatrix.js");

function serverBuilt() {
  try {
    resolveServerBinary();
    return null;
  } catch (e) {
    return "server binary not built (cargo build first): " + e.message.split("\n")[0];
  }
}
const SKIP = serverBuilt();

/** Extract fenced blocks whose info string is exactly `tag`. */
function blocks(md, tag) {
  const re = new RegExp("^```" + tag.replace(/ /g, "[ ]") + "\\n([\\s\\S]*?)^```$", "gm");
  const out = [];
  let m;
  while ((m = re.exec(md)) !== null) out.push(m[1]);
  return out;
}

/** Run a shell script; resolves { code, stdout, stderr }. 30 s bound. */
function sh(script, env, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn("sh", ["-euc", script], { env, cwd, stdio: ["ignore", "pipe", "pipe"] });
    const o = [];
    const e = [];
    const timer = setTimeout(() => child.kill("SIGKILL"), 30000);
    child.stdout.on("data", (c) => o.push(c));
    child.stderr.on("data", (c) => e.push(c));
    child.on("error", reject);
    child.on("exit", (code) => {
      clearTimeout(timer);
      resolve({ code, stdout: Buffer.concat(o).toString(), stderr: Buffer.concat(e).toString() });
    });
  });
}

/**
 * Drive a stdio MCP server (the bridge) with JSON-RPC requests in order;
 * resolves { responses: Map<id,msg>, stderr, code }.
 */
function mcpSession(cmd, args, env, requests) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, { env, stdio: ["pipe", "pipe", "pipe"] });
    const responses = new Map();
    const wanted = requests.filter((r) => r.id !== undefined).map((r) => r.id);
    let buf = "";
    const errs = [];
    const timer = setTimeout(() => child.kill("SIGKILL"), 30000);
    child.stderr.on("data", (c) => errs.push(c));
    child.on("error", reject);
    let next = 0;
    const sendNext = () => {
      while (next < requests.length) {
        const r = requests[next++];
        child.stdin.write(JSON.stringify(r) + "\n");
        if (r.id !== undefined) return; // wait for its response
      }
      child.stdin.end();
    };
    child.stdout.on("data", (c) => {
      buf += c.toString();
      let i;
      while ((i = buf.indexOf("\n")) >= 0) {
        const line = buf.slice(0, i).trim();
        buf = buf.slice(i + 1);
        if (!line) continue;
        const msg = JSON.parse(line);
        if (msg.id !== undefined) responses.set(msg.id, msg);
        if (wanted.every((id) => responses.has(id))) child.stdin.end();
        else sendNext();
      }
    });
    child.on("exit", (code) => {
      clearTimeout(timer);
      resolve({ responses, stderr: Buffer.concat(errs).toString(), code });
    });
    sendNext();
  });
}

const INIT = {
  jsonrpc: "2.0",
  id: 1,
  method: "initialize",
  params: { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "smoke", version: "0" } },
};
const INITIALIZED = { jsonrpc: "2.0", method: "notifications/initialized" };
const call = (id, name, args) => ({ jsonrpc: "2.0", id, method: "tools/call", params: { name, arguments: args } });
const toolText = (msg) => (msg && msg.result && msg.result.content ? msg.result.content.map((c) => c.text).join("") : "");
const isToolOk = (msg) => !!msg && !msg.error && msg.result && msg.result.isError !== true;

/** Raw curl POST to an MCP URL; resolves HTTP status. */
async function curlStatus(caPath, token, url, body, sid) {
  const args = ["-sS", "-o", "/dev/null", "-w", "%{http_code}", "--cacert", caPath,
    "-H", "Authorization: Bearer " + token, "-H", "Content-Type: application/json",
    "-H", "Accept: application/json, text/event-stream", "-d", JSON.stringify(body)];
  if (sid) args.push("-H", "Mcp-Session-Id: " + sid);
  args.push(url);
  return Number(execFileSync("curl", args).toString().trim());
}

describe("vnc-050 C11 headless smoke (docs recipe, local CA, real server)", { skip: SKIP || false }, () => {
  let ca;
  let srv;
  let tmp;
  let readBundle;
  let rwBundle;
  let env; // client env: PATH with npx shim, clean HOME, CA for curl + node
  const md = fs.readFileSync(DOC, "utf8");

  before(async () => {
    ca = makeCaFixture();
    assert.ok(ca, "openssl is required for the local CA fixture");
    srv = await startRealServer({
      tlsPair: ca.good,
      extraSlugs: ["alpha", "beta"],
      publicUrl: true,
      // Reuse the developer/CI model cache so context_store can embed (temp HOME has none).
      env: { UNIMATRIX_MODEL_CACHE: process.env.UNIMATRIX_MODEL_CACHE || path.join(os.homedir(), ".cache", "unimatrix", "models") },
    });
    const mint = async (mode, label) => {
      const r = await srv.cli(["client-bundle", "alpha", "--headless", "--mode", mode, "--label", label, "--trust", "ca"]);
      assert.equal(r.code, 0, r.stderr);
      return r.stdout.trim();
    };
    readBundle = await mint("read", "ci-read");
    rwBundle = await mint("read-write", "ci-rw");

    tmp = fs.mkdtempSync(path.join(os.tmpdir(), "uni-smoke-"));
    const shimDir = path.join(tmp, "bin");
    const home = path.join(tmp, "home");
    fs.mkdirSync(shimDir);
    fs.mkdirSync(home);
    fs.writeFileSync(
      path.join(shimDir, "npx"),
      '#!/bin/sh\n[ "$1" = "-y" ] && shift\n[ "$1" = "@dug-21/unimatrix" ] && shift\nexec "' +
        process.execPath + '" "' + BIN + '" "$@"\n',
      { mode: 0o755 }
    );
    env = {
      PATH: shimDir + path.delimiter + process.env.PATH,
      HOME: home,
      USERPROFILE: home,
      CA: ca.caPath,
      NODE_EXTRA_CA_CERTS: ca.caPath,
    };
  });

  after(async () => {
    if (srv) await srv.close();
    for (const d of [tmp, ca && ca.dir]) if (d) fs.rmSync(d, { recursive: true, force: true });
  });

  it("recipe_blocks_extracted_verbatim", () => {
    const b = blocks(md, "sh smoke");
    assert.ok(b.length >= 3, "expected >= 3 ```sh smoke``` blocks, found " + b.length);
    for (const blk of b) assert.ok(md.includes("```sh smoke\n" + blk + "```"), "block not verbatim");
    assert.equal(blocks(md, "json mcp-config").length, 1);
  });

  it("curl_recipe_runs_unmodified (AC-12)", async () => {
    const curlBlocks = blocks(md, "sh smoke");
    const r = await sh(curlBlocks.join("\n"), Object.assign({}, env, { UNIMATRIX_BUNDLE: readBundle }), tmp);
    assert.equal(r.code, 0, r.stderr);
    assert.match(r.stdout, /"id":2,"result"/);
    assert.doesNotMatch(r.stdout, /"isError":true/);
  });

  it("generic_mcp_client_recipe (JSON config)", async () => {
    const cfg = JSON.parse(blocks(md, "json mcp-config")[0]).mcpServers.unimatrix;
    const cfgEnv = Object.assign({}, env, cfg.env, { UNIMATRIX_BUNDLE: readBundle });
    const s = await mcpSession(cfg.command, cfg.args, cfgEnv, [INIT, INITIALIZED, call(2, "context_status", {})]);
    assert.ok(isToolOk(s.responses.get(2)), s.stderr);
    assert.ok(!fs.existsSync(path.join(env.HOME, ".unimatrix")), "headless attach must not write remote.json");
  });

  it("ac13_local_read_bundle_search_ok_write_denied", async () => {
    const s = await mcpSession(process.execPath, [BIN, "mcp-bridge"], Object.assign({}, env, { UNIMATRIX_BUNDLE: readBundle }), [
      INIT,
      INITIALIZED,
      call(2, "context_lookup", { query: "smoke" }),
      call(3, "context_store", { title: "denied", content: "read token must not write", topic: "smoke", category: "pattern", agent_id: "human" }),
    ]);
    assert.ok(isToolOk(s.responses.get(2)), "read lookup: " + JSON.stringify(s.responses.get(2)));
    const denied = s.responses.get(3);
    assert.ok(!isToolOk(denied), "read token store must be denied");
    assert.match(JSON.stringify(denied), /[Cc]apabilit/);
  });

  it("ac13_local_rw_token_writes_only_its_slug", async () => {
    let s;
    const deadline = Date.now() + 120000; // embedding model loads asynchronously after boot
    for (;;) {
      s = await mcpSession(process.execPath, [BIN, "mcp-bridge"], Object.assign({}, env, { UNIMATRIX_BUNDLE: rwBundle }), [
        INIT,
        INITIALIZED,
        call(2, "context_store", { title: "smoke rw", content: "rw token writes alpha", topic: "smoke", category: "pattern", agent_id: "human" }),
      ]);
      if (!/initializing/.test(JSON.stringify(s.responses.get(2))) || Date.now() > deadline) break;
      await new Promise((r) => setTimeout(r, 1000));
    }
    assert.ok(isToolOk(s.responses.get(2)), "rw store: " + JSON.stringify(s.responses.get(2)) + s.stderr);
    const b = decodeBundle(rwBundle);
    const betaUrl = b.mcp_url.replace(/\/v1\/alpha$/, "/v1/beta");
    assert.notEqual(betaUrl, b.mcp_url);
    assert.equal(await curlStatus(ca.caPath, b.token, betaUrl, INIT), 403);
    assert.equal(await curlStatus(ca.caPath, b.token, b.mcp_url, INIT), 200); // success companion
  });

  it("ac16_end_to_end: revoke rejects the next call in an open session", async () => {
    const r = await srv.cli(["client-bundle", "alpha", "--headless", "--mode", "read", "--label", "ci-revoke", "--trust", "ca"]);
    assert.equal(r.code, 0, r.stderr);
    const b = decodeBundle(r.stdout.trim());
    const hdr = path.join(tmp, "rev-headers.txt");
    execFileSync("curl", ["-sS", "-o", "/dev/null", "-D", hdr, "--cacert", ca.caPath,
      "-H", "Authorization: Bearer " + b.token, "-H", "Content-Type: application/json",
      "-H", "Accept: application/json, text/event-stream", "-d", JSON.stringify(INIT), b.mcp_url]);
    const sid = (fs.readFileSync(hdr, "utf8").match(/^mcp-session-id:\s*(\S+)/im) || [])[1];
    assert.ok(sid, "initialize returned a session id");
    assert.equal(await curlStatus(ca.caPath, b.token, b.mcp_url, INITIALIZED, sid), 202);
    const rv = await srv.cli(["client-bundle", "alpha", "--revoke", "ci-revoke"]);
    assert.equal(rv.code, 0, rv.stderr);
    assert.equal(await curlStatus(ca.caPath, b.token, b.mcp_url, call(2, "context_status", {}), sid), 401);
    const list = await srv.cli(["client-bundle", "alpha", "--list"]);
    assert.match(list.stderr + list.stdout, /ci-revoke\s+read\s+alpha.*revoked/);
  });

  it("audit_attribution_retained_after_revoke (AC-17)", async (t) => {
    let py = null;
    try {
      execFileSync("python3", ["-I", "-c", "import sqlite3"], { stdio: "ignore" });
      py = "python3";
    } catch (_e) {
      t.skip("python3 sqlite3 unavailable — cannot read audit_log without a JS sqlite driver");
      return;
    }
    const rv = await srv.cli(["client-bundle", "alpha", "--revoke", "ci-rw"]);
    assert.equal(rv.code, 0, rv.stderr);
    const db = path.join(srv.home, ".unimatrix", "alpha", "unimatrix.db");
    assert.ok(fs.existsSync(db), "slug store at " + db);
    const q = "import sqlite3,sys;c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True);" +
      "print(c.execute(\"select count(*) from audit_log where operation='http_access' and agent_id='token:ci-rw'\").fetchone()[0])";
    let n = 0;
    const deadline = Date.now() + 5000; // http_access rows are fire-and-forget
    while (Date.now() < deadline) {
      n = Number(execFileSync(py, ["-I", "-c", q, db]).toString().trim());
      if (n > 0) break;
      await new Promise((r) => setTimeout(r, 100));
    }
    assert.ok(n > 0, "http_access rows for token:ci-rw survive revoke");
  });

  it("docs_contain_legacy_baseline_and_gh_actions_example", () => {
    assert.match(md, /legacy_token_enabled = false/);
    assert.match(md, /UNIMATRIX_LEGACY_TOKEN_ENABLED=false/);
    assert.match(md, /```yaml\n[\s\S]*UNIMATRIX_BUNDLE: \$\{\{ secrets\./);
    for (const s of ["--headless --mode read", "--list", "--revoke", "--trust ca", "UNIMATRIX_REMOTE_HEADERS", "NODE_EXTRA_CA_CERTS"]) {
      assert.ok(md.includes(s), "doc mentions " + s);
    }
    assert.ok(!fs.existsSync(path.resolve(__dirname, "../../../.github/workflows/headless-smoke.yml")));
  });

  it("docs_no_reject_unauthorized_false", () => {
    assert.doesNotMatch(md, /rejectUnauthorized/);
    assert.doesNotMatch(md, /NODE_TLS_REJECT_UNAUTHORIZED=0/);
  });
});
