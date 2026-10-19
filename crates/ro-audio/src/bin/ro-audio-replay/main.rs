//! Streams a calib-recorder session through the live detector and estimator, packet by packet, and
//! scores the result against CalibMod's shots CSV. See docs/research/locate/locator.md.

mod score;
mod session;

use std::path::PathBuf;
use std::process::ExitCode;

use ro_audio::{AudioProfile, Locator};

const USAGE: &str = "usage: ro-audio-replay <session_dir> <shots.csv> [--profile profile.json] [--out per_shot.csv]";

fn main() -> ExitCode {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let (mut positional, mut profile_path, mut out) = (Vec::new(), None, None);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = || rest.next().map(PathBuf::from).ok_or(format!("{arg} needs a value\n{USAGE}"));
        match arg.as_str() {
            "--profile" => profile_path = Some(value()?),
            "--out" => out = Some(value()?),
            _ if arg.starts_with("--") => return Err(format!("unknown argument '{arg}'\n{USAGE}")),
            _ => positional.push(PathBuf::from(arg)),
        }
    }
    let [session_dir, shots_csv] = positional.as_slice() else {
        return Err(USAGE.into());
    };
    let profile = match &profile_path {
        Some(path) => AudioProfile::load(path)?,
        None => AudioProfile::builtin(),
    };

    let recording = session::Recording::load(session_dir)?;
    let shots = score::load_shots(shots_csv)?;
    let mut locator = Locator::new(profile).keeping_rejections();
    let mut rejected = Vec::new();
    let located: Vec<_> = recording
        .blocks()
        .flat_map(|block| {
            let found = locator.push(&block);
            rejected.extend(locator.take_rejected());
            found
        })
        .collect();

    let minutes = recording.frames as f64 / f64::from(recording.rate) / 60.0;
    let in_recording: Vec<score::Shot> = shots.into_iter().filter(|s| covers(&recording, s.utc_ms)).collect();
    let matched = score::match_shots(&in_recording, &located);

    println!("{} — {} shots in the recording, {} detections, {minutes:.1} min", session_dir.display(), in_recording.len(), located.len());
    print!("{}", score::report(&matched, minutes));
    print!("{}", score::false_positive_delays(&in_recording, &located, &matched.outcomes));
    print!("{}", score::miss_reasons(&matched.outcomes, &rejected));
    print!("{}", score::summary(&matched));

    let out = out.unwrap_or_else(|| session_dir.join("replay_shots.csv"));
    std::fs::write(&out, score::per_shot_csv(&matched.outcomes)).map_err(|e| format!("{}: {e}", out.display()))?;
    let turned_down = out.with_extension("rejected.csv");
    std::fs::write(&turned_down, score::rejected_csv(&in_recording, &rejected)).map_err(|e| format!("{}: {e}", turned_down.display()))?;
    let all = out.with_extension("detections.csv");
    std::fs::write(&all, score::detections_csv(&in_recording, &located, &matched.outcomes)).map_err(|e| format!("{}: {e}", all.display()))?;
    println!("per-shot -> {}, every detection -> {}", out.display(), all.display());
    Ok(())
}

/// Whether the recording spans the shot's fire time plus its match window.
fn covers(recording: &session::Recording, utc_ms: f64) -> bool {
    let (Some(first), Some(last)) = (recording.packets.first(), recording.packets.last()) else {
        return false;
    };
    let end_ms = (last.2 as f64 + last.1 as f64 * 1e7 / f64::from(recording.rate)) / 10_000.0;
    first.2 as f64 / 10_000.0 + 300.0 <= utc_ms && utc_ms + 400.0 <= end_ms
}
