//! Fixture-corpus loader (nan-018, ADR-004).
//!
//! Materializes hand-authored fixture entry-graphs (TOML) into a snapshot DB
//! the EXISTING `EvalServiceLayer::from_profile(db_path, ..)` consumes unchanged
//! (corpus = just another snapshot source — cumulative test infra), and produces
//! an [`AliasMap`] (alias -> resolved id + per-head member sets) for trust
//! evaluation.
//!
//! Load-bearing invariants (nan-018):
//! - **R-09 / C-04** — the primary corpus carries property assertions ONLY; a
//!   literal-id `expected` or a null ground truth is a HARD error.
//! - **R-10** — alias uniqueness is enforced; any assertion / `superseded_by`
//!   reference to an undefined alias is a HARD error, never a silent vacuous pass.
//! - **Security** — author-supplied file references are path-traversal checked;
//!   the DB is materialized only under the caller-controlled `target_db` path.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;
use unimatrix_core::{EntryRecord, Status};
use unimatrix_engine::graph::{build_typed_relation_graph, find_terminal_active};
use unimatrix_store::pool_config::PoolConfig;

use super::assertions::{RawEntry, RawFixture, RawScenario, safe_join};
use crate::eval::scenarios::{EntryRef, ExpectedAssertions, ScenarioRecord};

/// First entry id assigned to a fixture entry. Chosen away from 0/1 so renumber
/// tests can offset it without colliding with reserved ids.
const BASE_ID: u64 = 1_000;

// ---------------------------------------------------------------------------
// AliasMap
// ---------------------------------------------------------------------------

/// Alias -> resolved id and per-head member sets, produced at load.
///
/// Resolution is *total* at evaluation time precisely because the loader proved
/// every referenced alias exists (R-10) — no path degrades to a silent vacuous
/// pass.
#[derive(Debug, Clone, Default)]
pub struct AliasMap {
    alias_to_id: BTreeMap<EntryRef, u64>,
    head_members: BTreeMap<EntryRef, BTreeSet<u64>>,
}

impl AliasMap {
    /// Resolve an alias to its entry id.
    ///
    /// Total at evaluation time: load (steps 3/5) guaranteed existence of every
    /// alias an assertion can reference. The `Option` is returned only to keep
    /// the accessor panic-free for non-assertion callers; assertion evaluation
    /// resolves aliases the loader already validated.
    pub fn resolve(&self, r: &str) -> Option<u64> {
        self.alias_to_id.get(r).copied()
    }

    /// Superseded predecessor ids whose terminal-active resolves to `head`.
    ///
    /// Empty set for a head with no precomputed members (returned by reference
    /// to a shared empty set so the accessor is total and allocation-free).
    pub fn head_members(&self, head: &str) -> &BTreeSet<u64> {
        static EMPTY: BTreeSet<u64> = BTreeSet::new();
        self.head_members.get(head).unwrap_or(&EMPTY)
    }

    /// Number of aliases in the map (test/diagnostic accessor).
    pub fn len(&self) -> usize {
        self.alias_to_id.len()
    }

    /// True when no aliases are loaded.
    pub fn is_empty(&self) -> bool {
        self.alias_to_id.is_empty()
    }

    /// Construct an `AliasMap` directly from resolved maps (TEST-ONLY seam).
    ///
    /// The production path builds an `AliasMap` only via [`load_fixture_corpus`],
    /// which validates alias existence (R-10). This constructor lets the trust
    /// evaluator's truth-table tests assemble a map without materializing a DB.
    #[cfg(test)]
    pub fn for_test(
        alias_to_id: BTreeMap<EntryRef, u64>,
        head_members: BTreeMap<EntryRef, BTreeSet<u64>>,
    ) -> Self {
        AliasMap {
            alias_to_id,
            head_members,
        }
    }
}

// ---------------------------------------------------------------------------
// LoadedCorpus
// ---------------------------------------------------------------------------

