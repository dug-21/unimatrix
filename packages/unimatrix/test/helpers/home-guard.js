"use strict";
// NFR-05 guard: a test HOME/USERPROFILE must live under os.tmpdir() and never be the real home.
const fs = require("fs");
const os = require("os");
const path = require("path");

function real(p) {
  try { return fs.realpathSync.native(p); } catch (_e) { return path.resolve(p); }
}

function assertTmpHome(home) {
  if (typeof home !== "string" || !home) throw new Error("NFR-05: test HOME is not set");
  const h = real(home);
  const tmp = real(os.tmpdir());
  const rh = real(os.homedir());
  if (h === rh) throw new Error("NFR-05: test HOME is the real home directory");
  if (h !== tmp && !h.startsWith(tmp + path.sep)) throw new Error("NFR-05: test HOME is not under os.tmpdir()");
  return home;
}

module.exports = { assertTmpHome };
