# Calibration tools

Not part of the agent. These exist to measure the icon matcher against the real game, because the
matcher's thresholds are only as good as the backgrounds they were calibrated on — and every gap
found so far came from a background nobody had captured yet. See docs/item-matching.md.

Neither tool writes into the repo. Captures land in `%LOCALAPPDATA%\RustOverlayData\belt-corpus`, and
they must stay there: they are screenshots of a real session.

## belt-recorder

Records the belt row while Rust is focused.

```
cargo run --release -p belt-recorder -- [output-dir] [frames] [minutes]
```

Defaults to `%LOCALAPPDATA%\RustOverlayData\belt-corpus`, 250 frames, 25 minutes; it stops at whichever
comes first. Start it, alt-tab into Rust, and fly.

- **Only while Rust is focused.** Capture reads the composited desktop, so this is a privacy control
  before it is anything else.
- **Only world-view frames.** With the panel open every slot sits on the same flat grey — the case
  that already works.
- **Only the belt strip**, ~100 KB against ~3 MB for a full frame. The filename carries the frame
  size and the strip origin, so `InventoryGeometry` rebuilds the slot rects exactly.
- **Skips frames whose backdrop has not changed**, so parking somewhere does not write the same
  scene two hundred times.

**Keep the hotbar contents fixed for the session.** That is what makes every frame self-labelling
and the whole exercise cheap. Switching which slot is *selected* is free and gives more highlighted
samples. Using an item is not free — it silently changes that slot's ground truth, and the scores
afterwards grade against a label that is no longer true.

What to go looking for is contrast, not distance: bright and busy (grass, forest floor), bright and
flat (snow, sand, open water), dark (night, caves, indoors), and hard edges — standing so a wall,
cliff or horizon cuts *through* the belt row. Flat bright backgrounds and hard edges are where the
failures have been.

## belt-corpus

Scores a recorded corpus.

```
cargo run --release -p belt-corpus -- <corpus-dir> [truth,csv] [--from HHmmss] [--to HHmmss] [--departure n] [--confirm n] [--flat] [--dump] [--montage out.png]
```

`truth` is the six belt slots in order, using `-` for a slot known to be empty:

```
cargo run --release -p belt-corpus -- %LOCALAPPDATA%\RustOverlayData\belt-corpus rock,knife.bone,rifle.ak,rifle.ak,syringe.medical,-
```

`crates/belt-corpus/score/run.sh` runs every recorded corpus at once and fails on any rise in wrong
reads against its committed baseline; that is the gate for a matcher change.

Reads Facepunch's icons from the local install at runtime — they are never vendored. Exits non-zero
if anything read wrong, so it can gate a change.

- `--from` / `--to` bound the run by capture time, which is how to score a session in two halves
  when something changed partway through.
- `--exclude HHmmss-fff,...` skips strips by name, for frames that are not the game (a window or
  the taskbar over the belt). `score/run.sh` lists the ones it skips and why.
- `--departure` overrides `IconMatcher.MinimumDeparture`, so the empty-slot gate can be swept over a
  corpus without editing and rebuilding the matcher. Sweep it across *every* corpus at once — a
  threshold that only holds on the terrain it was fitted to is the failure this tool exists to catch.
- `--categories` restricts candidates to the given item categories (`Weapon,Attire,...`). Narrowing
  the catalogue does *not* reduce empty-slot inventions — measured, see docs/item-matching.md — but
  the flag is how that was shown and how to re-check it after a catalogue change.
- `--confirm [n]` replays the corpus through `SlotHistory`, the agent's multi-frame filter, so a
  change to it can be scored offline. Both corpora go to zero wrong reads at the shipped `n` of 2.
- `--skins` adds the icons from Steam's workshop cache as extra faces of the items they repaint.
  Off by default so a run stays comparable with every score recorded before skins existed. Prints the
  icon count and any workshop label no item answers to.
- `--skin-categories` / `--all-skins` widen `SkinCatalog.DefaultCategories`, the allow-list deciding
  whose skins are carried. Admitting everything is what 23 confirmed wrong reads looks like — see
  docs/item-matching.md — so these are for re-measuring that, not for running with.
- `--coarse` overrides `IconMatcher.CoarseShortlist`, which now defaults to a quarter of the
  catalogue rather than a flat 64. That flat number is what made the first skinned run look like a
  90-read regression, so sweep this before concluding anything about a catalogue change.
- `--flat` scores slots against one averaged background colour instead of the per-cell scene
  prediction the matcher now ships with, which is how that change was measured.
- `--dump` prints a line per frame (`.` correct, `-` abstain, `X` wrong) — the fastest way to spot
  the moment a slot's contents changed.
- `--montage` writes a contact sheet of every wrong read.

**Use `--montage` before believing any number.** A score sheet cannot tell a wrong read from a wrong
label. Nine "wrong" reads in the first session turned out to be a medkit that had been used —
worth knowing, but not the defect the number claimed. The seven remaining empty-slot inventions were
only understood as *busy terrain* by looking at them.

An empty slot naming anything at all is the worst result the matcher can produce, and it needs no
labels to detect: an answer on a slot known to be empty is wrong whatever the other slots hold. That
property is why the empty-slot findings survived the labels being wrong.
