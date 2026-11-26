"use strict";
// wiring-e2e (nan-024 AC-01/04/07, R-17/18/26/34/37/38/39): init -> read the WRITTEN command ->
// run it through the real harness shell(s) -> assert the stub endpoint saw the observation.
// Cloud attach (works on ubuntu/macos/windows); HOME has a space; HTTPS stub + NODE_EXTRA_CA_CERTS.
const { describe, it, before, after } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const H = require("./helpers/e2e-harness.js");

const cert = H.loadCert();
const skip = !cert && "openssl unavailable and E2E_CERT_DIR unset";
const norm = (p) => p.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

describe("wiring-e2e: cloud attach through the literal written command", { skip }, () => {
  let stub, f, env, init;
  before(async () => {
    stub = await H.startStub(cert);
    f = H.makeFixture();
    env = H.baseEnv(f, cert, { CODEX_HOME: f.codex });
    init = await H.runInit(f, env, ["--bundle", H.bundleFor(stub, cert)]);
  });
  after(async () => {
    if (stub) await stub.close();
    if (f) f.cleanup();
  });

  it("init succeeds into a home containing a space and pings the stub", () => {
    assert.equal(init.code, 0, init.out + init.err);
    assert.ok(f.home.includes(" "));
    assert.ok(stub.requests.some((r) => /"Ping"/.test(r.body)), "init pinged the endpoint");
  });

  it("Claude project key is the realpath spelling (R-38)", () => {
    const j = JSON.parse(fs.readFileSync(path.join(f.home, ".claude.json"), "utf8"));
    const keys = Object.keys(j.projects || {});
    assert.equal(keys.length, 1, JSON.stringify(keys));
    assert.equal(norm(keys[0]), norm(f.proj), "key spelling: " + keys[0]);
  });

  for (const [label, file, provider] of [
    ["claude-code settings.local.json", () => path.join(f.proj, ".claude", "settings.local.json"), null],
    ["codex-cli $CODEX_HOME/hooks.json (D-081)", () => path.join(f.codex, "hooks.json"), "codex-cli"],
  ]) {
    it("literal " + label + " command runs through every harness shell and the observation arrives", async () => {
      const line = H.hookCommand(file(), "PostToolUse");
      assert.ok(line.includes(" hook PostToolUse"), line);
      if (provider) assert.ok(line.includes("--provider " + provider), line);
      let n = 0;
      for (const sh of H.shells()) {
        const sid = "e2e-" + (provider || "claude") + "-" + n++;
        const before_ = stub.events().length;
        const r = await H.runLine(sh, line, f.proj, env, H.event(f, sid));
        assert.equal(r.code, 0, sh.name + ": " + r.err + r.out);
        assert.equal(r.out, "", sh.name + " stdout must be empty for an FNF event");
        const got = stub.events().slice(before_).filter((e) => e.body.includes(sid));
        assert.ok(got.length >= 1, sh.name + ": no observation for " + sid + "; stderr=" + r.err);
        assert.ok(got.every((e) => e.auth === "Bearer " + "a".repeat(64)), sh.name + ": auth header");
      }
    });
  }

  it("non-Unimatrix cwd: exit 0, no stdout, no stderr, no request (R-34/R-39)", async () => {
    const line = H.hookCommand(path.join(f.codex, "hooks.json"), "PostToolUse");
    const e0 = stub.requests.length;
    for (const sh of H.shells()) {
      const r = await H.runLine(sh, line, f.other, env, JSON.stringify({ session_id: "e2e-other", hook_event_name: "PostToolUse", cwd: f.other }));
      assert.equal(r.code, 0, sh.name + ": " + r.err);
      assert.equal(r.out, "", sh.name);
      assert.equal(r.err, "", sh.name + " stderr must be silent");
    }
    assert.equal(stub.requests.length, e0, "no network for a non-project cwd");
  });

  it("written MCP entry starts and answers initialize (first byte '{')", async () => {
    const j = JSON.parse(fs.readFileSync(path.join(f.home, ".claude.json"), "utf8"));
    const entry = j.projects[Object.keys(j.projects)[0]].mcpServers.unimatrix;
    if (H.WIN) {
      // D-072/D-047: recorded node + shim-entry.js mcp-stdio, no cmd wrapper.
      const inst = JSON.parse(fs.readFileSync(path.join(f.home, ".unimatrix", "install.json"), "utf8"));
      assert.equal(norm(entry.command), norm(inst.node));
      assert.equal(path.basename(entry.args[0]), "shim-entry.js");
      assert.equal(entry.args[1], "mcp-stdio");
    }
    const r = await H.mcpInitialize(entry, f.proj, env);
    assert.ok(r.out.startsWith("{"), "stdout first byte: " + JSON.stringify(r.out.slice(0, 80)) + " err=" + r.err);
    assert.ok(JSON.parse(r.out.split("\n")[0]).result, r.out);
  });

  it("mcp-stdio outside a project exits 0 with empty stdout and stderr", async () => {
    const j = JSON.parse(fs.readFileSync(path.join(f.home, ".claude.json"), "utf8"));
    const entry = j.projects[Object.keys(j.projects)[0]].mcpServers.unimatrix;
    const r = await H.run(entry.command, entry.args || [], { spawn: { cwd: f.other, env: Object.assign({}, env, entry.env || {}) }, input: "" });
    assert.equal(r.code, 0, r.err);
    assert.equal(r.out, "");
    assert.equal(r.err, "");
  });
});