/// Output of [`load_fixture_corpus`]: a materialized snapshot DB + the alias map.
#[derive(Debug, Clone)]
pub struct LoadedCorpus {
    /// Path to the materialized snapshot DB; feeds `EvalServiceLayer::from_profile`.
    pub db_path: PathBuf,
    /// Alias -> id + head-member sets; feeds `evaluate_trust`.
    pub alias_map: AliasMap,
    /// Property-assertion scenarios parsed from the corpus (alias-authored).
    pub scenarios: Vec<ScenarioRecord>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Hard error conditions during corpus load — all fail loud, never silent.
#[derive(Debug)]
pub enum CorpusError {
    /// A primary-corpus scenario carries a literal-id `expected` list (R-09).
    LiteralIdExpected { scenario: String },
    /// A primary-corpus scenario has neither assertions nor `expected` (R-09).
    NullExpected { scenario: String },
    /// The same alias is defined more than once across the corpus (R-10).
    DuplicateAlias { alias: String },
    /// An assertion / `superseded_by` references an undefined alias (R-10).
    MissingAlias { alias: String },
    /// An author-supplied file reference escaped the controlled path.
    PathTraversal { reference: String },
    /// A fixture file could not be read or parsed.
    Parse { path: PathBuf, reason: String },
    /// Snapshot-DB materialization failed.
    Materialize { reason: String },
}

impl std::fmt::Display for CorpusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LiteralIdExpected { scenario } => write!(
                f,
                "scenario '{scenario}': literal-id `expected` is banned in the primary corpus \
                 (assert outcomes, never constants — C-04/R-09)"
            ),
            Self::NullExpected { scenario } => write!(
                f,
                "scenario '{scenario}': null ground truth (no assertions, no expected) is banned \
                 in the primary corpus (R-09)"
            ),
            Self::DuplicateAlias { alias } => {
                write!(
                    f,
                    "duplicate alias '{alias}' (aliases must be globally unique — R-10)"
                )
            }
            Self::MissingAlias { alias } => write!(
                f,
                "alias '{alias}' is referenced but never defined (R-10 — never a silent vacuous pass)"
            ),
            Self::PathTraversal { reference } => write!(
                f,
                "rejected path reference '{reference}' (absolute or `..` escapes the corpus dir)"
            ),
            Self::Parse { path, reason } => {
                write!(f, "failed to parse fixture '{}': {reason}", path.display())
            }
            Self::Materialize { reason } => write!(f, "snapshot materialization failed: {reason}"),
        }
    }
}

impl std::error::Error for CorpusError {}

// ---------------------------------------------------------------------------
// Loader pipeline
// ---------------------------------------------------------------------------

/// Load and materialize a fixture corpus from `dir` into `target_db`.
///
/// `target_db` is the caller-controlled materialization path; the loader writes
/// ONLY there (and its sibling vector dir). See module docs for the enforced
/// invariants.
pub async fn load_fixture_corpus(
    dir: &Path,
    target_db: &Path,
) -> Result<LoadedCorpus, CorpusError> {
    // 1. Parse all fixture TOML files under `dir` (path-traversal checked).
    let raw = parse_fixtures(dir)?;

    // 2. Reject forbidden `expected` forms in the PRIMARY corpus (R-09, C-04).
    for (idx, scenario) in raw.scenarios.iter().enumerate() {
        let name = scenario_name(scenario, idx);
        if scenario.has_literal_expected() {
            return Err(CorpusError::LiteralIdExpected { scenario: name });
        }
        if scenario.is_null_ground_truth() {
            return Err(CorpusError::NullExpected { scenario: name });
        }
    }

    // 3. Assign ids and enforce global alias uniqueness (R-10).
    let mut alias_to_id: BTreeMap<EntryRef, u64> = BTreeMap::new();
    for (offset, entry) in raw.entries.iter().enumerate() {
        if alias_to_id.contains_key(&entry.alias) {
            return Err(CorpusError::DuplicateAlias {
                alias: entry.alias.clone(),
            });
        }
        alias_to_id.insert(entry.alias.clone(), BASE_ID + offset as u64);
    }

    // 4. Resolve `superseded_by` alias refs to ids; build EntryRecord rows.
    //    `entry.superseded_by = ["Y"]` means alias Y supersedes THIS entry,
    //    i.e. the successor Y carries `supersedes = this.id` (edge this -> Y).
    let rows = build_entry_rows(&raw.entries, &alias_to_id)?;

    // 5. Validate every assertion alias resolves (R-10) — never a vacuous pass.
    for scenario in &raw.scenarios {
        if let Some(assertions) = &scenario.assertions {
            for aref in assertions.referenced_aliases() {
                if !alias_to_id.contains_key(aref) {
                    return Err(CorpusError::MissingAlias {
                        alias: aref.clone(),
                    });
                }
            }
        }
    }

    // 6. Materialize the snapshot DB under the controlled path (writes ONLY there).
    materialize_snapshot(target_db, &rows).await?;

    // 7. Precompute head-member sets via find_terminal_active semantics.
    let head_members = precompute_head_members(&raw.scenarios, &alias_to_id, &rows);

    // 8. Build alias-authored ScenarioRecords (literal `expected` always None here).
    let scenarios = build_scenario_records(&raw.scenarios);

    Ok(LoadedCorpus {
        db_path: target_db.to_path_buf(),
        alias_map: AliasMap {
            alias_to_id,
            head_members,
        },
        scenarios,
    })
}

