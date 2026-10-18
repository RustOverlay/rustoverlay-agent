#!/bin/bash
# Workspace build + test + clippy, sequential and job-capped to stay within memory. Prints a summary only.
# usage: scripts/check.sh [jobs]   (full logs in target/check/)
set -u
REPO=$(cd "$(dirname "$0")/.." && pwd)
J=${1:-4}
OUT="$REPO/target/check"
mkdir -p "$OUT"
run() { local sub=$1; shift; cargo "$sub" --manifest-path "$REPO/Cargo.toml" --workspace --color never -j "$J" "$@"; }

run build --all-targets > "$OUT/build.txt" 2>&1; b=$?
echo "build:  exit $b"
[ $b -ne 0 ] && { grep -E '^error' -A5 "$OUT/build.txt" | head -40; exit $b; }

run test --no-fail-fast > "$OUT/test.txt" 2>&1; t=$?
awk '/^test result/ {p+=$4; f+=$6} END {print "test:   exit '$t', " p " passed, " f " failed"}' "$OUT/test.txt"
grep -E '^test .* FAILED$|^---- ' "$OUT/test.txt" | head -20

run clippy --all-targets -- -D warnings > "$OUT/clippy.txt" 2>&1; c=$?
echo "clippy: exit $c"
grep -E '^(error|warning)' -A4 "$OUT/clippy.txt" | head -40
exit $(( t || c ))
