"""Makes a freshly recorded audio-corpus session self-contained: copies the CalibMod shots and trace CSVs
whose time span overlaps the recording into it, and writes meta.json.

    py -3 crates/ro-audio/score/bundle_session.py <session dir> <server calib dir> --scenario ambush [--note "..."]
"""

import argparse
import datetime as dt
import json
from pathlib import Path


def utc_ms(iso):
    return dt.datetime.fromisoformat(iso[:26].rstrip("Z") + "+00:00").timestamp() * 1000


def span(csv):
    lines = [l for l in csv.read_text().splitlines()[1:] if l.strip()]
    if not lines:
        return None
    header = csv.read_text().splitlines()[0].split(",")
    i = header.index("utc_ms")
    times = [float(l.split(",")[i]) for l in lines]
    return min(times), max(times)


def overlapping(server_calib, prefix, start, stop):
    hits = []
    for csv in sorted(server_calib.glob(prefix + "-*.csv")):
        s = span(csv)
        if s and s[0] <= stop and s[1] >= start:
            hits.append(csv)
    return hits


def merged(csvs):
    header, rows = None, []
    for c in csvs:
        lines = c.read_text().splitlines()
        if header is None:
            header = lines[0]
        elif lines[0] != header:
            raise SystemExit(f"{c}: header differs from {csvs[0]}")
        rows += [l for l in lines[1:] if l.strip()]
    return "\n".join([header] + rows) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("session", type=Path)
    ap.add_argument("server_calib", type=Path)
    ap.add_argument("--scenario", required=True, help="ambush, sweep, ...")
    ap.add_argument("--note", default="")
    a = ap.parse_args()

    info = json.loads((a.session / "session.json").read_text())
    if not info.get("stop_utc"):
        raise SystemExit("session.json has no stop_utc: the recorder did not finish cleanly")
    start, stop = utc_ms(info["start_utc"]), utc_ms(info["stop_utc"])

    shots = overlapping(a.server_calib, "shots", start, stop)
    if not shots:
        raise SystemExit("no shots CSV overlaps the recording")
    (a.session / "shots.csv").write_text(merged(shots))
    traces = overlapping(a.server_calib, "trace", start, stop)
    if traces:
        (a.session / "trace.csv").write_text(merged(traces))

    meta = {
        "session": a.session.name,
        "recorded": info["start_utc"][:10],
        "scenario": a.scenario,
        "listener": "free play" if a.scenario == "ambush" else "awake",
        "use": ["detect", "bearing", "distance"],
        "shots_sources": [c.name for c in shots],
        "trace_sources": [c.name for c in traces],
        "compass_frames": info.get("compass_frames", 0),
    }
    if a.note:
        meta["note"] = a.note
    (a.session / "meta.json").write_text(json.dumps(meta, indent=2) + "\n")
    print(f"{a.session.name}: {len(shots)} shots CSV(s), {len(traces)} trace CSV(s), {meta['compass_frames']} compass frames")


if __name__ == "__main__":
    main()
