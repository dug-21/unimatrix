## ADR-006: ts_millis Unit Mismatch — Explicit Contract in Store API

### Context

SR-02: `observations.ts_millis` stores millisecond-epoch integers.
`query_log.ts` stores second-epoch integers. The old `query_phase_freq_table`
query used `q.ts > strftime('%s', 'now') - ?1 * 86400` (second-epoch arithmetic
with no scaling). The new Query A must use `ts_millis` with a `* 1000` scale
factor on the right-hand side.

A documentation-only fix is insufficient (as the risk assessment recommends):
if the `* 1000` is omitted, the query silently accepts it and returns rows
from only the last ~50 seconds of history (for a 30-day window: 30 days = 2,592,000
seconds; without scaling, the comparison is `ts_millis > <seconds_cutoff>` which
passes for all rows newer than a Unix timestamp interpreted as milliseconds,
approximately January 1970 + 2,592,000 milliseconds = January 30, 1970).
The error is both silent and inverted — the query returns far more rows than
expected, not fewer, making it hard to detect from result size alone.

The risk assessment recommends making the unit difference explicit in the store
API signature or adding an assertion.

Three approaches:

**Option A (documentation only):** Add a comment in the SQL. Provides no
runtime protection.

**Option B (typed parameter):** Define a newtype `LookbackDays(u32)` and a
`to_ts_millis_cutoff()` method that encapsulates the `* 1000` scaling. Prevents
accidental raw `u32` use.

**Option C (constant + doc comment):** Define a named constant
`MILLIS_PER_DAY: i64 = 86_400 * 1_000` and use it in the SQL binding expression.
Add a `#[doc]` comment asserting the ms-epoch contract. Provides readability
and a single named location for the scaling constant.

### Decision

Use Option C. Define `MILLIS_PER_DAY: i64 = 86_400 * 1_000` as a module-level
constant in the query_log.rs (or phase_freq.rs) module. Use it in the lookback
boundary computation:

```rust
/// `MILLIS_PER_DAY` is 86,400,000 — used to convert lookback_days to a
/// ts_millis (millisecond-epoch) cutoff. observations.ts_millis is ms-epoch;
/// query_log.ts is s-epoch. Do NOT use 86_400 here.
const MILLIS_PER_DAY: i64 = 86_400 * 1_000;

// In the query binding:
let cutoff_millis: i64 = {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    now_secs * 1_000 - (lookback_days as i64) * MILLIS_PER_DAY
};
```

Bind `cutoff_millis` as `i64` and use the SQL predicate `o.ts_millis > ?1`
(where `?1 = cutoff_millis`). This moves the arithmetic out of SQL into
verified Rust, eliminating any ambiguity about which side carries the scale
factor.

The SQL predicate becomes:

```sql
AND o.ts_millis > ?2
```

where `?2` is the pre-computed `cutoff_millis: i64`.

Option B (newtype) is deferred: introducing a new type would add complexity
across the call chain and is not warranted for a single use case. The
constant + comment approach is sufficient to make the unit difference visible
to reviewers and to create a named point of failure if the constant is ever
incorrectly changed.

### Consequences

- The `* 1000` scaling is expressed as named Rust constant `MILLIS_PER_DAY`,
  not an implicit SQL multiplier.
- Changing the constant to the wrong value (`86_400` instead of `86_400_000`)
  would produce the 1000× error — the constant name makes this self-evidently
  wrong.
- The SQL predicate `o.ts_millis > ?2` is simpler than
  `o.ts_millis > (strftime('%s', 'now') - ?1 * 86400) * 1000` — easier to
  read and less prone to operator-precedence surprises.
- The lookback computation is testable in Rust without a DB fixture.