// ---------------------------------------------------------------------------
// Stage helpers
// ---------------------------------------------------------------------------

/// Parse every `*.toml` fixture file directly under `dir`.
///
/// Each directory entry's file name is path-traversal checked against `dir`
/// before it is read (defense in depth — file names from `read_dir` are already
/// single components, but the check also covers any future author-supplied
/// reference form).
fn parse_fixtures(dir: &Path) -> Result<RawFixture, CorpusError> {
    let read = std::fs::read_dir(dir).map_err(|e| CorpusError::Parse {
        path: dir.to_path_buf(),
        reason: e.to_string(),
    })?;

    let mut merged = RawFixture {
        entries: Vec::new(),
        scenarios: Vec::new(),
    };

    // Deterministic order: sort file names so id assignment is reproducible
    // across runs/filesystems (renumber-survival depends only on alias, but a
    // stable order keeps materialization deterministic).
    let mut files: Vec<PathBuf> = Vec::new();
    for dirent in read {
        let dirent = dirent.map_err(|e| CorpusError::Parse {
            path: dir.to_path_buf(),
            reason: e.to_string(),
        })?;
        let name = dirent.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".toml") {
            continue;
        }
        // Path-traversal guard on the (author-controlled) reference.
        let safe = safe_join(dir, &name).map_err(|t| CorpusError::PathTraversal {
            reference: t.reference,
        })?;
        files.push(safe);
    }
    files.sort();

    for path in files {
        let text = std::fs::read_to_string(&path).map_err(|e| CorpusError::Parse {
            path: path.clone(),
            reason: e.to_string(),
        })?;
        let fixture: RawFixture = toml::from_str(&text).map_err(|e| CorpusError::Parse {
            path: path.clone(),
            reason: e.to_string(),
        })?;
        merged.entries.extend(fixture.entries);
        merged.scenarios.extend(fixture.scenarios);
    }

    Ok(merged)
}

/// Build `EntryRecord` rows, resolving `superseded_by` alias refs to the
/// `supersedes`/`superseded_by` id columns the graph builder reads.
fn build_entry_rows(
    entries: &[RawEntry],
    alias_to_id: &BTreeMap<EntryRef, u64>,
) -> Result<Vec<EntryRecord>, CorpusError> {
    // First pass: base rows keyed by id, with default supersedes/superseded_by.
    let mut rows: BTreeMap<u64, EntryRecord> = BTreeMap::new();
    for entry in entries {
        let id = alias_to_id[&entry.alias];
        rows.insert(id, base_entry_record(id, entry));
    }

    // Second pass: wire Supersedes relationships.
    //   entry.superseded_by = ["Y"]  ==>  Y supersedes `entry`
    //     entry(old).superseded_by = Y.id
    //     Y(new).supersedes        = entry.id
    for entry in entries {
        let old_id = alias_to_id[&entry.alias];
        for succ_alias in &entry.superseded_by {
            let succ_id =
                *alias_to_id
                    .get(succ_alias)
                    .ok_or_else(|| CorpusError::MissingAlias {
                        alias: succ_alias.clone(),
                    })?;
            if let Some(old) = rows.get_mut(&old_id) {
                old.superseded_by = Some(succ_id);
            }
            if let Some(succ) = rows.get_mut(&succ_id) {
                succ.supersedes = Some(old_id);
            }
        }
    }

    Ok(rows.into_values().collect())
}

