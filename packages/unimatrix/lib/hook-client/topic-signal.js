"use strict";

/**
 * Topic-signal extraction — pure port of attribution.rs:15-92 (read-only oracle).
 *
 * Priority chain (first match wins):
 *   1. extractFromPath         — `product/features/{id}/...`
 *   2. extractFeatureIdPattern — word-boundary feature-id tokens
 *      (structural filter: hyphen required, `[A-Za-z0-9\-_.]`, no digit requirement)
 *   3. extractFromGitCheckout  — `feature/{id}` in git commands
 *
 * Length checks are BYTE length (Buffer.byteLength) to match Rust str::len(),
 * never String.prototype.length (UTF-16 units).
 */

const MAX_FEATURE_ID_LEN = 128;

/**
 * Structural check for a plausible feature ID — port of
 * attribution.rs::is_valid_feature_id. Non-empty, byte length <= 128, contains a
 * hyphen, no leading/trailing hyphen, only `[A-Za-z0-9\-_.]`.
 */
function isValidFeatureId(s) {
  return (
    typeof s === "string" &&
    s !== "" &&
    Buffer.byteLength(s, "utf8") <= MAX_FEATURE_ID_LEN &&
    s.includes("-") &&
    !s.startsWith("-") &&
    !s.endsWith("-") &&
    /^[A-Za-z0-9\-_.]+$/.test(s)
  );
}

/**
 * Extract a feature ID from `product/features/{id}/...` — port of
 * attribution.rs::extract_from_path. Scans each marker left-to-right; the first
 * segment (to the next `/` or end) that validates wins.
 */
function extractFromPath(s) {
  const marker = "product/features/";
  let start = 0;
  for (;;) {
    const idx = s.indexOf(marker, start);
    if (idx === -1) {
      return null;
    }
    const after = idx + marker.length;
    const slash = s.indexOf("/", after);
    const segment = slash === -1 ? s.slice(after) : s.slice(after, slash);
    if (isValidFeatureId(segment)) {
      return segment;
    }
    start = after;
  }
}

/**
 * Unicode code points of `s` (matches Rust `char` iteration; the string iterator
 * yields code points). Helper documents intent.
 */
function codePoints(s) {
  return Array.from(s);
}

/** True iff `ch` (a single code point) is Unicode alphanumeric (\p{L} | \p{N}). */
function isUnicodeAlphanumeric(ch) {
  return /[\p{L}\p{N}]/u.test(ch);
}

/** True iff `ch` (a single code point) is Unicode whitespace. */
function isUnicodeWhitespace(ch) {
  return /\s/u.test(ch);
}

/**
 * Trim from both ends every code point that is NOT Unicode-alphanumeric and NOT
 * '-' — Rust `trim_matches(|c| !c.is_alphanumeric() && c != '-')`. Keeps interior
 * `-`/`_`/`.`; trims leading/trailing `_`/`.` etc.
 */
function trimToFeatureCandidate(word) {
  const cps = codePoints(word);
  let lo = 0;
  let hi = cps.length;
  const keep = (ch) => isUnicodeAlphanumeric(ch) || ch === "-";
  while (lo < hi && !keep(cps[lo])) {
    lo += 1;
  }
  while (hi > lo && !keep(cps[hi - 1])) {
    hi -= 1;
  }
  return cps.slice(lo, hi).join("");
}

/**
 * Extract a feature ID by word-boundary scan — port of
 * attribution.rs::extract_feature_id_pattern. Words split on Unicode whitespace
 * and `" ' ( )`; each trimmed to a candidate; first valid wins.
 */
function extractFeatureIdPattern(s) {
  // Split on Unicode whitespace OR " ' ( ) — mirrors the Rust closure.
  const words = s.split(/[\s"'()]/u);
  for (const word of words) {
    const candidate = trimToFeatureCandidate(word);
    if (isValidFeatureId(candidate)) {
      return candidate;
    }
  }
  return null;
}

/**
 * Extract a feature ID from a git checkout `feature/{id}` — port of
 * attribution.rs::extract_from_git_checkout. Takes code points after `feature/`
 * while Unicode-alphanumeric or '-'.
 */
function extractFromGitCheckout(s) {
  const idx = s.indexOf("feature/");
  if (idx === -1) {
    return null;
  }
  const rest = s.slice(idx + "feature/".length);
  const out = [];
  for (const ch of rest) {
    if (isUnicodeAlphanumeric(ch) || ch === "-") {
      out.push(ch);
    } else {
      break;
    }
  }
  const candidate = out.join("");
  return isValidFeatureId(candidate) ? candidate : null;
}

/**
 * Priority-chain topic-signal extraction — port of
 * attribution.rs::extract_topic_signal.
 */
function extractTopicSignal(text) {
  const t = typeof text === "string" ? text : "";
  const fromPath = extractFromPath(t);
  if (fromPath !== null) {
    return fromPath;
  }
  const fromPattern = extractFeatureIdPattern(t);
  if (fromPattern !== null) {
    return fromPattern;
  }
  return extractFromGitCheckout(t);
}

module.exports = {
  extractTopicSignal,
  isValidFeatureId,
  // exported for unit-test locality
  extractFromPath,
  extractFeatureIdPattern,
  extractFromGitCheckout,
  MAX_FEATURE_ID_LEN,
};
