## ADR-002: S2 Safe SQL Construction via sqlx::QueryBuilder

### Context

S2 matches entry pairs that share ≥2 terms from a configurable vocabulary list. The
vocabulary is `Vec<String>` in `InferenceConfig`, with an arbitrary number of terms.
The number of CASE WHEN expressions in the SQL must match the number of vocabulary
terms at tick time.

This requires dynamic SQL construction — the query cannot be a static string literal.
Dynamic SQL carries SQL injection risk (SR-01): if vocabulary terms are interpolated
directly into the SQL string, a term containing `'`, `--`, or `; DROP TABLE` could
corrupt or destroy the database. The operator provides the vocabulary in `config.toml`,
which is controlled infrastructure — but defense in depth requires parameterized binding
regardless of trust level.

Three construction approaches were considered:

**Option A** — String interpolation with manual escaping: escape each term by replacing
`'` with `''` and interpolate into the SQL string. Fragile; escaping is error-prone
and not auditable by sqlx's type system.

**Option B** — `sqlx::QueryBuilder` with push_bind: build the query incrementally,
pushing each vocabulary term as a bound parameter via `.push_bind(term)`. sqlx
handles the `?N` placeholder substitution. The term value is never part of the SQL
string — only a parameter slot. This is the canonical sqlx approach for dynamic
parameterized queries.

**Option C** — Prepared statement with JSON: store the vocabulary as a JSON array in
a single parameter and use SQLite's `json_each()` to expand it at query time. Cleaner
for very large vocabularies, but requires SQLite compiled with JSON1 (standard in
modern SQLite, but adds a runtime assumption). Also harder to reason about query plan.

**Option B** is chosen. The vulnerability surface is vocabulary terms interpolated
as SQL literals — `sqlx::QueryBuilder::push_bind` eliminates this surface entirely.
The query plan is transparent. No new dependencies.

**S2 SQL structure (with N vocabulary terms):**

The outer query shape is a self-join on `entries`, computing a term-match score per
pair using CASE WHEN expressions built dynamically:

```sql
SELECT e1.id AS source_id,
       e2.id AS target_id,
       (CASE WHEN instr(lower(' ' || e1.content || ' ' || e1.title || ' '),
                        lower(' ' || ? || ' ')) > 0 THEN 1 ELSE 0 END
        + CASE WHEN instr(lower(' ' || e1.content || ' ' || e1.title || ' '),
                          lower(' ' || ? || ' ')) > 0 THEN 1 ELSE 0 END
        + ...                                           -- one per term in vocabulary
       ) AS e1_matches,
       -- identical expression for e2 --
       ...
FROM entries e1
JOIN entries e2 ON e2.id > e1.id
JOIN entries chk1 ON chk1.id = e1.id AND chk1.status != ?quarantine
JOIN entries chk2 ON chk2.id = e2.id AND chk2.status != ?quarantine
WHERE e1.status != ?quarantine AND e2.status != ?quarantine
HAVING (e1_matches_count + e2_matches_count) -- see note below
```

Because SQLite does not allow column aliases in HAVING directly in all versions,
the term-match counting expression is repeated in the outer WHERE or a subquery.
The recommended implementation uses a CTE or wraps the inner query:

```sql
SELECT source_id, target_id, shared_terms
FROM (
    SELECT e1.id AS source_id,
           e2.id AS target_id,
           (  CASE WHEN instr(...e1..., lower(' ' || ?1 || ' ')) > 0 THEN 1 ELSE 0 END
            + CASE WHEN instr(...e1..., lower(' ' || ?2 || ' ')) > 0 THEN 1 ELSE 0 END
            + ...
           ) AS s1_count,
           (  CASE WHEN instr(...e2..., lower(' ' || ?1 || ' ')) > 0 THEN 1 ELSE 0 END
            + ...
           ) AS s2_count
    FROM entries e1
    JOIN entries e2 ON e2.id > e1.id
    WHERE e1.status != ?quarantine AND e2.status != ?quarantine
) inner
WHERE (s1_count >= 1 AND s2_count >= 1 AND s1_count + s2_count >= 2)
   OR (s1_count + s2_count >= 2)  -- simplified: pair must total >=2 matches
ORDER BY (s1_count + s2_count) DESC
LIMIT ?cap
```

**Note on "shared terms" definition:** A term counts toward the pair if EITHER entry
mentions it. "Sharing ≥2 terms" means the union of matched terms across the pair is
≥2. This is `(terms_in_e1 + terms_in_e2) >= 2` where each term is counted once per
side — not per-side minimum. This matches the ASS-038 definition. The implementation
uses the simpler per-pair total threshold rather than requiring each side to have ≥1.

**Term matching:** Space-padded instr for word boundary semantics:
`instr(lower(' ' || content || ' ' || title || ' '), lower(' ' || term || ' ')) > 0`
Eliminates false positives (e.g., "api" matching "capabilities"). Terms are bound
parameters — never concatenated into the SQL string.

**Weight formula:** `shared_term_count * 0.1` (capped at 1.0 in Rust after fetch).
Same approximation as S1 (SCOPE.md §Design Decision 2). `shared_term_count` is
`s1_count + s2_count` from the query.

**Early return when vocabulary is empty:**
```rust
if config.s2_vocabulary.is_empty() {
    tracing::info!(edges_written = 0, "s2 tick complete (vocabulary empty, no-op)");
    return;
}
```
This satisfies AC-06 and avoids constructing a syntactically invalid SQL query
(a SELECT with zero CASE WHEN expressions).

### Decision

Implement S2 SQL using `sqlx::QueryBuilder` with `.push_bind(term)` for every
vocabulary term. No vocabulary term is ever interpolated as a SQL string literal.
The construction loop is:

```rust
let mut qb = sqlx::QueryBuilder::new("SELECT ... FROM (...\n");
for term in &config.s2_vocabulary {
    qb.push("CASE WHEN instr(lower(' ' || e1.content || ' ' || e1.title || ' '), lower(' ' || ");
    qb.push_bind(term.as_str());
    qb.push(" || ' ')) > 0 THEN 1 ELSE 0 END +\n");
    // repeat for e2
}
qb.push(") WHERE ... ORDER BY ... LIMIT ");
qb.push_bind(config.max_s2_edges_per_tick as i64);
```

A code comment at the construction site documents the injection risk and its
mitigation: "SECURITY: vocabulary terms are push_bind parameters, never interpolated".

The dual-endpoint quarantine guard uses a bound parameter for `Status::Quarantined as i64`.

### Consequences

Easier: SQL injection via operator vocabulary is structurally impossible — terms are
parameter values, not SQL fragments. The mitigation is not a convention or review item;
it is enforced by the API.

Harder: The SQL construction is more verbose than a static string. The query plan
changes with vocabulary size — a vocabulary of 20 terms produces 40 CASE WHEN
expressions per row pair, which may be slow for large corpora. The `max_s2_edges_per_tick`
LIMIT cap bounds the total output, but does not bound the intermediate pair scan.
For corpus sizes above ~5,000 entries, a future optimization (FTS5 index or a
pre-materialized term-presence bitmap) may be needed. That is out of scope for crt-041.
