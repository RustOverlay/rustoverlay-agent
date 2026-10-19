#!/bin/bash
# Gunshot locator accuracy over every audio-corpus session, against baseline.txt. See README.md.
# usage: run.sh [--no-build] [--update | --accept]    (reports land in $SCORE_WORK, default <repo>/target/audio-score)
#   --update rewrites the baseline after a passing run; --accept rewrites it even over flagged regressions.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
WORK=${SCORE_WORK:-$REPO/target/audio-score}
CORPUS="$LOCALAPPDATA/RustOverlayData/audio-corpus"
BIN="${CARGO_TARGET_DIR:-$REPO/target}/release/ro-audio-replay.exe"
BASELINE="$HERE/baseline.txt"
build=1 update=0 accept=0
for arg in "$@"; do
  case "$arg" in --no-build) build=0 ;; --update) update=1 ;; --accept) accept=1 ;; *) echo "unknown argument $arg"; exit 2 ;; esac
done

if [ $build -eq 1 ]; then
  cargo build -q -p ro-audio --release --bin ro-audio-replay --manifest-path "$REPO/Cargo.toml" || exit 1
fi

mkdir -p "$WORK"
rm -f "$WORK"/*

fail=0
: > "$WORK/summary.txt"
for dir in "$CORPUS"/*/; do
  name=$(basename "$dir")
  [ -f "$dir/shots.csv" ] || continue
  "$BIN" "$dir" "$dir/shots.csv" --out "$WORK/$name.shots.csv" > "$WORK/$name.txt" 2>&1 || { echo "$name: replay failed"; fail=1; continue; }
  # "<name> recall shots fp own_surfaced own_total bearing_p50 bearing_p90 dist_p50 dist_p90"
  tr -d '\r' < "$WORK/$name.txt" | awk -v n="$name" '/^SUMMARY/ {
    for (i = 2; i <= NF; i++) { split($i, kv, "="); v[kv[1]] = kv[2] }
    split(v["own"], own, "/")
    print n, v["recall"], v["shots"], v["fp"], own[1], own[2], v["bearing_p50"], v["bearing_p90"], v["dist_p50"], v["dist_p90"] }' >> "$WORK/summary.txt"
done

# Missing a shot or inventing one is a regression; bearing and distance are shown for review, since
# a recall gain can add hard shots that widen them.
while read -r name recall shots fp own_s own_t bp50 bp90 dp50 dp90; do
  read -r _ b_recall b_shots b_fp b_own_s _ b_bp50 b_bp90 b_dp50 b_dp90 <<< "$(grep "^$name " "$BASELINE" 2>/dev/null)"
  line="$name | recall $recall/$shots | fp $fp | own $own_s/$own_t | bearing $bp50/$bp90 | dist $dp50/$dp90"
  if [ -n "${b_recall:-}" ]; then
    line="$line | baseline recall $b_recall fp $b_fp bearing $b_bp50/$b_bp90 dist $b_dp50/$b_dp90"
    [ "$recall" -lt "$b_recall" ] && { line="$line  << FEWER FOUND"; fail=1; }
    [ "$fp" -gt "$b_fp" ] && { line="$line  << MORE FALSE POSITIVES"; fail=1; }
    [ "$own_s" -gt "$b_own_s" ] && { line="$line  << MORE OWN SHOTS SURFACED"; fail=1; }
    [ "$shots" -ne "$b_shots" ] && line="$line  (shots $b_shots -> $shots: corpus changed?)"
  else
    line="$line | new session"
  fi
  echo "$line"
done < "$WORK/summary.txt"

if { [ $update -eq 1 ] && [ $fail -eq 0 ]; } || [ $accept -eq 1 ]; then
  cp "$WORK/summary.txt" "$BASELINE" && echo "baseline updated"
fi
[ $fail -eq 0 ] && echo "SCORE OK" || echo "SCORE FAILED (reports in $WORK)"
exit $fail
