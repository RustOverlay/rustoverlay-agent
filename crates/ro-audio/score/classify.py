"""Gunshot vs not, from the detector's own features, scored leave-one-session-out over the audio corpus.

Labels: a detection matched to a bot shot is 1; one within an own shot's window (-200..+100 ms) is
dropped (own-gun rejection is a separate problem); everything else is 0.
"""
import csv
import os
import sys
from bisect import bisect_left
from pathlib import Path

import numpy as np
from sklearn.linear_model import LogisticRegression
from sklearn.preprocessing import StandardScaler
from sklearn.tree import DecisionTreeClassifier

work = Path(sys.argv[1])
corpus = Path(os.environ["LOCALAPPDATA"]) / "RustOverlayData" / "audio-corpus"
BASE = ["level100_db", "ild20_db", "hi_lo_db", "early_late_db", "rise_db", "background_db"]
SHAPE = ["attack_ms", "decay_db", "crest_db", "centroid_hz", "flatness_db"]
FEATURES = BASE + (SHAPE if "--shape" in sys.argv else [])


def near_own(own, t):
    i = bisect_left(own, t - 100)
    return i < len(own) and own[i] <= t + 200


sessions = {}
for f in sorted(work.glob("*.shots.detections.csv")):
    name = f.name.split(".")[0]
    shots = list(csv.DictReader(open(corpus / name / "shots.csv")))
    own = sorted(float(s["utc_ms"]) for s in shots if s.get("source") == "own")
    X, y = [], []
    for r in csv.DictReader(open(f)):
        t = float(r["utc_ms"])
        if r["matched"] != "true" and near_own(own, t):
            continue
        row = [float(r[k]) for k in FEATURES]
        row[1] = abs(row[1])
        X.append(row)
        y.append(1 if r["matched"] == "true" else 0)
    sessions[name] = (np.array(X), np.array(y))


def models():
    yield "logistic", lambda: LogisticRegression(max_iter=2000, class_weight="balanced")
    yield "tree d3", lambda: DecisionTreeClassifier(max_depth=3, class_weight="balanced", min_samples_leaf=10)
    yield "tree d4", lambda: DecisionTreeClassifier(max_depth=4, class_weight="balanced", min_samples_leaf=10)


for label, make in models():
    print(f"=== {label} (leave one session out)")
    tot = {}
    for held in sessions:
        Xtr = np.vstack([sessions[s][0] for s in sessions if s != held])
        ytr = np.concatenate([sessions[s][1] for s in sessions if s != held])
        scaler = StandardScaler().fit(Xtr)
        m = make().fit(scaler.transform(Xtr), ytr)
        Xte, yte = sessions[held]
        p = m.predict_proba(scaler.transform(Xte))[:, 1]
        cells = []
        for thr in (0.2, 0.35, 0.5):
            keep = p >= thr
            tp, fp = int((keep & (yte == 1)).sum()), int((keep & (yte == 0)).sum())
            cells.append(f"t{thr}: {tp}/{int(yte.sum())} true {fp}/{int((yte == 0).sum())} false")
            a = tot.setdefault(thr, [0, 0, 0, 0])
            a[0] += tp; a[1] += int(yte.sum()); a[2] += fp; a[3] += int((yte == 0).sum())
        print(f"  {held:17} " + " | ".join(cells))
    for thr, (tp, n, fp, nf) in tot.items():
        print(f"  pooled t{thr}: true kept {tp}/{n} ({100 * tp / n:.1f}%), false kept {fp}/{nf} ({100 * fp / nf:.1f}%)")
    if label == "logistic":
        X = np.vstack([sessions[s][0] for s in sessions]); y = np.concatenate([sessions[s][1] for s in sessions])
        sc = StandardScaler().fit(X); m = make().fit(sc.transform(X), y)
        print("  weights (standardised):", dict(zip(FEATURES, np.round(m.coef_[0], 2))), "bias", round(m.intercept_[0], 2))
