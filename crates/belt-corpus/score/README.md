# Matcher score

`bash crates/belt-corpus/score/run.sh` (a few minutes; run it in the background) builds `belt-corpus`,
scores six configurations of the `%LOCALAPPDATA%\RustOverlay\belt-corpus*` corpora, and compares each
one's correct and WRONG counts with `baseline.txt`. It fails when any configuration gets more WRONG
reads than its baseline: an abstention costs a blank cell, a wrong item misleads the team.

Run it after any change to the matcher, `SlotHistory` or the icon pipeline. When a change is an
improvement, `run.sh --update` rewrites the baseline; commit it with the change so the gain is on
record. Reports (`--dump`) and montages of every wrong slot land in `target/score/`.
