'use strict';

// C2 — deterministic register-faithfulness check (ADR-018, #100). NO human judgment.
//
// The projected `contract/VOCABULARY.yaml` is a BYTE-IDENTICAL copy of the released
// file, so the kernel already sits at two registers exactly as the release cleared it:
//   - the six admitted-meaning terms carry their released meaning at the `admitted`
//     register (Program, Baseline, Release, registers, adopts, validates);
//   - Declaration/declares are carried at the RESERVED register (named, not admitted).
// C2 promotes nothing. This check RE-ASSERTS that structural invariant on the copied
// bytes and aborts (fail-loud) on any reserved->admitted or deferred->admitted bypass,
// or a schema-valid variant that omits a required admitted kernel term (lesson #140:
// catch the semantic omission at the entry point, not only downstream).
//
// This is the DETERMINISTIC confirmation that Declaration/declares stay reserved; it is
// NOT the meaning-preservation judgment (that is the human MP-REVIEW against the real
// receivers, recorded in the C8 ledger). Byte-identity proves the projected bytes ARE
// the cleared release bytes; it is never treated as proof of meaning-preservation.

const F = require('../../release/source/format');
const { ComposeError } = require('./errors');

// The six admitted-meaning kernel terms that MUST be present at the `admitted` register.
const ADMITTED_KERNEL = Object.freeze([
  'Program', 'Baseline', 'Release', 'registers', 'adopts', 'validates',
]);
// Terms that MUST be present at the `reserved` register and MUST NOT be admitted.
const RESERVED_TERMS = Object.freeze(['Declaration', 'declares']);

// checkRegisters(vocabularyBytes) -> void (throws ComposeError on any bypass/omission).
function checkRegisters(vocabularyBytes) {
  if (!Buffer.isBuffer(vocabularyBytes) || vocabularyBytes.length === 0) {
    throw new ComposeError('register-check-input', 'VOCABULARY.yaml',
      'projected VOCABULARY.yaml bytes are missing', 1);
  }
  let doc;
  try {
    doc = F.parseDocument(vocabularyBytes);
  } catch (e) {
    throw new ComposeError('register-check-input', 'VOCABULARY.yaml',
      `projected VOCABULARY.yaml is not a single strict document: ${e && e.message ? e.message : e}`, 1);
  }
  if (!doc || doc.schema !== 'collective.vocabulary/2' || !Array.isArray(doc.terms)) {
    throw new ComposeError('register-check-shape', 'VOCABULARY.yaml',
      'projected VOCABULARY.yaml is not a collective.vocabulary/2 term set', 1);
  }

  const byTerm = new Map();
  for (const t of doc.terms) {
    if (!t || typeof t.term !== 'string' || typeof t.register !== 'string') {
      throw new ComposeError('register-check-shape', 'VOCABULARY.yaml',
        'a vocabulary term is missing its `term` or `register`', 1);
    }
    byTerm.set(t.term, t.register);
  }

  // (a) every admitted-meaning kernel term present AND at the admitted register.
  for (const term of ADMITTED_KERNEL) {
    if (!byTerm.has(term)) {
      throw new ComposeError('admitted-term-absent', term,
        `required admitted-meaning kernel term "${term}" is absent from the projected vocabulary`, 1);
    }
    if (byTerm.get(term) !== 'admitted') {
      throw new ComposeError('admitted-term-demoted', term,
        `admitted-meaning kernel term "${term}" is at register "${byTerm.get(term)}", not "admitted"`, 1);
    }
  }

  // (b) reserved terms present AND reserved — never admitted (no reserved->admitted).
  for (const term of RESERVED_TERMS) {
    if (!byTerm.has(term)) {
      throw new ComposeError('reserved-term-absent', term,
        `reserved term "${term}" is absent from the projected vocabulary`, 1);
    }
    if (byTerm.get(term) !== 'reserved') {
      throw new ComposeError('reserved-term-promoted', term,
        `reserved term "${term}" is at register "${byTerm.get(term)}", not "reserved" — no mechanism promotes reserved->admitted (ADR-018)`, 1);
    }
  }

  // (c) no deferred/program-local term smuggled into the admitted register. Only the six
  // kernel terms and the four already-cleared release terms (Goal, Capability, Obligation,
  // Extension) are admitted by the release; anything else at `admitted` is a bypass.
  const ADMITTED_ALLOWED = new Set([
    ...ADMITTED_KERNEL, 'Goal', 'Capability', 'Obligation', 'Extension',
  ]);
  for (const [term, register] of byTerm) {
    if (register === 'admitted' && !ADMITTED_ALLOWED.has(term)) {
      throw new ComposeError('unexpected-admission', term,
        `term "${term}" is admitted but is not a released admitted term — deferred->admitted bypass (ADR-018)`, 1);
    }
  }
}

module.exports = { checkRegisters, ADMITTED_KERNEL, RESERVED_TERMS };
