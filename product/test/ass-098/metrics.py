#!/usr/bin/env python3
"""Shared ranking + inter-rater metrics for the reverse-QA relevance odometer (ass-098).

Pure-function module, stdlib only (math, random, statistics, collections) — NO numpy/scipy.
Weighted kappa, binary kappa, and Spearman are hand-rolled so the harness has zero external
Python dependency. Imported by rqa_odometer.py, rqa_knownitem.py, rqa_calib_stab.py, rqa_scale.py.

Grades are integer relevance labels 0-3 in PIPELINE RANK ORDER (index 0 == rank 1). Metrics are
scored against the JUDGE ORACLE (the grade list), never against pipeline ids or scores.
"""
import math
import random
import statistics as st
import collections


# ---- graded ranking metrics (judge oracle) --------------------------------

def p_at_k(grades, thr=2, k=5):
    """Precision@k: fraction of the top-k with grade >= thr (relevant)."""
    top = grades[:k]
    return sum(1 for g in top if g >= thr) / k if k else 0.0


def mrr(grades, thr=2):
    """Reciprocal rank of the first grade >= thr."""
    for i, g in enumerate(grades):
        if g >= thr:
            return 1.0 / (i + 1)
    return 0.0


def dcg(grades, k=5):
    return sum((2 ** g - 1) / math.log2(i + 2) for i, g in enumerate(grades[:k]))


def ndcg(grades, k=5):
    """nDCG@k — the headline odometer metric. 1.0 == judge-ideal ordering."""
    idcg = dcg(sorted(grades, reverse=True), k)
    return dcg(grades, k) / idcg if idcg > 0 else 0.0


def bootstrap_ci(value_lists, fn, n=2000, seed=7):
    """Bootstrap 95% CI of mean(fn(g)) over a list of grade-lists."""
    rng = random.Random(seed)
    vals = []
    for _ in range(n):
        samp = [rng.choice(value_lists) for _ in value_lists]
        vals.append(st.mean(fn(g) for g in samp))
    vals.sort()
    return round(vals[int(0.025 * n)], 4), round(vals[int(0.975 * n)], 4)


# ---- inter-rater agreement (judge stability / calibration) ----------------

def linear_weighted_kappa(pairs, R=4):
    """Linear-weighted Cohen's kappa over (grade_a, grade_b) pairs on an R-point scale."""
    if not pairs:
        return None
    O = collections.Counter(pairs)
    ca = collections.Counter(p[0] for p in pairs)
    cb = collections.Counter(p[1] for p in pairs)
    n = len(pairs)

    def w(i, j):
        return 1 - abs(i - j) / (R - 1)

    po = sum(w(i, j) * O[(i, j)] for i in range(R) for j in range(R)) / n
    pe = sum(w(i, j) * ca.get(i, 0) * cb.get(j, 0) / n for i in range(R) for j in range(R)) / n
    return (po - pe) / (1 - pe) if pe != 1 else None


def binary_kappa(pairs, thr=2):
    """Cohen's kappa on binary relevance (grade >= thr)."""
    if not pairs:
        return None
    bp = [(1 if x >= thr else 0, 1 if y >= thr else 0) for x, y in pairs]
    n = len(bp)
    O = collections.Counter(bp)
    po = (O[(0, 0)] + O[(1, 1)]) / n
    a = collections.Counter(x for x, _ in bp)
    b = collections.Counter(y for _, y in bp)
    pe = sum(a.get(i, 0) * b.get(i, 0) / n / n for i in (0, 1))
    return (po - pe) / (1 - pe) if pe != 1 else None


def spearman(pairs):
    """Spearman rank correlation over (a, b) pairs, with tie-averaged ranks."""
    n = len(pairs)
    if n < 2:
        return None

    def rank(vals):
        order = sorted(range(len(vals)), key=lambda i: vals[i])
        r = [0.0] * len(vals)
        i = 0
        while i < len(vals):
            j = i
            while j + 1 < len(vals) and vals[order[j + 1]] == vals[order[i]]:
                j += 1
            avg = (i + j) / 2 + 1
            for k in range(i, j + 1):
                r[order[k]] = avg
            i = j + 1
        return r

    xa = rank([p[0] for p in pairs])
    xb = rank([p[1] for p in pairs])
    mx, my = st.mean(xa), st.mean(xb)
    num = sum((xa[i] - mx) * (xb[i] - my) for i in range(n))
    den = math.sqrt(sum((v - mx) ** 2 for v in xa) * sum((v - my) ** 2 for v in xb))
    return num / den if den else None
