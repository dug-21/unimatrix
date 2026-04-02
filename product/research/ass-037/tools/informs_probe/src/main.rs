//! Informs edge probe for ASS-037 Q3b synthetic graph test.
//!
//! Loads the eval harness snapshot and HNSW index, finds all entry pairs that
//! satisfy Phase 4b structural Informs criteria (no NLI), and writes SQL INSERTs
//! to populate GRAPH_EDGES in snapshot-synthetic.db.
//!
//! Phase 4b criteria applied:
//!   1. Both entries active (status=0) in snapshot
//!   2. Category pair is in informs_category_pairs
//!   3. Cross-feature: source.feature_cycle != target.feature_cycle
//!   4. Temporal: source.created_at < target.created_at
//!   5. Cosine >= 0.5 (raised from production 0.3 per Q7 recommendation)
//!   6. Target is within source's k=20 HNSW neighborhood (ef=32)

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::UNIX_EPOCH;

use anndists::dist::DistDot;
use hnsw_rs::hnswio;
use rusqlite::{Connection, params};

const COSINE_FLOOR: f32 = 0.5;
const K: usize = 20;
const EF: usize = 32;
const MIN_EDGES_FOR_TEST: usize = 50;

// Valid (source_category, target_category) Informs pairs — matches production config.
const INFORMS_PAIRS: &[(&str, &str)] = &[
    ("lesson-learned", "decision"),
    ("lesson-learned", "convention"),
    ("pattern", "decision"),
    ("pattern", "convention"),
];

// Source categories (left side of the pair)
const SOURCE_CATEGORIES: &[&str] = &["lesson-learned", "pattern"];

#[derive(Debug)]
struct EntryMeta {
    id: u64,
    category: String,
    feature_cycle: String,
    created_at: i64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Paths relative to workspace root (run from /workspaces/unimatrix)
    let snapshot_db = Path::new("product/research/ass-037/harness/snapshot.db");
    let vector_dir = Path::new("product/research/ass-037/harness/vector");

    // -------------------------------------------------------------------------
    // 1. Open snapshot and load entry metadata
    // -------------------------------------------------------------------------
    let conn = Connection::open(snapshot_db)?;

    let mut stmt = conn.prepare(
        "SELECT id, category, feature_cycle, created_at FROM entries WHERE status=0",
    )?;

