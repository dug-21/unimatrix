#!/usr/bin/env node
"use strict";
// D-078 / D-087: report-only size of ~/.unimatrix/pkg/<version> after init's stable copy.
// No threshold, no pass/fail. Fixture HOME only (NFR-05). Linux only.
// Platform binary: real release binary (UNIMATRIX_BINARY | ../../target/release/unimatrix) plus the two
// libonnxruntime copies release.yml ships (ORT_LIB_DIR | /usr/local/lib); missing pieces are reported.
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { installStore } = require("../lib/install-store");

function du(p) {
  let n = 0;
  const st = fs.lstatSync(p);
  if (!st.isDirectory()) return st.size;
  for (const e of fs.readdirSync(p)) n += du(path.join(p, e));
  return n;
}

function main() {
  if (process.platform !== "linux") return console.log("measure-pkg-size: linux only, skipped");
  const pkgRoot = path.resolve(__dirname, "..");
  const arch = process.arch === "arm64" ? "arm64" : "x64";
  const platDir = path.resolve(pkgRoot, "..", `unimatrix-linux-${arch}`);
  const bin = process.env.UNIMATRIX_BINARY || path.resolve(pkgRoot, "..", "..", "target", "release", "unimatrix");
  const ortDir = process.env.ORT_LIB_DIR || "/usr/local/lib";
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "u-size-"));
  const home = path.join(tmp, "home");
  const src = path.join(tmp, "src");
  const fixture = path.join(src, "node_modules", "@dug-21", `unimatrix-linux-${arch}`);
  const notes = [];
  try {
    fs.mkdirSync(home, { recursive: true });
    fs.cpSync(pkgRoot, src, { recursive: true, filter: (s) => !/[\\/](node_modules|test)([\\/]|$)/.test(path.relative(pkgRoot, s)) });
    fs.mkdirSync(path.join(fixture, "bin"), { recursive: true });
    fs.copyFileSync(path.join(platDir, "package.json"), path.join(fixture, "package.json"));
    if (fs.existsSync(bin)) fs.copyFileSync(bin, path.join(fixture, "bin", "unimatrix"));
    else notes.push(`binary not found at ${bin}`);
    for (const n of ["libonnxruntime.so", "libonnxruntime.so.1"]) {
      const f = path.join(ortDir, n);
      if (fs.existsSync(f)) fs.copyFileSync(f, path.join(fixture, "bin", n === "libonnxruntime.so" ? "libonnxruntime.so.1.20.1" : n));
      else notes.push(`ORT lib not found: ${f}`);
    }
    const proj = path.join(tmp, "proj");
    fs.mkdirSync(proj);
    const r = installStore({ home, platform: "linux", arch: process.arch, nodePath: process.execPath, sourcePackageRoot: src, projectRoot: proj, dryRun: false, requireBinary: true });
    const dest = path.join(home, ".unimatrix", "pkg", r.version);
    const sub = path.join(dest, "node_modules", "@dug-21", `unimatrix-linux-${arch}`);
    const total = du(dest);
    const plat = fs.existsSync(sub) ? du(sub) : 0;
    console.log(JSON.stringify({ version: r.version, pkgVersionDirBytes: total, platformBinarySubtreeBytes: plat, jsAndMetaBytes: total - plat, notes }, null, 2));
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}
main();
