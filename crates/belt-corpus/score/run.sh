#!/bin/bash
# Matcher accuracy over the recorded corpora, against baseline.txt. See README.md.
# usage: run.sh [--no-build] [--update]    (reports and montages land in $SCORE_WORK, default <repo>/target/score)
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)
WORK=${SCORE_WORK:-$REPO/target/score}
C="$LOCALAPPDATA/RustOverlayData"
BIN="${CARGO_TARGET_DIR:-$REPO/target}/release/belt-corpus.exe"
BASELINE="$HERE/baseline.txt"
build=1 update=0
for arg in "$@"; do
  case "$arg" in --no-build) build=0 ;; --update) update=1 ;; *) echo "unknown argument $arg"; exit 2 ;; esac
done

if [ $build -eq 1 ]; then
  cargo build -q -p belt-corpus --release --bins --manifest-path "$REPO/Cargo.toml" || exit 1
fi

mkdir -p "$WORK"
rm -f "$WORK"/*

# The c1 truth holds only from 105408 (a medkit sat in slot 5 before), and 105609-130 and 110258-932
# show the taskbar and a terminal over the belt rather than the game.
C1="--from 105408 --exclude 105609-130,110258-932"

# id | corpus | truth | flags
CONFIGS=(
  "c1|belt-corpus|rock,knife.bone,rifle.ak,rifle.ak,syringe.medical,-|$C1"
  "c1-confirm|belt-corpus|rock,knife.bone,rifle.ak,rifle.ak,syringe.medical,-|$C1 --confirm 2"
  "c2|belt-corpus-2|-,knife.bone,-,rifle.ak,syringe.medical,-|"
  "c2-flat|belt-corpus-2|-,knife.bone,-,rifle.ak,syringe.medical,-|--flat"
  "skins|belt-corpus-skins|rifle.ak,door.hinged.metal,door.hinged.metal,door.hinged.metal,smg.thompson,-|--skins"
  "skins-confirm|belt-corpus-skins|rifle.ak,door.hinged.metal,door.hinged.metal,door.hinged.metal,smg.thompson,-|--skins --confirm 2"
)

# "<id> <scored> <correct> <wrong>" from one report.
summarise() {
  tr -d '\r' < "$2" | awk -v id="$1" '
    /^slots scored/ { scored = $4 }
    /^correct/      { correct = $3 }
    /^WRONG/        { wrong = $3 }
    END { print id, scored + 0, correct + 0, wrong + 0 }'
}

fail=0
: > "$WORK/summary.txt"
for spec in "${CONFIGS[@]}"; do
  IFS='|' read -r id dir truth flags <<< "$spec"
  # shellcheck disable=SC2086
  "$BIN" "$C/$dir" "$truth" $flags --dump --montage "$WORK/$id.png" > "$WORK/$id.txt" 2>&1
  # 2 is "scored, some reads wrong", which the baseline judges; anything else is a broken run.
  status=$?; [ $status -eq 0 ] || [ $status -eq 2 ] || { echo "$id: belt-corpus exited $status"; fail=1; continue; }
  summarise "$id" "$WORK/$id.txt" >> "$WORK/summary.txt"
done

# A wrong item is worse than no answer, so any rise in WRONG fails the run.
while read -r id scored correct wrong; do
  read -r _ b_scored b_correct b_wrong <<< "$(grep "^$id " "$BASELINE" 2>/dev/null)"
  pct() { awk -v n="$1" -v d="$2" 'BEGIN { printf "%.1f%%", d ? 100 * n / d : 0 }'; }
  line="$id | correct $correct/$scored ($(pct "$correct" "$scored")) | WRONG $wrong"
  if [ -n "${b_scored:-}" ]; then
    line="$line | baseline correct $b_correct, WRONG $b_wrong"
    [ "$wrong" -gt "$b_wrong" ] && { line="$line  << MORE WRONG"; fail=1; }
    [ "$scored" -ne "$b_scored" ] && line="$line  (scored $b_scored -> $scored: corpus changed?)"
  fi
  echo "$line"
done < "$WORK/summary.txt"

if [ $update -eq 1 ] && [ $fail -eq 0 ]; then
  cp "$WORK/summary.txt" "$BASELINE" && echo "baseline updated"
fi
[ $fail -eq 0 ] && echo "SCORE OK" || echo "SCORE FAILED (reports in $WORK)"
exit $fail
