// ass-095 ephemeral research harness — NOT product code.
// G4: reproduce aho-corasick vs regex::RegexSet scaling on this toolchain.
// G1/G3 sanity: run the curated lesson-sourced bank over the REAL failure corpus.
//
// The parent [workspace] is intentionally shadowed so this crate builds standalone.

use aho_corasick::AhoCorasick;
use regex::RegexSet;
use std::time::Instant;

fn gen_signatures(n: usize) -> Vec<String> {
    // Distinct literal signatures, shaped like real failure markers.
    (0..n)
        .map(|i| format!("mcp_error_-{:05}_agent_lacks_capability_class", i))
        .collect()
}

// ~4 MB scan corpus. Signatures are absent from most of it (worst case: full scan),
// with a handful planted near the tail so both engines must traverse the whole buffer.
fn build_corpus(sigs: &[String]) -> String {
    let unit = "the quick brown fox jumped over the lazy dog while cargo test ran and read failed ";
    let mut s = String::with_capacity(4 * 1024 * 1024);
    while s.len() < 4 * 1024 * 1024 {
        s.push_str(unit);
    }
    // plant a few real hits so match paths are exercised
    for sig in sigs.iter().take(3) {
        s.push_str(sig);
        s.push(' ');
    }
    s
}

fn throughput_mbps(bytes: usize, secs: f64) -> f64 {
    (bytes as f64 / (1024.0 * 1024.0)) / secs
}

fn bench_scaling() {
    let ns = [100usize, 1_000, 10_000, 50_000, 100_000];
    // RegexSet build is O(seconds) near 10k; cap the default-settings attempt to keep the
    // one-time run bounded. AhoCorasick is attempted at every N.
    let regexset_attempt_max = 10_000usize;

    println!("== G4 SCALING (this toolchain: aho-corasick 1.1.4, regex 1.12.3) ==");
    println!(
        "{:>8} | {:>12} | {:>14} | {:>14} | {:>16}",
        "N", "AC build ms", "AC scan MB/s", "RS build ms", "RS scan MB/s"
    );

    for &n in &ns {
        let sigs = gen_signatures(n);
        let corpus = build_corpus(&sigs);
        let cbytes = corpus.len();

        // ---- aho-corasick ----
        let t = Instant::now();
        let ac = AhoCorasick::new(&sigs).expect("AC build");
        let ac_build = t.elapsed().as_secs_f64() * 1e3;

        // warm + timed scans
        let mut hits = 0usize;
        let iters = 5;
        let t = Instant::now();
        for _ in 0..iters {
            hits += ac.find_iter(&corpus).count();
        }
        let ac_scan = throughput_mbps(cbytes * iters, t.elapsed().as_secs_f64());
        std::hint::black_box(hits);

        // ---- regex::RegexSet (default settings) ----
        let (rs_build, rs_scan) = if n <= regexset_attempt_max {
            let escaped: Vec<String> = sigs.iter().map(|s| regex::escape(s)).collect();
            let t = Instant::now();
            match RegexSet::new(&escaped) {
                Ok(rs) => {
                    let rs_build = t.elapsed().as_secs_f64() * 1e3;
                    let mut m = 0usize;
                    let t = Instant::now();
                    for _ in 0..iters {
                        m += rs.matches(&corpus).iter().count();
                    }
                    let rs_scan = throughput_mbps(cbytes * iters, t.elapsed().as_secs_f64());
                    std::hint::black_box(m);
                    (format!("{:.1}", rs_build), format!("{:.1}", rs_scan))
                }
                Err(e) => {
                    let ms = t.elapsed().as_secs_f64() * 1e3;
                    (format!("ERR@{:.0}ms", ms), format!("{:?}", e).chars().take(12).collect())
                }
            }
        } else {
            ("skipped(>10k)".to_string(), "skipped".to_string())
        };

        println!(
            "{:>8} | {:>12.1} | {:>14.1} | {:>14} | {:>16}",
            n, ac_build, ac_scan, rs_build, rs_scan
        );
    }
}

fn bench_real_bank() {
    // Curated lesson-sourced literal bank (G2): the runtime-failure classes actually
    // extractable from narrative lessons today. Literals only (aho-corasick domain).
    let bank: Vec<(&str, &str)> = vec![
        ("write_block_agent_id", "lacks Write capability"),
        ("write_block_agent_id2", "Agent 'anonymous'"),
        ("payload_too_large", "exceeds configured maximum"),
        ("rate_limit_overload", "overloaded_error"),
        ("rate_limit_overload2", "rate_limit_error"),
        ("mcp_invalid_param", "-32602"),
        ("mcp_write_denied", "-32003"),
        ("refusal", "I cannot"),
        ("refusal2", "I'm unable to"),
    ];
    let patterns: Vec<&str> = bank.iter().map(|(_, p)| *p).collect();

    let snippets = std::fs::read_to_string("failure_snippets.txt").unwrap_or_default();
    let lines: Vec<&str> = snippets.lines().collect();
    let total = lines.len();

    let ac = AhoCorasick::new(&patterns).expect("bank build");
    // Case-insensitive variant for refusal stems etc.
    let ac_ci = AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .build(&patterns)
        .expect("ci build");

    let t = Instant::now();
    let mut per_class = vec![0usize; patterns.len()];
    let mut matched_lines = 0usize;
    for l in &lines {
        let mut any = false;
        for m in ac_ci.find_iter(l) {
            per_class[m.pattern().as_usize()] += 1;
            any = true;
        }
        if any {
            matched_lines += 1;
        }
    }
    let elapsed = t.elapsed();
    std::hint::black_box(&ac);

    println!("\n== REAL failure corpus: curated lexical bank scoped to PostToolUseFailure ==");
    println!(
        "corpus = {} failure snippets ({} bytes); scan time = {:?} ({:.2} us/snippet)",
        total,
        snippets.len(),
        elapsed,
        elapsed.as_secs_f64() * 1e6 / total.max(1) as f64
    );
    println!("snippets with >=1 bank hit: {}/{}", matched_lines, total);
    for (i, (name, pat)) in bank.iter().enumerate() {
        if per_class[i] > 0 {
            println!("  {:>5}  {:<24} \"{}\"", per_class[i], name, pat);
        }
    }
}

fn main() {
    bench_real_bank();
    println!();
    bench_scaling();
}
