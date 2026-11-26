"use strict";

// nan-024 D-052/D-053: headless env tier honoured only with UNIMATRIX_PROJECT_ROOT
// containing cwd. Real entry point; a plain TCP listener counts connection attempts.

const { describe, it, before, after } = require("node:test");
const assert = require("node:assert/strict");
const net = require("node:net");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawn } = require("node:child_process");

const BIN = path.resolve(__dirname, "../bin/unimatrix.js");
const DOC = path.resolve(__dirname, "../../../docs/client-setup.md");

describe("mcp-bridge headless root gate", () => {
  let srv, port, hits, tmp, root, real;
  before(async () => {
    hits = 0;
    srv = net.createServer((s) => { hits++; s.destroy(); });
    await new Promise((r) => srv.listen(0, "127.0.0.1", r));
    port = srv.address().port;
    tmp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uni-root-")));
    root = path.join(tmp, "proj");
    fs.mkdirSync(path.join(root, "sub"), { recursive: true });
    fs.mkdirSync(root + "-sibling");
    real = path.join(tmp, "link");
    fs.symlinkSync(root, real);
  });
  after(() => { srv.close(); fs.rmSync(tmp, { recursive: true, force: true }); });

  const run = (cwd, extra, opts) => {
    const home = path.join(tmp, "home");
    fs.mkdirSync(home, { recursive: true });
    const env = Object.assign({ PATH: process.env.PATH, HOME: home, USERPROFILE: home,
      UNIMATRIX_REMOTE_MCP_URL: "https://127.0.0.1:" + port + "/mcp", UNIMATRIX_REMOTE_TOKEN: "t" }, extra);
    // async: the in-process listener must keep accepting while the child runs
    return new Promise((resolve) => {
      const c = spawn(process.execPath, [BIN, "mcp-bridge"], { cwd, env, stdio: ["pipe", "pipe", "pipe"] });
      let stdout = "", stderr = "";
      c.stdout.on("data", (d) => { stdout += d; });
      c.stderr.on("data", (d) => { stderr += d; });
      c.stdin.on("error", () => {});
      c.stdin.write('{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}\n');
      c.on("exit", (status) => resolve({ status, stdout, stderr }));
      c.stdin.end();
      if (opts && opts.killAfter) setTimeout(() => c.kill("SIGKILL"), opts.killAfter);
    });
  };
  const silent = (r) => { assert.equal(r.status, 0); assert.equal(r.stdout, ""); assert.equal(r.stderr, ""); };

  for (const [name, cwd, extra] of [
    ["no_project_root", () => root, () => ({})],
    ["cwd_outside_root", () => tmp, () => ({ UNIMATRIX_PROJECT_ROOT: root })],
    ["sibling_prefix_cwd", () => root + "-sibling", () => ({ UNIMATRIX_PROJECT_ROOT: root })],
    ["empty_root", () => root, () => ({ UNIMATRIX_PROJECT_ROOT: "" })],
    ["relative_root", () => root, () => ({ UNIMATRIX_PROJECT_ROOT: "proj" })],
  ]) {
    it(name + " -> exit 0 silent, zero requests", async () => {
      const before = hits;
      silent(await run(cwd(), extra()));
      assert.equal(hits, before);
    });
  }

  it("cwd inside root (incl. via symlinked root) reaches the endpoint", async () => {
    for (const [cwd, r] of [[path.join(root, "sub"), root], [path.join(real, "sub"), root], [path.join(root, "sub"), real]]) {
      const before = hits;
      await run(cwd, { UNIMATRIX_PROJECT_ROOT: r }, { killAfter: 1500 });
      assert.ok(hits > before, "bridge attempted a request for " + cwd);
    }
  });

  it("docs list UNIMATRIX_PROJECT_ROOT as required", () => {
    assert.match(fs.readFileSync(DOC, "utf8"), /\| `UNIMATRIX_PROJECT_ROOT` \| Required with any headless variable/);
  });
});
