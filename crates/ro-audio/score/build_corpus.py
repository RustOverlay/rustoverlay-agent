"""One-off: turn the 2026-10-17 calib sessions into self-contained audio-corpus sessions.

Each session gets its shots CSV (merged when a session spanned several) and a meta.json. New sessions
are recorded straight into audio-corpus by calib-recorder and bundled by bundle_session.py.

    py -3 crates/ro-audio/score/build_corpus.py <server calib dir>
"""

import json
import os
import shutil
import sys
from pathlib import Path

DATA = Path(os.environ["LOCALAPPDATA"]) / "RustOverlayData"
SESSIONS = {
    # name: (shots CSVs, meta)
    "quick1": (["shots-20261017T104950Z.csv"], {"listener": "awake", "use": ["detect", "bearing", "distance"]}),
    "sweep30": (["shots-20261017T105732Z.csv"], {"listener": "asleep", "use": ["detect"],
                "note": "listener asleep: panning collapsed to about ±5 dB, bearings unusable"}),
    "sweep30b": (["shots-20261017T110405Z.csv"], {"listener": "asleep", "use": ["detect"],
                 "note": "listener asleep: panning collapsed to about ±5 dB, bearings unusable"}),
    "awake1": (["shots-20261017T111003Z.csv"], {"listener": "awake", "use": ["detect", "bearing", "distance"]}),
    "awake2": (["shots-20261017T113004Z.csv"], {"listener": "awake", "use": ["detect", "bearing", "distance"], "profile_fit": True}),
    "dist1": (["shots-20261017T115030Z.csv"], {"listener": "awake", "use": ["detect", "bearing", "distance"], "profile_fit": True}),
    "block2": (["shots-20261017T115559Z.csv", "shots-20261017T115735Z.csv", "shots-20261017T115910Z.csv"],
               {"listener": "awake", "use": ["detect", "bearing", "distance"], "note": "M249 and silenced AK; cut short when the client froze"}),
    "blind1": (["shots-20261017T120505Z.csv"], {"listener": "awake", "use": ["detect", "bearing", "distance"], "profile_fit": True}),
}


def merged_csv(paths):
    header, rows = None, []
    for p in paths:
        lines = p.read_text().splitlines()
        if header is None:
            header = lines[0]
        elif lines[0] != header:
            raise SystemExit(f"{p}: header differs from the first CSV")
        rows += [l for l in lines[1:] if l.strip()]
    return "\n".join([header] + rows) + "\n"


def main():
    server_calib = Path(sys.argv[1])
    corpus = DATA / "audio-corpus"
    corpus.mkdir(parents=True, exist_ok=True)
    for name, (csvs, meta) in SESSIONS.items():
        src, dst = DATA / "calib" / name, corpus / name
        if dst.exists():
            print(f"{name}: already in the corpus")
            continue
        if not src.exists():
            print(f"{name}: no session at {src}")
            continue
        shutil.move(str(src), str(dst))
        (dst / "shots.csv").write_text(merged_csv([server_calib / c for c in csvs]))
        (dst / "meta.json").write_text(json.dumps({"session": name, "recorded": "2026-10-17", "scenario": "sweep",
                                                   "shots_sources": csvs, **meta}, indent=2) + "\n")
        print(f"{name}: moved, {len(csvs)} shots CSV(s)")


if __name__ == "__main__":
    main()
