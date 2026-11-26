"use strict";
// tmp + rename write that never changes an existing file's permission bits.
const fs = require("fs");
const path = require("path");

/** Existing file's mode (& 0o777) or null. Never throws. */
function existingMode(file) {
  try {
    return fs.statSync(file).mode & 0o777;
  } catch (_) {
    return null;
  }
}

/**
 * Replace `file` atomically. Existing file: its mode is preserved. New file: `newMode`
 * (user-level config callers pass 0o600; default 0o644). tmp removed on failure.
 */
function atomicWrite(file, text, newMode) {
  const existing = existingMode(file);
  const mode = existing !== null ? existing : newMode || 0o644;
  const tmp = file + "." + process.pid + ".tmp";
  try {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(tmp, text, { encoding: "utf8", mode });
    fs.chmodSync(tmp, mode); // umask-proof
    fs.renameSync(tmp, file);
  } catch (e) {
    try { fs.unlinkSync(tmp); } catch (_) { /* best effort */ }
    throw e;
  }
}

module.exports = { atomicWrite, existingMode };
