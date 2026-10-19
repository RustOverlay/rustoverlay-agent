# Audio corpus score

`bash crates/ro-audio/score/run.sh` (a few minutes; run it in the background) builds `ro-audio-replay`, streams every
session in `%LOCALAPPDATA%\RustOverlayData\audio-corpus` through the live locator, and compares each against
`baseline.txt`. It fails if a session finds fewer shots, raises more false positives, or surfaces more of the
player's own gunshots. Bearing and distance errors are printed for review but don't gate the run, because a
recall gain can add hard shots that widen them. `--update` rewrites the baseline after a run that passes.
`--accept` rewrites it over flagged regressions. Use it only for a trade-off taken on purpose, and record
why in the commit.

The corpus only grows. Every change to the detector or estimator is scored against every session ever
recorded, so a session never needs re-recording.

## A session

| File | From | What |
|---|---|---|
| `audio.wav`, `timing.csv`, `clock.csv`, `session.json` | `calib-recorder --game` | game-only loopback with its QPC→UTC timeline |
| `compass/*.png`, `compass/index.csv` | `calib-recorder` (unless `--no-compass`) | the HUD compass strip at 20 Hz, for a yaw reader |
| `shots.csv` | CalibMod | ground truth: bot shots plus `source=own` rows for the player's gun |
| `trace.csv` | CalibMod (`calib.ambush`, `calib.trace on`) | the listener's pose at 10 Hz |
| `meta.json` | `bundle_session.py` | scenario, listener state, what the session is valid for (`use`) |

## Recording a free-play session

1. Start the local server with CalibMod and join it. Run `ai.think false` and `ai.move false`.
2. Run `calib-recorder --game`, which writes to `audio-corpus/<utc timestamp>`.
3. Run `calib.ambush minutes=10` over RCON and play normally until it reports done.
4. Stop the recorder with Ctrl+C, then
   `py -3 crates/ro-audio/score/bundle_session.py <session> <server>/server/<identity>/calib --scenario ambush`.
5. Run `run.sh` and look at the new session's line. Commit the baseline once the session is understood.

The sessions from 2026-10-17 were brought in by `build_corpus.py`. `quick1`, `sweep30`, `sweep30b` and
`awake1` were recorded from the whole endpoint, so Discord and music are mixed in. `sweep30` and `sweep30b`
had a sleeping listener, so their bearings mean nothing (`meta.json` `use: ["detect"]`).
