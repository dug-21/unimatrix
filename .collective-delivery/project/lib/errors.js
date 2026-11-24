'use strict';

// C2 — composer error type. Carries a deterministic `state`, the offending `item`,
// and an exit `code` aligned to the col-005 CLI convention (OVERVIEW §"Cross-cutting
// conventions"):
//   1 negative / denied  (a compose refusal: byte-identity, register bypass,
//                         graded-value/determinism self-check, boundary violation)
//   2 usage-invalid
//   3 source-unavailable / unexpected
//
// A composer refusal fails LOUD and CLOSED: it opens no bundle and returns non-zero.
// C2 writes to no receiver and performs no merge.
class ComposeError extends Error {
  constructor(state, item, message, code = 1) {
    super(message);
    this.name = 'ComposeError';
    this.state = state;
    this.item = item === undefined || item === null ? null : String(item);
    this.code = code;
  }
}

module.exports = { ComposeError };
