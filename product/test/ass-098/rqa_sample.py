#!/usr/bin/env python3
"""Reverse-QA step 1 — sample Active entries E from the paired snapshot, stratified by category.

Targets are Active-only (status=0). Deprecated(1)/Quarantined(3) entries stay IN the corpus as
distractors (they are searchable and can be returned) but are never sought as targets — they
should recede, not be surfaced. Writes entries.jsonl (id,title,content,category).

The per-category weights are relative; they are scaled to the requested -n so the sampler works
at n=3 (smoke) through n>=150 (gate panels). Availability caps each category; any shortfall is
back-filled from the remaining Active pool to hit -n exactly.
"""
import argparse
import json
import sqlite3
import random
from collections import Counter

# relative stratification weights across category (spread; small categories represented, not swamped)
WEIGHTS = {"decision": 20, "pattern": 18, "lesson-learned": 16,
           "procedure": 10, "capability": 8, "convention": 6, "goal": 6}


def main():
    ap = argparse.ArgumentParser(description="Sample Active entries as reverse-QA targets.")
    ap.add_argument("--db", required=True, help="Paired snapshot SQLite (unimatrix snapshot).")
    ap.add_argument("--out", required=True, help="Output entries JSONL.")
    ap.add_argument("-n", type=int, default=80, help="Number of targets to sample (default 80).")
    ap.add_argument("--seed", type=int, default=20260711, help="RNG seed (default 20260711).")
    ap.add_argument("--min-content-len", type=int, default=150,
                    help="Skip entries whose content is shorter than this (default 150).")
    args = ap.parse_args()

    rng = random.Random(args.seed)
    con = sqlite3.connect(args.db)
    con.row_factory = sqlite3.Row

    # gather the full Active pool per weighted category
    pool = {}
    for cat in WEIGHTS:
        rows = con.execute(
            "SELECT id,title,content,category FROM entries "
            "WHERE status=0 AND category=? AND length(content)>=?",
            (cat, args.min_content_len)).fetchall()
        rows = [dict(r) for r in rows]
        rng.shuffle(rows)
        pool[cat] = rows

    # scale weights -> per-category target counts summing to n
    total_w = sum(WEIGHTS.values())
    quota = {c: round(args.n * w / total_w) for c, w in WEIGHTS.items()}

    picked, leftover = [], []
    for cat, rows in pool.items():
        take = min(quota[cat], len(rows))
        picked.extend(rows[:take])
        leftover.extend(rows[take:])

    # trim or back-fill to exactly n
    rng.shuffle(picked)
    if len(picked) > args.n:
        picked = picked[:args.n]
    elif len(picked) < args.n:
        rng.shuffle(leftover)
        picked.extend(leftover[:args.n - len(picked)])
    rng.shuffle(picked)

    with open(args.out, "w") as f:
        for r in picked:
            f.write(json.dumps(r) + "\n")
    print(f"sampled {len(picked)} targets -> {args.out}; category spread:",
          dict(Counter(r["category"] for r in picked)))


if __name__ == "__main__":
    main()