    let entries: Vec<EntryMeta> = stmt
        .query_map([], |row| {
            Ok(EntryMeta {
                id: row.get::<_, i64>(0)? as u64,
                category: row.get(1)?,
                feature_cycle: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                created_at: row.get(3)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    let entry_map: HashMap<u64, &EntryMeta> = entries.iter().map(|e| (e.id, e)).collect();

    // -------------------------------------------------------------------------
    // 2. Load vector_map: entry_id -> hnsw_data_id
    // -------------------------------------------------------------------------
    let mut vm_stmt = conn.prepare("SELECT entry_id, hnsw_data_id FROM vector_map")?;
    let vector_map: HashMap<u64, usize> = vm_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)? as u64,
                row.get::<_, i64>(1)? as usize,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // -------------------------------------------------------------------------
    // 3. Load existing Informs edges to avoid duplicates
    // -------------------------------------------------------------------------
    let mut existing_stmt = conn.prepare(
        "SELECT source_id, target_id FROM graph_edges WHERE relation_type='Informs'",
    )?;
    let existing_informs: HashSet<(u64, u64)> = existing_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)? as u64,
                row.get::<_, i64>(1)? as u64,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    // -------------------------------------------------------------------------
    // 4. Load HNSW index
    // -------------------------------------------------------------------------
    // Read the basename from the metadata file
    let meta_path = vector_dir.join("unimatrix-vector.meta");
    let meta_content = std::fs::read_to_string(&meta_path)?;
    let basename = meta_content
        .lines()
        .find(|l| l.starts_with("basename="))
        .and_then(|l| l.split_once('='))
        .map(|(_, v)| v.trim().to_string())
        .ok_or("missing basename in metadata")?;

    eprintln!("Loading HNSW index: basename={basename}");
    let reloader = Box::leak(Box::new(hnswio::HnswIo::new(vector_dir, &basename)));
    let hnsw = reloader.load_hnsw::<f32, DistDot>()?;
    eprintln!("HNSW loaded: {} points", hnsw.get_nb_point());

    // -------------------------------------------------------------------------
    // 5. Build data_id -> Vec<f32> map from layer 0 (contains all points)
    // -------------------------------------------------------------------------
    let point_indexation = hnsw.get_point_indexation();
    let mut data_id_to_vec: HashMap<usize, Vec<f32>> = HashMap::new();
    for point in point_indexation.get_layer_iterator(0) {
        let origin_id = point.get_origin_id();
        let vec = point.get_v().to_vec();
        data_id_to_vec.insert(origin_id, vec);
    }
    eprintln!("Vectors extracted: {} in layer 0", data_id_to_vec.len());

    // -------------------------------------------------------------------------
    // 6. Find Informs candidate pairs
    // -------------------------------------------------------------------------
    let source_entries: Vec<&EntryMeta> = entries
        .iter()
        .filter(|e| SOURCE_CATEGORIES.contains(&e.category.as_str()))
        .collect();

    eprintln!(
        "Source entries (lesson-learned + pattern): {}",
        source_entries.len()
    );

    let mut candidate_pairs: Vec<(u64, u64, f32)> = Vec::new(); // (source_id, target_id, cosine)
    let mut seen_pairs: HashSet<(u64, u64)> = HashSet::new();
    let mut searched = 0usize;
    let mut no_vector = 0usize;

    for source in &source_entries {
        let Some(&data_id) = vector_map.get(&source.id) else {
            no_vector += 1;
            continue;
        };
        let Some(source_vec) = data_id_to_vec.get(&data_id) else {
            no_vector += 1;
            continue;
        };

        // Search k=20 neighbors (ef=32)
        let neighbors = hnsw.search(source_vec.as_slice(), K, EF);
        searched += 1;

        for neighbor in &neighbors {
            // distance = 1 - dot_product for DistDot on normalized vectors
            let cosine = 1.0f32 - neighbor.distance;
            if cosine < COSINE_FLOOR {
                continue;
            }

            let target_id = neighbor.d_id as u64;
            if target_id == source.id {
                continue;
            }

            let Some(target) = entry_map.get(&target_id) else {
                continue; // target not in active entries
            };

            // Phase 4b structural checks
            // 1. Category pair valid
            let pair = (source.category.as_str(), target.category.as_str());
            if !INFORMS_PAIRS.contains(&pair) {
                continue;
            }

            // 2. Cross-feature constraint
            if source.feature_cycle == target.feature_cycle && !source.feature_cycle.is_empty() {
                continue;
            }

            // 3. Temporal: source is older than target
            if source.created_at >= target.created_at {
                continue;
            }

            let pair_key = (source.id, target_id);
            if seen_pairs.contains(&pair_key) {
                continue;
            }
            if existing_informs.contains(&pair_key) {
                continue; // already exists
            }

            seen_pairs.insert(pair_key);
            candidate_pairs.push((source.id, target_id, cosine));
        }
    }

    // -------------------------------------------------------------------------
    // 7. Report and output
    // -------------------------------------------------------------------------
    eprintln!(
        "\nSummary:\n  Entries searched: {searched}\n  No-vector skips: {no_vector}\n  Candidate pairs: {}\n  Existing Informs edges (skipped): {}\n  Min threshold: {}",
        candidate_pairs.len(),
        existing_informs.len(),
        MIN_EDGES_FOR_TEST,
    );

    if candidate_pairs.len() < MIN_EDGES_FOR_TEST {
        eprintln!(
            "\nVERDICT: UNTESTABLE — only {} candidate pairs qualify (minimum {})",
            candidate_pairs.len(),
            MIN_EDGES_FOR_TEST
        );
        std::process::exit(2);
    }

    eprintln!("\nVERDICT: TESTABLE — {} candidate pairs qualify", candidate_pairs.len());

    // Output SQL INSERTs
    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    println!("-- ASS-037 Q3b: Synthetic Informs edges ({} pairs)", candidate_pairs.len());
    println!("-- Phase 4b structural criteria only (cosine >= {COSINE_FLOOR}, k={K})");
    println!("BEGIN TRANSACTION;");
    for (src, tgt, cosine) in &candidate_pairs {
        println!(
            "INSERT OR IGNORE INTO graph_edges (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only) VALUES ({src}, {tgt}, 'Informs', {cosine:.6}, {now}, 'ass037-probe', 'structural', 0);"
        );
    }
    println!("COMMIT;");
    println!("-- Total: {} Informs edges injected", candidate_pairs.len());

    Ok(())
}
