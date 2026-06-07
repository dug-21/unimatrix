"use strict";

// AC-12 / C-04 payload size gate (vnc-026): the shipped hook client under
// lib/hook-client/ must stay under 100 KB. Portable (no `du`, which differs
// between GNU/BSD/Windows): sums the byte length of every .js file in the
// directory tree. Exits non-zero (fails CI) when the limit is exceeded — this
// is a real gate, not advisory.
//
// "100 KB" is interpreted as 100,000 bytes (decimal kilobytes), the stricter
// of the two common readings (100 KiB = 102,400); keeping the decimal reading
// gives a small safety margin for any future single-byte additions.

const fs = require("fs");
const path = require("path");

const LIMIT_BYTES = 100 * 1000; // 100 KB (decimal)
const ROOT = path.resolve(__dirname, "..", "lib", "hook-client");

function sumJsBytes(dir) {
  let total = 0;
  const rows = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      const sub = sumJsBytes(full);
      total += sub.total;
      rows.push(...sub.rows);
    } else if (entry.isFile() && entry.name.endsWith(".js")) {
      const size = fs.statSync(full).size;
      total += size;
      rows.push({ file: path.relative(ROOT, full), size });
    }
  }
  return { total, rows };
}

if (!fs.existsSync(ROOT)) {
  console.error("check-hook-client-size: directory not found: " + ROOT);
  process.exit(1);
}

const { total, rows } = sumJsBytes(ROOT);
rows.sort((a, b) => b.size - a.size);
for (const r of rows) {
  console.log(String(r.size).padStart(8) + "  " + r.file);
}
console.log("-".repeat(40));
console.log(
  "lib/hook-client/ total: " +
    total +
    " bytes (" +
    (total / 1000).toFixed(1) +
    " KB) — limit " +
    LIMIT_BYTES +
    " bytes (100 KB)"
);

if (total >= LIMIT_BYTES) {
  console.error(
    "FAIL: payload " + total + " bytes >= limit " + LIMIT_BYTES + " bytes (AC-12 / C-04)"
  );
  process.exit(1);
}
console.log("OK: payload within the 100 KB budget");