/// Construct a minimal `EntryRecord` for a fixture entry.
fn base_entry_record(id: u64, entry: &RawEntry) -> EntryRecord {
    EntryRecord {
        id,
        title: entry.title.clone(),
        content: entry.content.clone(),
        topic: String::new(),
        category: entry.category.clone(),
        tags: Vec::new(),
        source: "fixture-corpus".to_string(),
        status: parse_status(&entry.status),
        confidence: 0.0,
        created_at: 0,
        updated_at: 0,
        last_accessed_at: 0,
        access_count: 0,
        supersedes: None,
        superseded_by: None,
        correction_count: 0,
        embedding_dim: 0,
        created_by: String::new(),
        modified_by: String::new(),
        content_hash: String::new(),
        previous_hash: String::new(),
        version: 1,
        feature_cycle: String::new(),
        trust_source: String::new(),
        helpful_count: 0,
        unhelpful_count: 0,
        pre_quarantine_status: None,
    }
}

/// Parse an authored status string. Unknown values default to `Active` (the
/// fixture authoring guide constrains this to `Active`/`Deprecated`).
fn parse_status(s: &str) -> Status {
    match s {
        "Deprecated" => Status::Deprecated,
        "Proposed" => Status::Proposed,
        "Quarantined" => Status::Quarantined,
        _ => Status::Active,
    }
}

/// Precompute, per `redirect_to_head` alias, the set of superseded predecessor
/// ids whose terminal-active resolves to the head (graph.rs:547 semantics).
fn precompute_head_members(
    scenarios: &[RawScenario],
    alias_to_id: &BTreeMap<EntryRef, u64>,
    rows: &[EntryRecord],
) -> BTreeMap<EntryRef, BTreeSet<u64>> {
    // Collect the distinct redirect_to_head aliases referenced anywhere.
    let mut head_aliases: BTreeSet<&str> = BTreeSet::new();
    for scenario in scenarios {
        if let Some(a) = &scenario.assertions {
            for h in &a.redirect_to_head {
                head_aliases.insert(h.as_str());
            }
        }
    }
    if head_aliases.is_empty() {
        return BTreeMap::new();
    }

    // Build the typed graph once; a Supersedes cycle would be a fixture-author
    // bug — treat it as "no members" (the trust evaluator then fails loud on a
    // head-absent / unredirected member rather than panicking here).
    let graph = match build_typed_relation_graph(rows, &[]) {
        Ok(g) => g,
        Err(_) => return BTreeMap::new(),
    };

    let mut out: BTreeMap<EntryRef, BTreeSet<u64>> = BTreeMap::new();
    for head_alias in head_aliases {
        let head_id = match alias_to_id.get(head_alias) {
            Some(&id) => id,
            None => continue, // validated upstream; defensive
        };
        let mut members: BTreeSet<u64> = BTreeSet::new();
        for e in rows {
            if e.id == head_id {
                continue;
            }
            if find_terminal_active(e.id, &graph, rows) == Some(head_id) {
                members.insert(e.id);
            }
        }
        out.insert(head_alias.to_string(), members);
    }
    out
}

