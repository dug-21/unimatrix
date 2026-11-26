"use strict";

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { describe, it, beforeEach, afterEach } = require("node:test");
const { resolveRuntime, RuntimeResolutionError } = require("../lib/resolve-runtime.js");

const SELF = path.resolve(__dirname, "..");

function mkClient(dir) {
  fs.mkdirSync(path.join(dir, "lib", "hook-client"), { recursive: true });
  fs.writeFileSync(path.join(dir, "lib", "hook-client", "index.js"), "");
  return dir;
}
function mkBin(file, mode = 0o755) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, "#!/bin/sh\n", { mode });
  fs.chmodSync(file, mode);
  return file;
}

describe("resolve-runtime", () => {
  let home, uh, pkg, instBin, envBin, redBin, redClient;
  const base = (o) => ({ env: {}, home, platform: "linux", arch: "x64", transport: "local", ...o });
  const writeRedirect = (o) => fs.writeFileSync(path.join(uh, "redirect.json"), typeof o === "string" ? o : JSON.stringify(o));

  beforeEach(() => {
    home = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uni-rr-")));
    uh = path.join(home, ".unimatrix");
    fs.mkdirSync(uh);
    pkg = mkClient(path.join(uh, "pkg", "1.0.0"));
    instBin = mkBin(path.join(pkg, "node_modules", "@dug-21", "unimatrix-linux-x64", "bin", "unimatrix"));
    fs.writeFileSync(
      path.join(pkg, "node_modules", "@dug-21", "unimatrix-linux-x64", "package.json"),
      '{"name":"@dug-21/unimatrix-linux-x64","version":"1.0.0"}'
    );
    fs.writeFileSync(path.join(uh, "install.json"), JSON.stringify({ version: "1.0.0", packageRoot: pkg, node: process.execPath }));
    envBin = mkBin(path.join(home, "env", "unimatrix"));
    redBin = mkBin(path.join(home, "red", "unimatrix"));
    redClient = mkClient(path.join(home, "redclient"));
  });
  afterEach(() => fs.rmSync(home, { recursive: true, force: true }));

  const code = (c, source) => (e) => e instanceof RuntimeResolutionError && e.code === c && (!source || e.source === source) && typeof e.path === "string" && e.message.includes(e.path);

  it("install only: binary and client from install", () => {
    const r = resolveRuntime(base());
    assert.deepStrictEqual(r, { binary: fs.realpathSync(instBin), clientRoot: pkg, binarySource: "install", clientSource: "install" });
  });
  it("env only: binary env, client install (R-05)", () => {
    const r = resolveRuntime(base({ env: { UNIMATRIX_BINARY: envBin } }));
    assert.strictEqual(r.binary, fs.realpathSync(envBin));
    assert.strictEqual(r.binarySource, "env");
    assert.strictEqual(r.clientRoot, pkg);
    assert.strictEqual(r.clientSource, "install");
  });
  it("redirect only: both from redirect", () => {
    writeRedirect({ binary: redBin, client: redClient });
    const r = resolveRuntime(base());
    assert.strictEqual(r.binarySource, "redirect");
    assert.strictEqual(r.clientSource, "redirect");
    assert.strictEqual(r.clientRoot, redClient);
    assert.strictEqual(r.binary, fs.realpathSync(redBin));
  });
  it("env + redirect: binary env, client redirect", () => {
    writeRedirect({ binary: redBin, client: redClient });
    const r = resolveRuntime(base({ env: { UNIMATRIX_BINARY: envBin } }));
    assert.strictEqual(r.binarySource, "env");
    assert.strictEqual(r.clientSource, "redirect");
  });
  it("redirect with only binary: client from install", () => {
    writeRedirect({ binary: redBin });
    const r = resolveRuntime(base());
    assert.strictEqual(r.clientSource, "install");
    assert.strictEqual(r.binarySource, "redirect");
  });
  it("redirect with only client: binary from install package, not redirected client", () => {
    writeRedirect({ client: redClient });
    const r = resolveRuntime(base());
    assert.strictEqual(r.clientSource, "redirect");
    assert.strictEqual(r.binarySource, "install");
    assert.strictEqual(r.binary, fs.realpathSync(instBin));
  });
  it("redirect removal reverts to install", () => {
    writeRedirect({ client: redClient });
    fs.unlinkSync(path.join(uh, "redirect.json"));
    assert.strictEqual(resolveRuntime(base()).clientSource, "install");
  });
  it("redirect via symlink resolves to real binary", () => {
    const link = path.join(home, "red", "link");
    fs.symlinkSync(redBin, link);
    writeRedirect({ binary: link });
    assert.strictEqual(resolveRuntime(base()).binary, fs.realpathSync(redBin));
  });
  it("invalid redirect binary variants throw, never fall through (SR-20)", () => {
    const err = code("redirect_binary_invalid", "redirect");
    writeRedirect({ binary: path.join(home, "nope") });
    assert.throws(() => resolveRuntime(base()), err);
    writeRedirect({ binary: mkBin(path.join(home, "red", "noexec"), 0o644) });
    assert.throws(() => resolveRuntime(base()), err);
    writeRedirect({ binary: "relative/unimatrix" });
    assert.throws(() => resolveRuntime(base()), err);
    writeRedirect({ binary: path.join(home, "red") });
    assert.throws(() => resolveRuntime(base()), err);
  });
  it("redirect client lacking entry / relative throws", () => {
    const bare = path.join(home, "bare");
    fs.mkdirSync(bare);
    writeRedirect({ client: bare });
    assert.throws(() => resolveRuntime(base()), code("redirect_client_invalid", "redirect"));
    writeRedirect({ client: "rel/dir" });
    assert.throws(() => resolveRuntime(base()), code("redirect_client_invalid", "redirect"));
  });
  it("malformed redirect.json throws naming path", () => {
    for (const bad of ["{not json", "[]", "null", '{"binary":5}', '{"client":{}}']) {
      writeRedirect(bad);
      assert.throws(() => resolveRuntime(base()), code("redirect_malformed", "redirect"));
    }
    writeRedirect("{bad");
    assert.throws(() => resolveRuntime(base({ transport: "cloud", platform: "darwin" })), code("redirect_malformed"));
  });
  it("invalid env binary throws even when redirect valid", () => {
    writeRedirect({ binary: redBin });
    assert.throws(
      () => resolveRuntime(base({ env: { UNIMATRIX_BINARY: "/nonexistent/x" } })),
      (e) => code("env_binary_invalid", "env")(e) && e.message.includes("UNIMATRIX_BINARY points to non-existent file")
    );
  });
  it("empty UNIMATRIX_BINARY is ignored", () => {
    assert.strictEqual(resolveRuntime(base({ env: { UNIMATRIX_BINARY: "" } })).binarySource, "install");
  });
  it("env never affects client (D-024)", () => {
    const r = resolveRuntime(base({ env: { UNIMATRIX_BINARY: envBin, UNIMATRIX_CLIENT: redClient } }));
    assert.strictEqual(r.clientRoot, pkg);
  });
  it("cloud transport: no binary resolved, env invalid ignored", () => {
    const r = resolveRuntime(base({ transport: "cloud", env: { UNIMATRIX_BINARY: "/nonexistent" } }));
    assert.strictEqual(r.binary, null);
    assert.strictEqual(r.binarySource, null);
    assert.strictEqual(r.clientRoot, pkg);
  });
  it("darwin/win32: binary null, no throw, env ignored (R-15)", () => {
    for (const platform of ["darwin", "win32"]) {
      for (const transport of ["local", "cloud"]) {
        const r = resolveRuntime(base({ platform, transport, env: { UNIMATRIX_BINARY: "/nonexistent" } }));
        assert.strictEqual(r.binary, null);
        assert.strictEqual(r.clientSource, "install");
      }
    }
  });
  it("unresolvable binary names every source tried", () => {
    fs.rmSync(path.join(pkg, "node_modules"), { recursive: true });
    assert.throws(
      () => resolveRuntime(base()),
      (e) => code("binary_unresolvable")(e) && /UNIMATRIX_BINARY/.test(e.message) && /redirect/.test(e.message) && /install/.test(e.message)
    );
  });
  it("unsupported linux arch throws unsupported_platform_local", () => {
    assert.throws(() => resolveRuntime(base({ arch: "ia32" })), code("unsupported_platform_local"));
  });
  it("D-078: removing the original source package does not change install binary", () => {
    const src = mkClient(path.join(home, "npx-cache", "pkg"));
    assert.ok(src);
    fs.rmSync(path.join(home, "npx-cache"), { recursive: true });
    assert.strictEqual(resolveRuntime(base()).binary, fs.realpathSync(instBin));
  });
  it("no install: throws install_missing; allowSelf uses own package root", () => {
    fs.unlinkSync(path.join(uh, "install.json"));
    assert.throws(() => resolveRuntime(base({ transport: "cloud" })), code("install_missing", "install"));
    const r = resolveRuntime(base({ transport: "cloud", allowSelf: true }));
    assert.strictEqual(r.clientRoot, SELF);
    assert.strictEqual(r.clientSource, "install");
  });
  it("allowSelf never overrides redirect or env", () => {
    fs.unlinkSync(path.join(uh, "install.json"));
    writeRedirect({ client: redClient, binary: redBin });
    const r = resolveRuntime(base({ allowSelf: true, env: { UNIMATRIX_BINARY: envBin } }));
    assert.strictEqual(r.clientSource, "redirect");
    assert.strictEqual(r.binarySource, "env");
  });
  it("invalid install.json / packageRoot throws", () => {
    fs.writeFileSync(path.join(uh, "install.json"), "{bad");
    assert.throws(() => resolveRuntime(base()), code("install_invalid"));
    fs.writeFileSync(path.join(uh, "install.json"), JSON.stringify({ packageRoot: path.join(home, "gone") }));
    assert.throws(() => resolveRuntime(base()), code("install_invalid"));
    const bare = path.join(home, "bare");
    fs.mkdirSync(bare);
    fs.writeFileSync(path.join(uh, "install.json"), JSON.stringify({ packageRoot: bare }));
    assert.throws(() => resolveRuntime(base()), code("client_invalid"));
  });
  it("wrapper returns the same value as resolveRuntime().binary", () => {
    const saved = process.env.HOME;
    const savedBin = process.env.UNIMATRIX_BINARY;
    try {
      process.env.HOME = home;
      process.env.UNIMATRIX_BINARY = envBin;
      const { resolveBinary } = require("../lib/resolve-binary.js");
      const rt = resolveRuntime({ env: process.env, home: os.homedir(), platform: process.platform, arch: process.arch, transport: "local", allowSelf: true });
      assert.strictEqual(resolveBinary(), rt.binary);
    } finally {
      if (saved === undefined) delete process.env.HOME; else process.env.HOME = saved;
      if (savedBin === undefined) delete process.env.UNIMATRIX_BINARY; else process.env.UNIMATRIX_BINARY = savedBin;
    }
  });
});