/// Build alias-authored `ScenarioRecord`s. `expected` is always `None` (the
/// loader rejected any literal-id authoring upstream).
fn build_scenario_records(scenarios: &[RawScenario]) -> Vec<ScenarioRecord> {
    use crate::eval::scenarios::ScenarioContext;
    scenarios
        .iter()
        .enumerate()
        .map(|(idx, s)| ScenarioRecord {
            id: scenario_name(s, idx),
            query: s.query.clone(),
            context: ScenarioContext {
                agent_id: String::new(),
                feature_cycle: "nan-018".to_string(),
                session_id: String::new(),
                retrieval_mode: "flexible".to_string(),
                phase: None,
            },
            baseline: None,
            source: "fixture-corpus".to_string(),
            expected: None,
            assertions: s
                .assertions
                .clone()
                .filter(|a: &ExpectedAssertions| !a.is_empty()),
        })
        .collect()
}

/// Stable name for a scenario (explicit id or positional `corpus-{idx}`).
fn scenario_name(s: &RawScenario, idx: usize) -> String {
    s.id.clone().unwrap_or_else(|| format!("corpus-{idx}"))
}

// ---------------------------------------------------------------------------
// Snapshot materialization
// ---------------------------------------------------------------------------

/// Materialize `rows` into a fresh snapshot DB at `target_db`.
///
/// Opens a writable `SqlxStore` (runs migrations → schema), then inserts each
/// entry via raw SQL with its explicit id / status / supersession columns. The
/// graph builder derives Supersedes edges from the `supersedes` column
/// (authoritative — graph.rs Pass 2a), so no `graph_edges` rows are needed.
async fn materialize_snapshot(target_db: &Path, rows: &[EntryRecord]) -> Result<(), CorpusError> {
    let store = unimatrix_store::SqlxStore::open(target_db, PoolConfig::default())
        .await
        .map_err(|e| CorpusError::Materialize {
            reason: e.to_string(),
        })?;

    let pool: &SqlitePool = store.write_pool_server();
    let mut max_id = 0u64;
    for row in rows {
        insert_entry_row(pool, row).await?;
        max_id = max_id.max(row.id);
    }

    // Keep the id counter ahead of the highest fixture id so any later write
    // through the normal path cannot collide with a fixture id.
    if !rows.is_empty() {
        // Counter name matches `counters::next_entry_id` (store-internal literal).
        unimatrix_store::counters::set_counter(pool, "next_entry_id", max_id)
            .await
            .map_err(|e| CorpusError::Materialize {
                reason: e.to_string(),
            })?;
    }

    Ok(())
}

/// Insert a single fixture `EntryRecord` with explicit columns.
async fn insert_entry_row(pool: &SqlitePool, row: &EntryRecord) -> Result<(), CorpusError> {
    sqlx::query(
        "INSERT INTO entries (id, title, content, topic, category, source,
            status, confidence, created_at, updated_at, last_accessed_at,
            access_count, supersedes, superseded_by, correction_count,
            embedding_dim, created_by, modified_by, content_hash,
            previous_hash, version, feature_cycle, trust_source,
            helpful_count, unhelpful_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11,
            ?12, ?13, ?14, ?15,
            ?16, ?17, ?18, ?19,
            ?20, ?21, ?22, ?23,
            ?24, ?25)",
    )
    .bind(row.id as i64)
    .bind(&row.title)
    .bind(&row.content)
    .bind(&row.topic)
    .bind(&row.category)
    .bind(&row.source)
    .bind(row.status as u8 as i64)
    .bind(row.confidence)
    .bind(row.created_at as i64)
    .bind(row.updated_at as i64)
    .bind(row.last_accessed_at as i64)
    .bind(row.access_count as i64)
    .bind(row.supersedes.map(|v| v as i64))
    .bind(row.superseded_by.map(|v| v as i64))
    .bind(row.correction_count as i64)
    .bind(row.embedding_dim as i64)
    .bind(&row.created_by)
    .bind(&row.modified_by)
    .bind(&row.content_hash)
    .bind(&row.previous_hash)
    .bind(row.version as i64)
    .bind(&row.feature_cycle)
    .bind(&row.trust_source)
    .bind(row.helpful_count as i64)
    .bind(row.unhelpful_count as i64)
    .execute(pool)
    .await
    .map_err(|e| CorpusError::Materialize {
        reason: e.to_string(),
    })?;
    Ok(())
}
