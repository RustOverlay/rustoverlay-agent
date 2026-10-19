# Resolving a slot to an item

Part two of inventory reading: given a cropped slot, which item is in it. Part one — where the slots
are — is in `hud-layout.md`.

## The corpus ships with the game

`<Rust>\Bundles\items` holds a definition JSON and an icon PNG per item, in plaintext:

```
1,243 definitions        rifle.ak.json
1,233 with icons         rifle.ak.png     512x512, 32bpp ARGB
```

```json
{"itemid":1545779598,"shortname":"rifle.ak","Name":"Assault Rifle","Category":"Weapon",
 "stackable":1,"condition":{"enabled":true,"max":150.0},"isWearable":false,"isHoldable":true}
```

Three things follow.

**The icons have transparent backgrounds**, so the alpha channel is an exact mask. Comparison runs
only over opaque cells, which makes a match independent of whatever the slot is drawn over.

**The metadata narrows the search before any pixels are compared.** An armour slot can only hold one
of 165 `isWearable` items, not one of 1,233. Most confusions are with items that could never appear
in that slot, so the filter buys accuracy as well as speed.

**It is version-matched and self-updating.** The catalogue comes from the player's own install, so a
game update refreshes it for free — the same property `RealLogSmokeTests` relies on for log drift.

> Read these at runtime from the player's install. Never vendor them into the repo: the icons and
> definitions are Facepunch's.

## Matching

`IconSignature` downscales an icon to 32x32 and compares with zero-mean normalised correlation over
cells opaque in both images. Correlation rather than difference, so a slot that tints or dims the
icon still matches.

Measured against the local install — every icon put through a simulated slot render, then matched
against all 1,233 candidates:

| | 16x16 | 32x32 |
|---|---|---|
| top-1 correct | 98.0% | **98.7%** |
| confident (passed score and margin gates) | 275/300 | **281/300** |
| **confident and wrong** | **0** | **0** |
| mean margin over runner-up | 0.257 | 0.347 |
| per slot, 1,233 candidates | 6.8 ms | 17.1 ms |

> **Gotcha:** 16x16 is too coarse for thin icons. `bow.hunting` covered too few opaque cells to
> correlate and scored **0.000 against itself**. 32x32 fixes it, at 2.5x the compare cost.

Of the four remaining misses at 32x32, three are genuine ties where the correct item and the winner
score identically — `mannequin` / `clothing.mannequin`, `apartment.master_key` / `door.key`,
`radiationremovetea.pure` / `radiationresisttea.pure`. Their icons are indistinguishable, and the
margin gate abstains rather than guessing. `spiderweb` is the only true failure: near-total
transparency leaves nothing to correlate.

**Nothing was ever confidently wrong.** That is the property worth protecting — a wrong item shown
to a teammate is worse than a blank slot, so `Identify` returns null unless the winner clears both
an absolute score and a margin over the runner-up.

## Open

- A full inventory is ~700 ms; see "Speed" below for the current breakdown. Read on change, not on a
  timer.
- Corpus signature build takes ~4.6 s at startup. Cache it, keyed on the install's item bundle
  timestamps.
## Crop the icon, not the slot

The single biggest factor in real-slot accuracy is **alignment**, not the metric. The icon neither
fills the slot nor sits centred in it — the slot's edges carry the condition bar and the stack or
ammo count. `SlotRect.IconArea` insets by 10% of slot size and lifts by 4%, found by a 2D search
against four known items across two resolutions.

| crop | assault rifle | medical syringe |
|---|---|---|
| slot rect | 0.231, **rank 22** | 0.530, rank 1 |
| `IconArea` | **0.862**, rank 1 | **0.956**, rank 1 |

Two percent of slot size away from this alignment the rifle falls back to rank 22, but the optimum
is a plateau (offset −4 to −6%, inset 6–12% all rank first on all four items), so it is expressed as
a fraction of slot size and expected to hold across resolutions.

> **Gotcha:** an earlier "correction" to `BeltBottomFromBottom`, made by measuring the slot's border,
> moved the crop 2px and dropped the syringe from 0.841 to 0.530. The border and the icon are not
> concentric. Calibrate crops against the icon, not the chrome.

### Two metric changes that were tried

- **Per-channel correlation**, to survive the blue fill on a held slot. Rejected: normalising
  channels independently discards colour, so a grey and a green item of the same silhouette score
  alike. The rifle fell from rank 1 to rank 25.
- **Per-channel centring under one normalisation.** Rejected here as "within 0.02 of the original on
  every un-highlighted slot — no measurable benefit for the added complexity", and **now shipped**,
  gated to chromatic backgrounds. The measurement was right and the conclusion did not follow from
  it: un-highlighted slots are precisely the ones it cannot affect, because their three channel
  means are already equal. The held slot it was reached for in the first place was never measured.
  See *The held slot*.

## Margin is the discriminator, not score

Calibrated on a frame with four genuinely empty belt slots:

| | best score | margin over runner-up |
|---|---|---|
| real item | 0.79 – 0.96 | **0.245 – 0.473** |
| empty slot | 0.77 – 0.81 | **0.012 – 0.027** |

An empty slot outscores some real items outright, so no absolute threshold can separate them.
`MinimumScore` at 0.65 only rejects obvious junk; the margin does the real work.

That first calibration read the gap as an order of magnitude, and set `MinimumMargin` to 0.15. A
wider corpus later showed the gap is much narrower than four empty slots suggested — an empty slot
has been observed at margin 0.158 — and that the runner-up is usually a *variant of the correct
item*. Both findings are below; the gate is now 0.10, measured against a margin that skips
look-alikes.

## Scored against a labelled inventory

43 slots across two frames, labelled by hand:

| container | correct | abstained | **wrong** |
|---|---|---|---|
| Belt, inventory open | 6/6 | 0 | **0** |
| Belt, world view | 5/6 | 1 | **0** |
| Wear | 4/7 | 3 | **0** |
| Main | 19/24 | 5 | **0** |
| **total** | **34/43** | **9** | **0** |

Nothing was wrong. Every failure was an abstention, which is the intended trade.

Three of the five main-grid abstentions are `5.56 Rifle Ammo`, `Explosive 5.56` and `Incendiary
Pistol Bullet` — three bullet icons that differ by a few pixels of tint. The margin gate is doing
exactly its job there; resolving them needs a signal beyond the icon.

> **Gotcha:** each container needs its own origin calibration. The comb fit that finds the grid locks
> onto the **separator line**, which sits ~4px (at 0.75 scale) above the slot interior. That was
> enough to make every main-grid slot abstain while the belt scored 6/6 in the same frame — the
> matcher looked fine and the geometry looked fine, because the error was smaller than it looked in
> an overlay. Calibrate each container against known items rather than assuming a shared offset.

### Second labelled run, snow biome

A different loadout on a bright background, 43 slots:

| container | correct | abstained | **wrong** |
|---|---|---|---|
| Belt, inventory open | 5/6 | 1 | **0** |
| Belt, world view | 4/6 | 2 | **0** |
| Wear | 6/7 | 1 | **0** |
| Main | 15/24 | 9 | **0** |
| **total** | **30/43** | **13** | **0** |

Still nothing wrong, but recall fell. The cause is the background: slot interiors measure 207-228
luminance here against 48-90 in the temperate frame, because the panels are translucent and snow
shows through. Icons wash out, scores compress, and margins fall under the gate.

**The top candidate was correct in 8 of the 13 abstentions**, at margins of 0.06-0.12 — just under
0.15. The gate is costing real recall.

Lowering it looked unsafe: an *empty* slot in the same frame scored 0.87 at margin 0.07, squarely
inside that band. Two ways to detect emptiness directly were tried and both failed:

| signal | empty slots | filled slots | separable |
|---|---|---|---|
| luminance variance | 8.4, 1.3 | 2.0 - 72.4 (median 4.3) | no — most filled slots are flatter |
| interior vs margin colour | 0.4, 1.7 | 2.8 - 4.7 | no — overlapping |

> **Gotcha:** an empty slot is not visually quiet. It shows whatever the game draws behind it, so on
> a busy or bright background it has more structure than a flat resource icon. Emptiness has to come
> from something other than the slot's own pixels.

### A third loadout, at a different resolution

A deployables-heavy inventory at **1920x1080** — doors, gates, vehicle modules, no armour worn — put
every slot of the main grid into abstention while the belt in the same frame read perfectly.

That signature meant geometry, not matching, and the cause was that the belt was anchored to the
frame's bottom while the main grid and wear row were offsets from the **top**. The two forms agree
at 1440x1080 and nowhere else. `WearTopAt1920` was 1189, which at scale 1.0 is past the bottom edge
of a 1080-tall frame — the wear row was being computed outside the image.

> **Gotcha:** if one container abstains on every slot while another reads perfectly in the same
> frame, suspect that container's origin before touching the matcher. This is the second time that
> exact symptom has appeared, and both times it was geometry.

Fixing it took the third set from 30/43 to 42/43 and is covered by two tests: one asserting every
container scales identically about the bottom edge, one asserting no slot falls outside the frame at
reference width.

### The background hypothesis, and why it was wrong

The obvious reading of the snow result was that brightness caused it, and the fix was to compensate:
the observed slot is `alpha * icon + (1 - alpha) * background`, with the background measurable from
the slot margin.

A third capture tested it properly — **the same loadout as the snow set, in desert**, so background
is the only variable. It scores 29/43 against snow's 30/43. Background is not the cause. The gap
between the first labelled set (34/43) and the second (30/43) is the *loadout*: set B is full of
ammo variants and small components, and set A was not.

> **Gotcha:** sets one and two differed in both biome and loadout, so the biome looked responsible
> by elimination. It took a capture that held the loadout fixed to separate them. Two captures
> differing in two variables cannot attribute anything.

### What actually costs recall: variant families

With the top three candidates printed, the pattern is unmistakable — the runner-up is nearly always
a cosmetic variant of the correct item:

| slot | winner | runner-up |
|---|---|---|
| 5.56 ammo | `ammo.rifle` 0.884 | `ammo.snowballgun` **0.884** |
| blue keycard | `keycard_blue` 0.853 | `keycard_green` 0.768 |
| HQ pistons | `piston3` 0.795 | `piston2` 0.702 |
| pistol bullet | `ammo.pistol` 0.840 | `ammo.pistol.fire` 0.773 |

A small margin against a look-alike is not evidence of uncertainty — the two icons are the same
picture. A small margin against an *unrelated* item is. The gate was calibrated on a frame where the
runner-up happened to be unrelated, and has been penalising the wrong thing ever since.

So the margin is now taken over the best candidate that is **not** indistinguishable from the winner
(catalogue-vs-catalogue correlation ≥ 0.90), and the look-alikes travel in the result as
`IconMatch.Indistinguishable` instead of suppressing it. Reporting "one of these two" is both honest
and more useful than reporting nothing.

### Detecting an empty slot, at the third attempt

Two earlier detectors failed because they measured the wrong thing. The one that works is the
**standard deviation of exactly the cells a comparison correlates**, on the composited scale:

| | contrast |
|---|---|
| empty slots | 0.004 - 0.012 |
| filled slots | 0.036 upward |

Nothing lands in between across 172 labelled slots, so `MinimumContrast` is 0.03 and costs no real
read. It generalises the degenerate `variance <= 1e-6` guard that was already there: correlation is
scale invariant, so without a floor the faintest gradient in an empty slot matches a real icon
perfectly. That is precisely how an empty wear slot came to be reported as `pumpkin`.

> **Gotcha:** the two failed detectors measured whole-slot luminance variance and interior-vs-margin
> colour. Both looked at the slot; neither looked at the cells being compared. The signal was there
> the whole time, in a region the earlier measurements averaged away.

> **This section is superseded — kept because the reasoning failure is the useful part.** At 332
> slots the two ranges above become 0.005-0.044 and 0.022-0.258, which overlap, and three empty slots
> were confidently read as items. "Nothing lands in between" was true of the sample and false of the
> world; a gap in 172 observations was taken as a property rather than as an absence of evidence.
> The detector that actually works compares the slot's structure against the amount the candidate
> icon *predicts*, rather than against a constant — see "Sixth set" below. `MinimumContrast` remains
> only as a flat-patch backstop.

### Measured over all four labelled sets — 172 slots

| configuration | correct | abstained | **wrong** | top-1 |
|---|---|---|---|---|
| original baseline, margin 0.15 | 142 | 30 | **0** | 107/117 |
| family-aware margin, 0.15 | 146 | 24 | **2** | 107/117 |
| family-aware margin alone, 0.10 | 149 | 18 | **5** | 107/117 |
| + background compositing, 0.10 | 151 | 18 | **3** | 114/117 |
| **+ contrast floor — shipped** | **154** | **18** | **0** | **114/117** |

Every part is load-bearing, and for a different reason:

- **Family-aware margin** buys the recall — it stops the gate punishing look-alikes.
- **Background compositing** barely moves the confident count alone, but lifts top-1 from 107/117 to
  114/117, and that ranking headroom is what makes a 0.10 gate viable at all.
- **Bottom-anchored geometry** recovered an entire container at 1920x1080.
- **The contrast floor** is what keeps the result honest — the middle rows show recall climbing and
  wrong answers climbing with it. It is the only reason the last row has a zero in it.

### Fourth set: components at night

A components-and-armour inventory at 1440x1080 on a near-black background scored **43/43 — no
abstentions and nothing wrong** — on a corpus it had never been tuned against. It also read a
*hovered* slot correctly, which was not expected: the highlight the game paints on hover is not the
same problem as the blue fill behind a held belt slot, and only the latter defeated matching.

Its empty slots measure 0.004-0.006 contrast, the lowest seen, which widens the emptiness gap rather
than narrowing it — a dark background makes an empty slot easier to reject, not harder.

> **Gotcha:** the margin gate has almost no headroom left. An empty slot has been measured at margin
> 0.158, above the old 0.15 gate, and was excluded only because its top score fell under
> `MinimumScore`. Margin alone cannot carry emptiness detection; that is `MinimumContrast`'s job now.

Compositing part-fixes the held slot, which had been a known gap: the highlighted belt slot ranked
`water` above the rifle under the raw metric, and ranks `rifle.ak.diver` first under compositing.
The margin was still too thin to report, so it stayed an abstention rather than a wrong answer —
until the centring change in *The held slot* opened it up.

### Fifth set: guns, ammo and worn armour — 220 slots

Three captures taken to fill the categories the corpus was thin on: six guns in the belt, the same
six slots swapped for six ammo types, then both together in the inventory with five armour pieces
worn. 48 new labelled slots, **194 correct, 26 abstained, 0 wrong** over 220.

Broken out by catalogue category, which is what the consumers actually care about:

| category | correct | abstained | wrong | of which ambiguous |
|---|---|---|---|---|
| Resources | 26 | 0 | **0** | 0 |
| Ammunition | 17 | 9 | **0** | **12** |
| Weapon | 9 | 7 | **0** | 0 |
| Medical | 16 | 0 | **0** | 0 |
| Attire | 12 | 3 | **0** | 0 |
| Component | 10 | 4 | **0** | 0 |
| Tool | 12 | 1 | **0** | 0 |
| Construction | 11 | 0 | **0** | 0 |

*Ambiguous* means correct but returned as a look-alike set rather than a single item. For ammunition
that is the majority of the successes: only 5 of 26 ammo slots yield a definite type.

> **Gotcha:** worn armour was previously written off as a wear-row problem, on the evidence of the
> arctic hazmat suit failing twice in the wear slot. Five different pieces worn in the same row —
> boots, hoodie, heavy plate pants, jacket, riot helmet — all read correctly. The failure is that one
> item, not the container. Two failures of a single item are not a container result.

Weapon failures behave differently from every other category: they fail the **score** gate, not the
margin gate. `t1_smg` tops out at 0.49-0.60 and `rifle.l96` at 0.53 in the main grid, both with ample
margin over the runner-up. Wide thin objects fill less of a square icon area, so fewer cells carry
the shape. Nothing else in the corpus fails this way — every other abstention is a margin abstention.

The same L96 reads confidently in a world-view belt slot and abstains in the main grid of the frame
taken a minute later, so the gate is marginal for it rather than the container being at fault.

### Refining the alignment — 194 to 201

The icon area is placed by arithmetic, and the arithmetic is a pixel or two out. A compact item does
not care; a rifle a few pixels wide moves clean off itself. Sweeping the icon area over one slot at
a time showed `t1_smg` going 0.60 to 0.77 and `rifle.l96` 0.53 to 0.79, while `wood` and
`riot.helmet` — already near their best — barely moved. Thin items are alignment-sensitive and
compact items are not, which is why the original inset, tuned on four compact items, sat on a
plateau that was not the peak.

Retuning the constant does not fix it: an inset of 0.09 beats 0.10 on mean and worst-case score
across the corpus and scores exactly the same 194. The error is per-slot, so the fix has to be too.

`Rank(ISlotSampler)` now ranks at the nominal placement, picks one better alignment, and re-scores
the leading 25 candidates there. **194 to 201 correct, 26 abstentions down to 19, still zero wrong.**
Attire and Component were the biggest gainers; the arctic hazmat suit that failed twice now reads.

One slot in the fifth set was left unlabelled at the time and has since been identified as
`heavy.plate.jacket`, making the corpus **221 slots: 202 correct, 19 abstained, 0 wrong.** It reads
correctly, taking Attire to 15/16.

> **Gotcha:** who chooses the alignment decides whether this is safe. Letting the whole shortlist
> compete — take whichever alignment maximises anyone's score — reaches 202, but `microphonestand`
> overtakes `rf.detonator` on an alignment that suits it, and abstains only because it happens to
> fall under `MinimumScore`. Choosing the alignment by the nominal leader alone scores 201 and never
> reorders a rank one. One fewer read is the right price.

### Sixth set: the same inventory at three resolutions — 332 slots

Shotgun and launcher ammunition, three rocket types, four worn pieces and six guns with magazines,
captured at 1440x1080, 1600x1024 and 1920x1080 without touching the inventory between shots. 111 new
labelled slots.

This is the first set where the corpus disagreed with the code, and it did so twice.

**Three slots were confidently wrong, all of them empty**, all in the 1920 frame: two wear slots read
as `gloweyes`/`pumpkin` at 0.789, and a main slot as `cannonball` at 0.645. First wrong reads in the
corpus's history, and they arrived the moment the backdrop changed — an outdoor road under bright sky
rather than snow, night or desert.

**The absolute contrast floor does not generalise.** It was set at 0.03 on the strength of empty
slots spanning 0.007-0.012 and filled ones 0.036 upward. Over all 332 slots:

| | range |
|---|---|
| empty slots | 0.005 - **0.044** |
| filled slots | **0.022** - 0.258 |

The distributions overlap outright. No absolute floor separates them, and the 3x gap the constant was
calibrated in was an artefact of a smaller sample. This is worth stating plainly because the original
note claimed the gap as evidence: it was not evidence, it was a coincidence of which frames had been
captured.

#### What replaced it: structure predicted, not structure present

Correlation is scale invariant. That is what lets it survive the game dimming an icon, and equally
what lets a nearly flat slot correlate perfectly with anything once its noise is amplified. The fix
is to look at *how much* amplification was needed — compare the slot's spread against the spread the
composited candidate predicts:

| | ratio |
|---|---|
| correct reads | 0.581 - 1.018 |
| correct, reported as a look-alike set | 0.715 - 0.771 |
| the three confidently wrong reads | **0.120, 0.121, 0.188** |

A 3x gap, and this one is measured across every background in the corpus rather than a subset. The
gate sits at 0.35. Ratios below one are expected: the game dims and softens the icon.

**332 slots: 306 correct, 26 abstained, 0 wrong.** The gate did not merely delete the three wrong
reads — it also promoted three abstentions to correct answers, because dropping implausible rivals
out of the ranking let the true item win its margin.

> **Gotcha:** the previous detector is still in the code as `MinimumContrast`, guarding the
> uncomposited comparison. It is not the empty-slot detector and should not be treated as one. If a
> future capture produces another wrong read, the ratio is the thing to look at.

Nothing in the matcher is pinned to 1440x1080: the same inventory scored the same way at all three
resolutions, and the only resolution-specific failures were the empty slots above.

### Seventh set: the same inventory at night, and the backpack — 373 slots

The loadout F inventory again at 1440x1080 with a night backdrop, plus the backpack slot, which now
has geometry. **373 slots: 348 correct, 25 abstained, 0 wrong.** Counts: **144/144, and 130/130 empty
slots silent.** (349/24/0 after the resampler change below.)

Night changed nothing for either reader, which is the answer to the open lighting question. Attire is
now 44/44 with the backpack included — `largebackpack` reads at 0.93-0.97 with a 0.44 margin in all
four frames, the most confident item in the corpus.

### Speed: 4.5s to 0.7s a frame

A full inventory is 37 slots and each was costing 120ms, almost all of it in ranking 1233 candidates
at 32x32. Two changes, neither of which alters a single read:

| | per slot | per frame |
|---|---|---|
| before | 120 ms | 4466 ms |
| coarse-to-fine shortlist | 39 ms | 1442 ms |
| staged alignment search | 23 ms | 892 ms |
| shortlist 150 to 64 | **19 ms** | **704 ms** |

**Coarse-to-fine.** An 8x8 pass over the whole catalogue, then full 32x32 scoring of the leaders only.
Over 239 filled slots the true item's coarse rank is 11 or better in 238, so a shortlist of 64 is a
6x margin. The exception is `rf.detonator` at coarse rank 181 — RF items draw their frequency over the icon, a
large "0MHz" that dominates an 8x8 patch. It abstains either way today, but anything that handles
that overlay must revisit that number.

**Staged alignment.** The search swept offset and size together, 75 re-crops a slot, and re-cropping
became the dominant cost once the catalogue scan was cheap. Searching offset first and size at the
winning offset is 27 re-crops — and scored two reads *better*, not worse.

> **Gotcha:** the coarse scores must be computed once into a list, not inside the sort comparator. A
> comparator that recomputes turns one pass over the catalogue into n log n of them, which is slower
> than the full-size scan it was meant to replace.

Counts cost 1 ms a slot and were never the problem.

### Speed again: 704 ms to 0.3 ms, and one read gained

Three further changes. The first two are pure throughput; the third changed the resampler and, as a
side effect, took the corpus from **348 correct to 349, still 0 wrong**.

| | per frame |
|---|---|
| after the shortlist work | 704 ms |
| one pass instead of six | 457 ms |
| managed area-filter crop | 191 ms |
| parallel over slots (16 cores) | 31 ms |
| unchanged slots skipped | **0.3 ms** |

**One pass instead of six.** `SimilarityTo` computed `Spread` over both spans for the structure
ratio, then `Correlation` recomputed both variances — and `Spread(x)² == varianceX / n`. They were
always the same numbers. A single `Measure` now returns the covariance and both variances, and every
gate is derived from it. Bit-identical output for 1.5x.

**Cropping without GDI+.** Re-cropping was 292 ms of the remaining 398, all of it
`Graphics.DrawImage`. `FrameSlotSampler` reads the `IFrameBuffer` pixel copy instead: 58 ms, and no
GDI+ lock, which is what makes the next line possible at all.

> **Gotcha — snap the crop grid and thin items die.** The first version averaged whole source pixels,
> giving some output cells 2 and their neighbours 3 at a ~2.3px ratio. That is a 50% swing in the
> sampling grid and it cost **six reads**, every one a thin item. Weighting each source pixel by its
> fractional overlap instead scores 349 — one *better* than `HighQualityBicubic`, which is what this
> replaced.

**Parallel, then skipped.** Slots are independent, so `InventoryReader` matches them with
`Parallel.For`: 191 ms to 31 ms. Then it hashes each slot's pixels and reuses the previous reading
when they have not moved. An inventory is static across the overwhelming majority of frames, so the
steady state is 0.3 ms and only genuinely changed slots are paid for.

The hash strides every 3rd pixel. That is safe in the direction that matters: a false *hit* costs
only work already budgeted for, while a false *miss* would need two different items to agree on every
sampled pixel.

> **Gotcha — parallelism found a latent bug in the glyph atlas.** `FontGlyphAtlas.At` cached into an
> unsynchronised `Dictionary` and rasterised through GDI+. Single-threaded that is fine; called from
> every slot at once it can corrupt the dictionary and drive GDI+ concurrently. It is locked now, at
> no cost — a frame uses three heights and renders each once. The failure would have been
> intermittent and cold-cache-only, which is the worst kind to find later.

### Why the remaining abstentions are not a threshold to loosen

> **Superseded.** This section explains why the *gates* could not be loosened, and that still holds —
> no gate moved. What changed is what happens to a variant family: the section after it takes the
> corpus from 349/24/0 to **366/7/0** by grouping look-alikes more willingly, without touching a
> single threshold on score or margin. Read this one for why the obvious fix does not work.

Measured when there were 25 of them; the area-filter crop later recovered one (`ammo.shotgun.slug` at
1440), leaving 24 and one fewer slug/fire pair. Nothing else below changed, and no threshold moved.

The true item is rank 1 in **24 of the 25**. Three fail the score gate and 22 fail the margin gate.

> **Gotcha:** read these numbers off the decision, not off a ranking printed at the nominal
> alignment. The scores the gates actually see come from the *refined* alignment and differ by up to
> 0.1 — enough to make an abstention look like it should have passed, and to send an investigation
> after a bug that is not there.

The three score-gate failures are not one problem:

| slot | rank 1 | score | margin | true item's rank |
|---|---|---|---|---|
| `snow main[14]` | `ballista.bolt.pitchfork` | **0.6256** | **0.2883** | 1 |
| `guns belt[3]` | `t1_smg` | 0.4874 | 0.0687 | 1 |
| `coast main[6]` | `microphonestand` | 0.6223 | 0.2151 | **0** |

`ballista.bolt.pitchfork` misses the gate by 0.024 while carrying nearly 3x the margin it needs. That
is a confident read with a mediocre absolute score, which is the case the two independent gates
handle worst.

`rf.detonator` no longer appears in its ranking at all — the coarse pass drops it, because RF items
draw their frequency across the icon and a large "0MHz" dominates an 8x8 patch. The outcome is
unchanged (it abstained before the shortlist existed too) but it is no longer *recoverable*, which is
a cost the shortlist introduced and is worth knowing before anyone tackles the overlay.

The 22 margin failures fall into four clusters, and only the last is not a variant family:

| pair | slots | margin |
|---|---|---|
| `ammo.shotgun.slug` against `ammo.shotgun.fire`, both directions | 8 | 0.073 - 0.095 |
| `ammo.rifle.hv` / `.incendiary` against `ammo.rifle.explosive` | 8 | 0.044 - 0.075 |
| `rifle.ak.diver` against `tshirt.long` | 4 | 0.041 - 0.095 |
| `keycard_blue` against `keycard_green` | 2 | 0.084 - 0.086 |

Both gates were set when the contrast floor was the only defence against an empty slot, so both were
re-derived against the full corpus:

| `MinimumScore` | `MinimumMargin` | correct | abstained | **wrong** |
|---|---|---|---|---|
| 0.65 | **0.10** (shipped) | 348 | 25 | **0** |
| 0.65 | 0.07 | 360 | 12 | 1 |
| 0.65 | 0.04 | 369 | 3 | 1 |
| 0.55 | 0.10 | 346 | 23 | 4 |
| 0.35 | 0.03 | 360 | 0 | 13 |

Lowering the score gate is unambiguously bad — 4 to 13 wrong. Lowering the *margin* is the tempting
one: 0.04 recovers all but three abstentions for a single wrong read, and that read is another empty
slot (`wall.frame.lunar2025_b`, at structure ratio 0.389 against a correct-read floor of 0.581).

It is still the wrong trade, for a reason that is not visible in this table. The confusable ammo
pairs' separation was measured earlier at 0.028-0.208 *for the same pair across frames* — the gap
itself moves by more than 0.05 with alignment and lighting. A 0.04 margin would therefore be deciding
inside its own noise, and would be confidently wrong on some capture not in this corpus. 0.10 stays.

Trading the score gate against the margin does not work either. If a large margin could substitute
for a mediocre score, `ballista.bolt.pitchfork` would be recoverable — but the region is not safe:

| `MinimumScore` | `MinimumMargin` | correct | abstained | **wrong** |
|---|---|---|---|---|
| 0.65 | 0.10 (shipped) | **348** | 25 | **0** |
| 0.60 | 0.15 | 334 | 38 | **1** |
| 0.55 | 0.25 | 297 | 76 | 0 |

At any score gate below 0.65 the `rf.detonator` slot reads as `microphonestand` with a margin of
0.215 — a confidently wrong answer. The only rule that would admit `ballista` (margin 0.288) while
excluding it needs a margin threshold between 0.215 and 0.288, fitted to one example on each side.
That is thinner than the noise the corpus already exhibits, so the gates stay independent and simple.

> **The real number to remember:** `MinimumScore` is load-bearing and its whole safety margin on that
> slot is **0.028** — `microphonestand` scores 0.6223 against a gate of 0.65. It is not a comfortable
> threshold; it is a threshold that happens to hold.

#### Weighting the correlation by icon coverage — tried, reverted

`ballista.bolt.pitchfork` scores 0.6256 because it is thin: prongs a pixel or two wide, so most of
the cells inside its mask are barely-covered edge whose predicted value is nearly the background.
Weighting each cell by its alpha, so solid cells count more than edge, targets exactly that.

It works, slightly, and it is not worth it:

| | correct | abstained | wrong | per slot |
|---|---|---|---|---|
| unweighted, shortlist 64 (shipped) | 348 | 25 | **0** | **18 ms** |
| weighted, shortlist 64 | 349 | 23 | **1** | 18 ms |
| weighted, no shortlist | 349 | 24 | **0** | 70 ms |
| weighted, shortlist 256 | 349 | 24 | **0** | 33 ms |

One extra read for 1.8x the time. But the middle row is the important one, and it is not a fact about
weighting at all.

> **Gotcha — the coarse shortlist can be worse than losing a read.** When it drops the *true* item,
> the top of the shortlist is a stranger that nothing opposes, and a wrong answer can clear both
> gates. That is exactly what the weighted/64 row is: `rf.detonator` is absent (coarse rank 181), so
> `microphonestand` wins unopposed. The shipped configuration survives this only because that
> impostor lands 0.028 under the score gate. Raise `CoarseShortlist` past 181 if it ever does not.

Weighting the *coarse* pass the same way, for consistency, is much worse and was also reverted: at
8x8 the weighting concentrates on a handful of cells, and the rifles fall to coarse rank 750+, with
only 227 of 243 true items kept even at a shortlist of 300. The coarse pass wants breadth, not
fidelity to the metric that decides.

> **Gotcha:** an icon-structure floor was tried as a way to kill the `wall.frame` read — it matches
> because that icon is nearly featureless, so it predicts almost no structure and the ratio test is
> trivially satisfied. It does not work: a *correct* read, `door.double.hinged.toptier`, has an even
> flatter icon (0.062 against 0.080). Flatness of the candidate is not the discriminator.

The route that preserves zero-wrong is the look-alike mechanism rather than the gates. The abstaining
pairs sit just under the 0.90 family threshold — `ammo.rifle.incendiary`/`explosive` at 0.838,
`keycard_blue`/`green` at 0.844, `ammo.rifle.hv`/`explosive` at 0.816, `ammo.shotgun.fire`/`slug` at
0.760 — so they abstain instead of being reported as an ambiguous *set*, which would be strictly more
information and still never wrong. Lowering that threshold also loosens the margin gate, since the
margin is taken over the first non-look-alike, so it needs validating on its own. **Done below, and it
works — 349/24/0 to 366/7/0.**

Note which abstentions this would *not* fix, correctly: `rifle.ak.diver` against `tshirt.long` scores
-0.268. Those are genuinely unrelated items that happen to land close, which is exactly the case the
margin gate exists for.

### Grouping look-alikes properly — 349/24/0 to 366/7/0

Lowering the family threshold from 0.90 to 0.75 does rescue the abstentions, and on its own it is
also **wrong**, in two separate ways. Both had to be fixed before it could ship, and neither is
visible in a recall number.

| `AmbiguousAt` | mutual | correct | single-item | abstained | wrong | max set |
|---|---|---|---|---|---|---|
| 0.90 (before) | - | 349 | 207 | 24 | **0** | 2 |
| 0.80 | no | 359 | 196 | 13 | **1** | 4 |
| 0.75 | no | 366 | 188 | 6 | **1** | 4 |
| 0.75 | yes | 366 | 188 | 7 | **0** | 3 |
| 0.75, fallback only | yes | **366** | **207** | **7** | **0** | 3 |

**First failure: a loose group deletes its own opposition.** At 0.80 the corpus produces a confidently
wrong read — `rifle.ak.diver` reported as `water|tshirt.long|blueprintbase|bucket.water`, with the
truth not even in the set. The leader at the refined alignment is `water`, a nearly featureless icon,
and loosening the threshold absorbs `tshirt.long` into *water's* group. `tshirt.long` was the runner-up
whose margin was rejecting the read. Group it and the objection disappears.

This is the same shape as the coarse-shortlist gotcha above: **a wrong candidate does not have to beat
its opposition if something removes it first.**

The fix is to require a look-alike set to be *mutually* alike, not merely alike to the winner. A
degenerate icon resembles many unrelated items without any of them resembling each other, while a real
variant family resembles itself throughout. Measured: slug/fire and the keycards stay grouped;
`water|tshirt.long|blueprintbase|bucket.water` does not.

`MaxLookAlikes = 3` is kept as a second, independent guard even though it never fires at 0.75. The two
fail at different points — mutual grouping alone admits a wrong read once the threshold reaches 0.72,
while the pair is clean everywhere from 0.90 down to 0.65. That is the difference between a threshold
sitting 0.03 from a cliff and one sitting mid-plateau.

**Second failure: recall was being bought with specificity.** Applying the loose threshold everywhere
takes correct reads from 349 to 366 — and single-item answers from **207 down to 188**. Nineteen slots
that had named one item now name a set, because some second candidate newly falls inside the winner's
group. A set beats an abstention and loses to a single item, so this trade is close to worthless: 17
slots gained, 19 blurred.

So the loose threshold is not a replacement for the strict one. It is a *fallback*, tried only on
slots the strict pass has already given up on. Both numbers then move the right way — 366 correct with
all 207 single-item answers intact.

> **Gotcha — read the specificity column, not just recall.** Every configuration in that table is
> "more correct" than the shipped one. Three of the five are worse.

The 7 that remain are the hard ones and none is a family problem: four `rifle.ak.diver` against
`tshirt.long` (genuinely unrelated, correlation -0.268), plus the three score-gate failures
`ballista.bolt.pitchfork`, `t1_smg` and `rf.detonator`.

### Ammo variants cannot be split by the icon — measured, not assumed

Three separate attempts, over the ten labelled ammo slots and their confusable families:

| approach | separation from the nearest rival |
|---|---|
| correlation at 32x32 (shipped) | 0.028 - 0.208 |
| correlation at 48x48 | 0.030 - 0.206 |
| correlation at 64x64 | 0.032 - 0.204 |
| gain-fitted colour residual | 0.012 - 0.167 |
| raw colour residual, no fit | 0.009 - 0.149 |

Resolution changes nothing at all, which settles what kind of difference this is: the variants differ
by a small amount of colour spread over the whole round, not by fine detail a downscale destroys.
Absolute-colour metrics, which correlation discards by construction, do better on some variants and
worse on others — no metric clears 0.10 across the board.

The true variant is nonetheless rank one in every single case, under all five metrics. So the
information is there and it is real; there is just not enough of it to bet on. Reporting the family
is the honest answer, and the count digits remain the only signal that would settle it.

#### The coloured tip helps, and still does not reach the gate

The variants are tinted where they differ — black for explosive, blue for high velocity, red for
incendiary — so a sixth attempt targeted exactly that. Take the pixels where the family members
disagree with each other, un-composite the slot back to icon colour through the shared alpha, remove
one global gain fitted against the *family mean* so no member is favoured, and rank by what is left:

| metric | true variant rank one | worst margin |
|---|---|---|
| colour over the disagreeing pixels, alone | 8/10 | -0.074 |
| correlation alone (shipped) | 10/10 | 0.028 |
| correlation - 2.0 x colour error | **10/10** | **0.054** |

Colour alone is worse than correlation — it puts `ammo.rifle` behind `ammo.rifle.explosive`, in both
frames, systematically rather than by noise. But it is a genuinely *independent* signal, and added to
correlation it roughly doubles the worst-case margin. That is the only thing yet measured that moves
that number at all.

It still does not reach the 0.10 the gate demands, so nothing changes in the shipped matcher. Two
cautions before anyone revisits it: the combination degrades past a weight of about 4.0 (10/10 to
9/10, then 8/10), so the useful range is bounded on both sides; and all ten probes come from two
frames in one lighting condition, which is precisely the variation a colour metric is most exposed
to. The 0.054 should be read as an optimistic bound, not a measurement.

## Reading the counts — stop segmenting

`CountReader` reads the number in a slot's lower edge. Over 114 labelled counts and 116 empty slots,
spanning three resolutions:

| | result |
|---|---|
| counts read exactly, prefix included | **114/114** |
| empty slots correctly silent | **116/116** |

That includes unprefixed magazine counts on the belt (`0`, `2`, `4`), prefixed stacks in the
inventory (`x2` to `x99`), a three-digit `x104`, and a single item that shows no count at all.

The typeface is **RobotoCondensed-Bold** — 191 references in `Bundles/`, beside an asset named
`pickupcondensedamount`. It is Apache-2.0, so unlike Facepunch's icons it is vendored into the repo
rather than read from the install. Confirmed by measurement, not just by name: against 24 labelled
glyphs Roboto scored 20 while every substitute collapsed to 12-14. See docs/hud-layout.md for the
band geometry.

Four decisions carry the result, each of which replaced something that measurably did not work.

**Do not segment.** The first design cut the text into glyphs and classified them. It returned the
wrong glyph count on 11 of 24 strips, and no classifier recovers from that. Sliding the whole
alphabet along the strip removes the failure rather than mitigating it.

**Do not binarise.** Thresholding is what the segmenter was built on, and it is the loss: at 12px a
glyph is mostly anti-aliased edge, and the threshold discards exactly the gradients carrying the
shape. Rendering templates at 12px and binarising them *identically* still made things worse — 20/24
down to 16/24. The mask now only locates text; it never describes it.

**Template on the advance width, not the ink box.** A tight box makes `1` a three-pixel stroke that
correlates with any vertical edge, and leaves the distance to the next glyph undefined. Measured, it
inserted phantom `1`s into half the strings (`x16` read as `7116`). Advances abut, so the next glyph
starts exactly one pixel left of the current one, and `1` must now prove the blank space around it.

**Let the last glyph fix the geometry.** Glyph height and baseline are searched, not measured — the
same text measured 11 to 14 pixels tall across six slots when derived from an ink mask, and only the
slots that landed on 12 scored at all. The rightmost glyph picks both for the whole string, the same
shape as the icon matcher letting its leader pick the alignment.

> **Gotcha:** `x` is lower case, so inside a digit-height cell it is mostly blank and never scores as
> a digit does. Rendering every glyph to the same height is the obvious construction and it is wrong
> — it cost every stack count its prefix, silently turning `x104` into `104`. `x` also never ends a
> count, so it is excluded from the walk entirely and tested once, separately, and more loosely.

### Three thresholds, because they answer three different questions

| gate | value | question | plateau over 114 counts |
|---|---|---|---|
| `MinimumScore` | 0.64 | is there any text here at all? | 0.54 - 0.70 |
| `ContinuationScore` | 0.50 | is there another glyph to the left? | 0.46 - 0.58 |
| `PrefixScore` | 0.66 | is this a stack rather than an ammo count? | 0.58 - 0.74 |

Each sits in the middle of a measured plateau rather than at a tuned point.

> **Gotcha:** these must not be one number. Reusing the anchor's 0.64 for the walk stops it on a
> faint digit and reads `x16` as `6` — a stack reported as an ammo count, which is a *wrong number*
> and strictly worse than reading nothing. In the other direction, lowering the anchor to 0.58 lets
> an empty slot invent a digit. The anchor sits higher than the walk for that reason.

> **Gotcha:** `PrefixScore` was originally 0.35, reasoned rather than measured — 'x' is lower case
> and mostly blank in a digit-height cell, so it "could not score like a digit". Wrong: against
> advance-width templates a real 'x' scores like anything else. The loose gate bought nothing and
> cost a wrong read, the crossbow's empty magazine reading as `x0` — "no rounds loaded" turned into
> "a stack of zero". Note which direction the error ran: a *lower* gate produced the false positive,
> because the thing being tested for was never weak to begin with.

### 114/114 was measured on drawn strips, and live play found the gap

A live capture read a belt slot holding `x30` cloth as `9` for a single frame, with `x30` either
side of it. The corpus never contained this because it is made of static strips: the belt with the
panel shut is read over moving terrain, and none of the three gates has anything to say about a
frame whose background changed.

The value is diagnostic. A misclassified digit would have given `39` or `20` — losing a digit
entirely means the walk in `ReadBand` terminated one glyph early, which is the `x16` → `6` failure
the gotcha above predicts, arriving by a route the thresholds do not cover.

Counts are therefore filtered across frames by `SlotHistory`, on the same rule as identity: two
agreeing frames to publish, two contradicting to withdraw, panel-backed reads trusted outright. This
is a containment, not a fix — `CountReader` still misreads, the value just never reaches a teammate.
`VisionService` logs every contradicted count and saves the frame behind it (at most one a minute),
so the root cause can eventually be worked against real pixels rather than reconstructions.

## The zero-wrong claim has a boundary, and it is the panel

Every slot in the 373-slot corpus was a slot the game was actually drawing. Pointed at the main-grid
and wear geometry on a frame where the inventory is **shut**, the matcher confabulates:

| capture | invented reads (of 31) | what it read |
|---|---|---|
| snow, world view | 10 | `spiderweb` x9, `machete` |
| night, world view | 11 | `tshirt.long` x11 |
| guns, world view | 1 | `tshirt.long` |
| ammo, world view | 1 | `bucket.water` |
| desert / coast, world view | 0 | - |

23 of 186. The mechanism is the structure ratio: it asks whether the slot shows as much structure as
the *candidate* predicts, so a flat icon predicts almost none and a flat patch of snow or night sky
satisfies it. The score and margin gates then pass because nothing better is competing. Flat world
against flat icon is a case the corpus could not contain, because the corpus is made of drawn slots.

Note the repeat offenders. `tshirt.long` is the same near-featureless icon that broke loose look-alike
grouping, and `spiderweb` is another. The gates are weakest exactly where an icon carries little
structure to test against.

> **This makes the inventory-open detector load-bearing, not an optimisation.** A false "open" costs
> up to 31 invented items in one frame. The belt is unaffected — it is drawn in world view too — so
> only the main grid, wear row and backpack are at risk.

### Detecting the panel: three failures and what they had in common

Statistics of what the world happens to look like will not do it. Each of these was measured across
the same 15 labelled frames, and each overlaps:

| signal | shut | open | verdict |
|---|---|---|---|
| spread of slot backgrounds | 0.012 - 0.281 | 0.015 - 0.089 | overlaps entirely |
| autocorrelation at the slot pitch | 0.057 - **0.602** | 0.018 - 0.663 | a shut frame beats most open ones |
| blur ratio, fine over coarse gradient | 0.388 - **0.609** | **0.562** - 0.890 | overlaps |
| **gap darker than both neighbours** | **0.08 - 0.26** | **1.00** every frame | 0.74 clear |

The three failures share one flaw: they measure a property the world can imitate. Snow really is
uniform, night sky really is smooth, and a snowfield is self-similar at 96px just as a slot row is.
There is always a scene that defeats a threshold on scene content.

What the world cannot imitate is an edge at a coordinate we already know. The panel separates slots
with a darker gap, so ask each gap whether it is darker than the two slots flanking it and count the
votes — 38 polls over the main grid. Open frames agree unanimously; shut frames manage a quarter.

Rendered text was the first idea and is worse than it looks: Rust is localised, so a template of the
word "INVENTORY" fails on any non-English client. The geometry carries the same "drawn or not"
property with no language attached.

> **Gotcha — a strict comparison on a flat region votes on rounding noise.** A gap averages 455
> pixels and a slot edge 637. On a perfectly uniform frame the two means are equal in principle and
> differ in the last bits of the accumulation, so `<` splits them arbitrarily and about half of 38
> polls vote yes — a blank screen reads as an open panel. Requiring the gap to win by one luminance
> level of 255 fixes it. All fifteen real captures passed either way, because real captures never
> contain two exactly equal regions; only a synthetic test caught it.

## The held slot

Rust fills the selected belt slot with flat blue — measured at `30,94,141` against the panel's
`68,63,58` — and composites the icon on top unchanged. The icon pixels are identical to the same
item in an unselected slot; only what surrounds them differs. That slot was nonetheless the worst
in the corpus, and on a real capture it answered **`water` at 0.829 for a held assault rifle**: not
an abstention but a confident wrong read, which is the one outcome the gates exist to prevent.

The cause is not the background *estimate*. It is that `Correlation` centred both vectors on a
single mean taken across all three channels. Samples are interleaved `r,g,b`, so a strongly
coloured background gives both sides the same large per-channel offset, and a shared offset is
covariance that exists whichever candidate is being scored. Every candidate is dragged toward 1 and
the margin gate has nothing left to separate. Measured on one held AK: on the panel grey the true
item scores 0.866 and the runner-up 0.351; on the blue fill the whole field bunches into 0.65-0.83
and `water` — a flat, slot-filling icon — comes out on top.

Centring each channel on its own mean removes it. This is **not** the per-channel correlation
rejected above: the scale stays joint, so relative channel amplitude still separates a grey item
from a green one of the same silhouette. It is only the DC term that goes.

It is applied only when the background is chromatic — channel spread ≥ 0.15, against 0.02-0.03 for
every panel grey and 0.42 for the highlight. Applied everywhere it is not free: over the capture
corpus it cost four correct reads (`ammo.pistol.hv`, `ballista.bolt.pitchfork`, `scrap`, `t1_smg`)
and blurred the grenade-launcher, shotgun and planner variants into look-alike sets, because some
of what separates those *is* each channel's DC. Gated, the same corpus gives **13 slots gained and
none lost**: five held Thompsons, four held diver AKs, two held AKs and two wear slots, including
both of the confidently wrong reads.

> **Gotcha — per-channel centring can trip the contrast gate.** Removing each channel's mean also
> removes the variance a flat-coloured item carries between channels. A uniformly coloured icon
> ends up with no spread at all and scores 0. No catalogue icon is that flat, but the first
> synthetic fixture written for this was, and failed for that reason rather than the one under test.

A per-channel *median* background was tried alongside, and does estimate the highlight far better —
`30,94,141` against the mean's `47,100,125`, since the margin it averages also contains the
condition bar, the count text and the edge of the fill. Over the corpus it was a wash: two wear
slots gained, two grey slots lost. It is not shipped. The estimate was never what was broken.

## The empty belt slot in world view

Found by recording 250 belt strips while flying around with a fixed hotbar — see *The capture
corpus* below. With the inventory shut the belt is drawn straight over the scene, and an empty slot
is then a rectangle of landscape. The matcher named one in **21 of the 250 frames**: `spiderweb` at
0.973 against snow, `floor.grill` at 0.846 against floorboards, `tshirt.long` at 0.824 against rock.
Confidently wrong, and the worst shape of wrong — there is no item there to be mistaken about.

`MinimumStructureRatio` is what should catch this and cannot. It was calibrated where an empty slot
is flat panel grey; terrain carries as much structure as any icon, so it passes. The winners give
the mechanism away: faint, texture-like icons matching flat snow and sky.

**What did not work.** Both of these were measured over the corpus and put empty and filled slots
directly on top of each other:

| | empty median | filled median |
|---|---|---|
| margin spread — flat where a panel is drawn | 0.065 | 0.076 |
| brightness relative to the world above the belt | 0.665 | 0.677 |

The slot panel is semi-transparent, so terrain shows through it and neither flatness nor darkening
survives. This is the same wall `InventoryPanel` hit: every measure of what the world *happens to
look like* has a scene that defeats it.

**What works: continuity.** Interpolate across the icon area from the slot's own four margin edges.
An empty slot is the same scene inside as out, so the prediction lands; an item is opaque and
unrelated to what is behind the slot, so it misses. A structural question, not a statistical one,
and indifferent to whether the scene is flat snow or dense grass.

| gate | real reads lost | inventions killed |
|---|---|---|
| colour gap, icon against margin | 0 | 3 / 21 |
| spread ratio | 0 | 0 / 21 |
| edge contrast | 0 | 0 / 21 |
| **continuity residual, cut 0.039** | **0 of 1226** | **14 / 21** |
| continuity + colour gap | 4 | 15 / 21 |

The highest a real read ever measured was 0.0399, so the cut sits just under it and is free.

> **Gotcha — the threshold does not transfer to panel slots.** It is a mean over the whole icon
> area, so its floor moves with the backdrop. Over terrain nothing predicts well and even a rifle
> clears 0.04; over flat panel grey an empty slot predicts perfectly and a *thin* item — a bat, a
> holosight — is diluted below the same number. Applied to panel slots it cost 29 real reads for no
> gain. It is enabled per slot, by `overWorld`.

**Seven survive**, and they are the mirror image of what it catches: dense grass, a signpost, debris
on snow — busy scenes where the middle genuinely does not follow from the edges. All seven scored
0.65-0.76, so a score floor reaches them in principle; in practice it does not, because real reads
live in that band too (a held AK reads 0.75). Killing all 21 that way costs **276 of 1226 real
reads**. Not worth it, and recorded here so it is not retried.

### Second corpus: the position is not the variable, and neither is the catalogue

129 strips with three empty slots — positions 0, 2 and 5 — against the first corpus's single slot 5.
The inventions land **3, 3 and 4**, dead even. `MinimumDeparture = 0.039` is not fitted to one slot's
neighbourhood; the leak rate is 3.3% of empty-slot reads here against 3.8% on the first corpus.

The terrain family is new, though. The first corpus's survivors were snow and grass; these are almost
all **man-made structure** — floor planks, a plank wall, a building edge against sky. Regular strong
texture is the adversarial case for a bilinear prediction by construction, so the residual clears the
cut easily and the matcher then finds something in the grain. Same rate, different reason.

**Raising the cut is not the answer, now measured on both.** Sweeping it over the second corpus:

| departure | wrong | abstained |
|---|---|---|
| **0.039** | 11 | 25 |
| 0.05 | 9 | 60 |
| 0.07 | 4 | 197 |
| 0.10 | 2 | 317 |

Each invention killed costs 17 to 90 real reads — the same shape as the 276-of-1226 arithmetic above,
reached independently on a different terrain family.

**Narrowing the catalogue does not help either.** Restricting candidates to Attire, Resources,
Ammunition, Medical, Component and Weapon — 444 icons of 1249, a 65% cut — moved 11 wrong to **12**.
Every invented name changed (`boatbuildingstation`, `concretepickaxe`, `bigcatmeat` and the fireworks
out; `metalpipe`, `horsedung`, `nucleus`, `workbench.upgrade.surplus` in) and four *new* inventions
appeared where the full catalogue had correctly abstained. Busy terrain correlates at 0.65-0.77 with
*something* in any few-hundred-icon set, so removing the winner promotes the runner-up. The invention
rate is a property of the terrain and the acceptance threshold, not of catalogue size. Recorded so it
is not retried; `tools/BeltCorpus --categories` reproduces it.

> **The one wrong read that is not an empty slot.** A held AK on a washed-out bright backdrop read
> `skull.human` at 0.745 — a filled slot, confidently wrong, the class the zero-wrong invariant is
> actually about. One sample, unexplained, and it survives the category cut. It is the thing in this
> corpus worth pulling apart.

## One frame is the wrong unit — `SlotHistory`

Every gate above asks whether *this* slot in *this* frame holds an item, and on busy terrain that
question has no reliable answer. Across frames it does. Over 129 strips the eleven wrong reads were
eleven *different* items and no two adjacent frames ever agreed, while the bone knife and syringe
were named in all 129 and the AK in 104.

The belt cannot change without the player acting; the terrain behind it changes continuously. A real
item is the only thing that survives its own backdrop moving.

**Withhold an identity until two readings agree, and the corpus goes clean.**

| | wrong | correct | abstained |
|---|---|---|---|
| corpus 1, valid window — baseline | 8 | 1040 | 56 |
| corpus 1, valid window — confirmed | **0** | 1016 | 88 |
| corpus 2 — baseline | 11 | 738 | 25 |
| corpus 2 — confirmed | **0** | 730 | 44 |

Zero wrong on both, for about 2.3% of real reads. That includes the `skull.human` read, which no
per-frame gate reached, and it is the first time the world-view belt has matched the panel's
zero-wrong record.

### Agreement must be between *independent* readings

`InventoryReader` serves an unchanged slot from cache, keyed on a pixel hash. So "two frames agreed"
is trivially true whenever the player stands still, and naive frame-counting would confirm a
stationary invention rather than reject it — laundering one fluke into a confident answer. A reading
only counts when the slot's pixels actually moved; identical pixels are one observation, not two.

That is also why a confirmed identity is *held* while the pixels are unchanged rather than needing
constant re-proof: a stationary player with a real item accumulates no new evidence either.
`SlotHash` is shared by both classes so they cannot drift apart on what "changed" means.

| confirm after | corpus 2 wrong | corpus 2 correct |
|---|---|---|
| 1 | **21** | 749 |
| **2** | **0** | 730 |
| 3 | 0 | 706 |
| 4 | 0 | 673 |

One is worse than no filter at all — with no agreement required the hold keeps a bad answer alive
across frames instead of letting it flicker away. The plateau starts at two, and two is its cheapest
point. Latency is one frame, so about a second at the resting cadence.

Two further decisions, both measured against the same corpora:

- **Agreement is set intersection, not equality.** Ammo variants cannot be split by the icon, and
  over changing terrain which member leads is not stable, so requiring the same leader twice would
  abstain on them forever. The reported set is the intersection, which is narrower than either
  reading alone.
- **Panel-backed readings seed the confirmation instead of starting a count.** Those slots are clean
  over 373 labelled reads, so shutting the inventory must not cost two silent frames re-proving what
  was just read against flat grey.

> **Gotcha — the corpus cannot test the stationary case.** `BeltRecorder` skips frames whose backdrop
> has not changed, so every stored strip differs from the last and all of them count as independent.
> The independence rule is what closes that hole, and it is argued from the cache's behaviour rather
> than measured. A corpus that deliberately parks in one spot would settle it.

Replay with `tools/BeltCorpus --confirm [n]`.

## Compositing over the scene, not over a colour — residuals

A slot renders `alpha * icon + (1 - alpha) * background`, and until now `background` was one averaged
colour taken from the margin. The old docstring gave the reason: the margin is the only region
guaranteed free of item pixels, *"so that is all that can be measured without knowing the answer
first"*. `DepartureFrom` stopped that being true — it interpolates a per-cell prediction of the scene
from the same margin. `SceneFrom` samples that field onto the signature grid.

`CompositedThreshold` is 0.1, so cells that are only a tenth icon are compared, and nine tenths of
their predicted value was that flat colour. Over panel grey it is a fair guess; over grass or planks
it is wrong at every cell, and the error lands in the candidate's variance. Sparse icons — bows,
arrows, rods, anything mostly outline — carry the most of it.

**Correlating the values would have reintroduced a solved bug.** Both sides then contain the scene,
and shared structure is covariance that exists whichever candidate is scored — the flat-blue held
slot failure in `Measure`, spatially varying instead of constant, and growing with the scene's own
contrast. Subtracting the prediction from both sides removes it by construction:

```
slot - scene        against        alpha * (icon - scene)
```

what the item adds, against what the candidate says it should add. An empty slot adds nothing, so its
residual is noise about zero and the existing contrast gate rejects it with no fitted threshold.

| | correct | abstained | wrong |
|---|---|---|---|
| corpus 1 — flat | 1040 | 56 | 8 |
| corpus 1 — scene | 1062 | 35 | 7 |
| corpus 2 — flat | 738 | 25 | 11 |
| corpus 2 — scene | 751 | 16 | 7 |

Recall up on both, abstentions down by a third, and wrong reads *down* rather than up — the risk did
not materialise, which is the evidence that the residual form is doing what it claims. Combined with
`SlotHistory`:

| | correct | abstained | wrong |
|---|---|---|---|
| corpus 1 — shipped | 1028 | 76 | **0** |
| corpus 2 — shipped | 740 | 34 | **0** |

Corpus 2 is now better than the original flat baseline on both axes at once: 740 correct against 738,
with none of the eleven wrong reads. Corpus 1 pays 12 reads of 1040 for the same result.

Applied only over the world, so the 373-slot panel corpus is untouched by construction — the flat
path and every threshold calibrated on it still run there. The coarse shortlist also stays flat: it
only has to keep the right item in the top 64, and gating twice is another way to lose one early.

> **Gotcha:** this shares a statistic with `MinimumDeparture`, which is otherwise an independent
> check on the same slot. Measured, that costs nothing. It is still the first thing to look at if a
> later corpus disagrees, because the gate can no longer fail independently of the score.

`tools/BeltCorpus --flat` restores the averaged-colour path for comparison.

## The capture corpus

`%LOCALAPPDATA%\RustOverlayData\belt-corpus` — 250 belt strips, ~30 MB, recorded in about eight minutes
of flying with the hotbar held fixed. Only the strip is kept: a full frame is ~3 MB and the belt is
all that is measured, and the filename carries the frame size and the strip origin so slot rects
rebuild exactly.

Recorded by `belt-recorder` and scored by `belt-corpus`; see docs/matcher-tools.md for how to
run another session and what to go looking for. The corpus itself is never committed — it is
screenshots of a real session.

A fixed hotbar makes every frame self-labelling, which is what makes this cheap to produce. Two
things to know when reading results off it:

- **Anything the player does mid-session breaks the labels.** A medkit was used from slot 5
  partway through, which silently turned that slot's ground truth from an item into empty. The
  empty-slot finding above survives this because it needs no item labels at all — an answer on a
  slot known to be empty is wrong whatever the other slots hold.
- **The first corpus's labels are only valid for a sub-range**, for that reason. Slot 5 held the
  medkit for the opening 66 frames, so scored end to end it reports 68 wrong against the 8 real
  ones. Its valid window is `--from 105408`, the last 184 frames.

Keep sessions in separate directories so each stays scoreable against its own truth, and record that
truth here — corpus 2's went unwritten for a release and had to be recovered by reading off what the
matcher named most often.

| corpus | strips | truth |
|---|---|---|
| `belt-corpus` | 250 | `rock,knife.bone,rifle.ak,rifle.ak,syringe.medical,-` — valid only with `--from 105408` |
| `belt-corpus-2` | 129 | `-,knife.bone,-,rifle.ak,syringe.medical,-` — labels good throughout |
| `belt-corpus-skins` | 236 | `rifle.ak,door.hinged.metal,door.hinged.metal,door.hinged.metal,smg.thompson,-` — every item skinned |

**Both corpora are 1440x1080 at one quality setting.** Resolution independence was established for
panel slots (three resolutions, sixth set) but never for the belt drawn over terrain, and quality
settings have never been varied at all. Every constant in the world-view path is measured on one
display configuration.

## Known gaps

- **Ammo variants are now reported as a set, not separated.** They are grouped by the loose fallback
  rather than abstained on, so a slot reads as "one of hv / incendiary / explosive" with its count.
  Splitting the set still needs a non-icon signal: the coloured tip roughly doubles the margin and
  still does not reach the gate. The count digits read, so the plan remains to track a family's count
  across frames, since two variants in the same family do not consume together.
- **Backpack capacity is not read.** The slot now has geometry and the pack itself identifies, but
  the `0/28` on it is a fill count, not a stack count. `CountReader` would read the `0` and report a
  stack of zero, so it must not be pointed at this slot until a capacity reader exists.
- **Three slots fail the score gate, not the margin gate**, and the gate cannot move: at 0.55 the
  corpus produces four wrong reads, at 0.35 thirteen. `t1_smg` reaches 0.486 in a world-view belt
  slot, `ballista.bolt.pitchfork` 0.371, and `rf.detonator` 0.565. These need better scores,
  not a lower bar. The RF one is its own category: the game draws the frequency ("0MHz") across the
  icon, which no catalogue entry accounts for, and that also drops it to coarse rank 181.
- ~~**A held slot's blue tint still costs the read.**~~ **Fixed** — see *The held slot* below. The
  four abstentions that were `rifle.ak.diver` against `tshirt.long` went with it.
- **World-view slots are less reliable than inventory ones.** With no panel behind them the icon
  composites over terrain, and one slot that resolves in the inventory frame abstains in the world
  frame of the same session. Measured over the capture corpus: ~5% of filled belt slots abstain,
  and two identical AKs one slot apart can differ, one over rock and one over grass.
- **Seven empty-slot inventions remain**, on busy terrain. See *The empty belt slot in world view*
  — a score floor is not the answer and the arithmetic is recorded there.
- **Nothing vectorised yet.** The scoring loop is scalar `double` over `float` spans. Now that it is
  one pass it is a clean target for `TensorPrimitives`, but the cold frame is already 31 ms and the
  steady state 0.3 ms, so this buys nothing until something else needs the headroom.

## Earlier: verified against real slots

The shipped PNG **does** match what the game renders. Cropping belt slot 2 from a real capture puts
`rifle.ak` top at 0.603, 0.28 clear of the runner-up. No draw-time shader or transform defeats the
correlation, so extracting the real UI sprites from the client bundles is unnecessary.

But real-slot scores sit far below the simulated ones, and the thresholds above were fitted to the
simulation:

| | synthetic | real slot |
|---|---|---|
| mean top score | 0.977 | ~0.6–0.85 |

Observed on one frame: `syringe.medical` at 0.841 (correct), `rifle.ak` at 0.603 (correct),
`ballista.static` at 0.633 for a slot holding a machine gun — **wrong, and it clears both gates**.

> **Gotcha:** the synthetic round trip renders the icon to fill the slot exactly. The game insets it,
> and adds a condition bar and a count. Matching a naive slot crop therefore compares differently
> framed images, which depresses every score and compresses the gap between right and wrong. Fit the
> crop to the icon's actual placement within the slot before trusting any threshold.

Both of those are now done — the crop is `SlotRect.IconArea`, and the gates are calibrated against
86 labelled real slots rather than the simulation. The `ballista.static` false positive that
motivated the warning no longer occurs.

## The icon source, and skins

Two findings from the 2026-08-09 bundle and metadata extraction (see docs/hud-layout.md for the
tooling and the geometry half). Both bear on matching and neither is handled yet.

### The 512px PNG is the same image the game draws — measured, don't retry this

The in-game sprite and the PNG on disk *look* like different assets:

| | `Bundles/items/*.png` | in-game `icon` texture |
|---|---|---|
| size | 512x512 | 256x256 |
| format | 8-bit RGBA | BC3/DXT5 (11 of 1242 are BC7) |
| mips | none | 9, bilinear, aniso 16 |

They are not. **The 512 PNG is an upsample of the same 256 master, not an independent render.** Its
error against its own 256→512 round trip is the same magnitude as its difference from the BC3
texture. Downscaled to the 32x32 signature the two agree to within 0.1-0.2% MAD:

| item | RGB corr | RGB MAD | alpha corr | mask cells 512 / 256 |
|---|---|---|---|---|
| rock | 0.9989 | 0.0022 | 0.999998 | 616 / 616 |
| knife.bone | 0.9995 | 0.0011 | 0.999997 | 244 / 242 |
| rifle.ak | 0.9937 | 0.0013 | 0.999995 | 216 / 216 |
| syringe.medical | 0.9996 | 0.0012 | 0.999996 | 307 / 307 |

Alpha — the channel most at risk from BC3 — is the *most* identical of the four, because BC3 has a
dedicated alpha block and the box filter to 32x32 erases what quantization remains. At full
resolution `rifle.ak`'s **PNG** is the more alpha-quantized of the pair (106 distinct values against
the texture's 192).

Scored over the corpus, truth-item similarity moves **+0.0018 pooled** (920 slots, 98.6% of them
improving, largest per-item gain `rifle.ak` at +0.0033). Against gates calibrated at 0.10 and 0.35
that is three orders of magnitude short of mattering. Nothing changes rank order.

> **Verdict: do not build a bundle-reading path.** Not because it would be expensive — it would not.
> `items.preload.bundle` (2.5 MB) maps every shortname to a CAB and PathID, the texture bundles ship
> with **compression flag NONE** so it is pure seek-and-read, and all 1242 icons extract and decode
> in ~2.5 s warm from ~110 MB of scattered reads. It buys nothing, which is the better reason.
>
> Note the mapping must go through `ItemDefinition.iconSprite`, not names: `rifle.ak`'s texture is
> `ak47u.icon`. A name-matching approach would get that one wrong.

### Skins break the icon↔itemid assumption

> Superseded by [Skins](#skins) below, which measures this and fixes most of it. Kept for the
> decompiled shape of `ItemSkin`, which is still the reference for `Redirect`.

```
public class ItemSkin : SteamInventoryItem {
    public Sprite inGameIcon;
    public ItemDefinition Redirect;
}
```

`VirtualItemIcon` carries `public ulong skin` beside `public Image iconImage`, and `ItemIcon`'s
sprite resolver (obfuscated, but its signature returns `Sprite` plus a tint and a float) assigns the
result to `iconImage.sprite`. Consequences:

- A skinned item renders `ItemSkin.inGameIcon`, often a Workshop texture that is **not in
  `Bundles/items` at all**, so it cannot match the vanilla PNG under any geometry or colour model.
- `Redirect` points at a different `ItemDefinition`, so even a correct icon match does not uniquely
  determine the item id.

## Skins

The section above ended by asking whether an unmatched skin abstains or confabulates. It was
measured, and the answer was worse than expected. What follows replaces it.

### The skin icons ship on the player's disk

`steamapps\workshop\content\252490\<workshopid>\` — one directory per skin, holding `icon.png` at
**512x512 RGBA, the same format as `Bundles/items`**. Drop-in for `IconSignatures.FromFile`, alpha
channel and all. `workshop_icon.png` sits beside it and is the store thumbnail, not what a slot draws.

Same "read from the player's install, never vendor" property as the item bundles, plus one more:
Steam fills the cache on demand as the player meets skins, so it self-updates and covers what that
player actually sees.

`manifest.txt` names the item a skin paints, as a display label:

```json
{"Version": 3, "ItemType": "AK47", "AuthorId": 76561198218106369,
 "Groups": [{"Textures": {"_MainTex": "_MainTex0.png"}, "Floats": {...}, "Colors": {...}}]}
```

Two things follow from the schema. Manifests carry **material properties and never a mesh**, so a
skin is the same render in different paint — the silhouette is guaranteed identical up to `_Cutoff`.
And `ItemType` is a closed vocabulary: 85 distinct labels across the 1,457 cached skins, of which 61
are a definition's own `Name` verbatim. The remaining 24 are in `SkinItemTypes`, which is a name
table and carries the caveat the section above gives for icons — with the mitigation that it fails
safe, and `SkinCatalog.UnmappedLabels` makes drift visible instead of silent.

### It was not an abstention problem, it was a wrong-read problem

Every cached skin put through a simulated slot render against the shipped 1,249-icon catalogue:

| | vanilla control | skinned |
|---|---|---|
| slots | 1,249 | 1,457 |
| correct | 95.5% | **1.4%** |
| abstained | 3.0% | 97.7% |
| **confidently wrong** | **0** | **12** |

`rock → chineselantern`, `bbq → flare`, `smg.thompson → vehicle.2mod.flatbed`. Zero-wrong was
already broken, in the one place nothing was looking.

### Skins are extra faces of the same item

The catalogue is keyed on `IconFace` — an item paired with one picture of it. A vanilla item has one
face; a skinned one has as many as the player has cached. Faces never leave the matcher: `IconMatch`
names an `ItemDefinition` exactly as before.

| | correct | abstained | wrong |
|---|---|---|---|
| before | 1.4% | 97.7% | 12 |
| faces in the catalogue | 90.8% | 9.2% | 0 |
| **+ same-item look-alike skip** | **97.3%** | 2.7% | **0** |
| vanilla, with skins loaded | 95.5% | 3.0% | 0 |

The vanilla row is **identical to the row measured without skins** — 1193 correct, 19 in-set, 37
abstained, on both. 1,457 extra candidates cost the unskinned path nothing.

> **Gotcha:** `IconFace` exists rather than a second `ItemDefinition` carrying the skin's path, and
> the reason is not tidiness. `ItemDefinition` is a record, so a skinned AK and a vanilla AK would be
> unequal *by value* — and `SlotHistory.Intersect` puts them in a `HashSet` to intersect readings
> across frames. Two frames disagreeing only about which picture an AK wore would intersect to
> nothing and withdraw a correct read. Single-frame scoring cannot see this; it is why the
> measurement above is not sufficient on its own.

The 90.8% row is the other half worth keeping. `MaxLookAlikes` was built for *different items that
look identical*; skins are the exact dual, so 163 mutually-similar AK skins tripped the cap and
abstained. A candidate naming the item that already won is skipped outright — neither a rival to take
the margin over nor an ambiguity to report. It is not a hole in the gate: a genuine rival is still a
rival however many faces the winner has, which `A_skinned_catalogue_still_abstains_when_two_items_are_too_close`
pins down.

### The cache moves while the game runs

`VisionService` used to build the catalogue once, on the note that *"neither changes while the game
runs"*. False for skins — during one session three directories were written at 10:03:38, 10:03:35
and 10:03:31, minutes after launch.

`SkinWatcher` is event-driven off a `FileSystemWatcher`, and sweeps on a 2-minute backstop anyway,
because the watcher **silently drops events when its buffer overflows** and Steam writes these in
bursts — 547 directories in one month here. Joining a populated server is exactly when a burst
happens and exactly when the drop would be invisible.

The watcher only raises a flag; every directory read and every mutation happens in `Sweep`, so
nothing crosses threads but one boolean.

Steam also writes a skin's files into their final directory one at a time rather than moving a
finished one into place, so a sweep can land mid-write:

```
3771963093/  _BumpMap0.png     10:03:38.218
             icon.png          10:03:38.394   <- an event can arrive here
             manifest.txt      10:03:38.394
             (dir finalised)   10:03:39.357
```

Such a directory is left unremembered and retried. The icon half is the same case one layer down —
only the decode can see a truncated PNG — so `IconSignatures.Build` reports what would not decode and
`SkinCatalog.Forget` puts it back in the queue.

### The coarse shortlist had to become a fraction

Simulation said this was free. The recorded corpus said otherwise, and it is the one finding here
that no amount of icon-against-icon scoring could have produced.

Scored over 1,500 real belt slots, skins on, everything else as shipped:

| | correct | abstained | wrong |
|---|---|---|---|
| vanilla only | 1262 | 233 | 5 |
| **skins, `CoarseShortlist` 64** | **1172** | **320** | **8** |

90 correct reads gone and three new wrong ones. `CoarseShortlist` was a flat 64 — justified against
239 labelled slots where the true item's coarse rank was 11 or better in 238 of them. That
justification holds only while the catalogue is one icon per item. Adding 1,457 skin faces left the
same 64 places to be fought over by twice as many candidates, and skins *of other items* crowded the
true one out before anything looked at it closely.

Two fixes that do not work, both measured:

- **Raise the number proportionally.** 139, the like-for-like equivalent, still gives 1256 / 7.
- **Dedupe the shortlist by item**, keeping each item's best face. Worse — 1137 / 4. The 8x8 pass
  picks a different face than the 32x32 would, so collapsing to one face depresses that item's real
  score below the margin.

What works is treating the shortlist as a *share* of the catalogue, because what it has to survive is
competition and competition is proportional:

| depth | vanilla (1,249 faces) | with skins (2,706 faces) |
|---|---|---|
| 64 | 1262 / 5 | 1172 / 8 |
| ~20% | 1274 / 4 | 1272 / 3 |
| ~30% | 1278 / 4 | 1276 / 2 |
| unlimited | 1283 / 3 | 1281 / 2 |

Shipped at a quarter, which is better than the flat 64 **unskinned as well** — the fixed number had
been quietly costing reads before skins existed:

| | correct | abstained | wrong |
|---|---|---|---|
| vanilla, flat 64 | 1262 | 233 | 5 |
| vanilla, quarter | 1277 | 219 | 4 |
| **skins, quarter** | **1273** | 225 | **2** |

### Cost

Decoding dominates startup and is one bitmap per file, so it parallelises. GDI+ forbids several
threads over *one* bitmap, which is what `FrameSlotSampler` exists for — not several bitmaps.

| | serial | parallel, 16 cores |
|---|---|---|
| vanilla (1,723 icons, 354 MB) | 7.38 s | 2.48 s |
| skins (1,457 icons, 283 MB) | 6.76 s | 2.13 s |
| **both (3,180 icons, 637 MB)** | 14.79 s | **5.49 s** |

So loading skins as well is *faster than the catalogue alone used to be*. Signatures cost about 12 MB.

Matching goes 10.8 ms a slot to 14.5 — the deeper shortlist, not the extra candidates. That lands on
far fewer slots than it looks: `InventoryReader` skips any slot whose pixel hash is unchanged and runs
the rest through `Parallel.For`, so the worst case is the frame after the inventory opens, 37 fresh
slots at roughly 34 ms of wall clock.

### Which skins are carried, and why most are not

`belt-corpus-skins` is the third session and the first with skinned items in it: 236 strips holding a
skinned assault rifle, three skinned sheet metal doors, a skinned Thompson and one empty slot. It is
what settled this, and it settled it against the simulation.

Admitting every cached face, through `SlotHistory` at the shipped `--confirm 2`:

| | correct | wrong |
|---|---|---|
| no skins | 236 | 2 |
| every face | 474 | **23** |
| allowed categories only | 466 | **0** |

The middle row is the whole argument. Skins are not optional — without them the matcher reads *none*
of the five items, abstaining on all 1180 filled-slot reads, while the rifle alone goes to 230 of 236
with them. But admitting all of them breaks the invariant on real frames, through the production
filter, and the cost is not spread out: every one of the 23 is a door slot, and every one names either
`locker` or `burlap.headwrap`.

Two different failures wear the same face:

- **`locker` (11) is an honest look-alike.** Its skins cover 96% of the canvas as a dark filled
  rectangle, which at 32x32 is a skinned sheet metal door. Correct behaviour here is to abstain, but
  the pair correlates below `LookAlikeAt` and never forms a set.
- **`burlap.headwrap` (12) is not.** Its three cached skins cover 8%, 13% and 32% of the canvas
  against 47% for the vanilla icon — they are rendered small in frame. Scoring is masked by the
  *candidate's* alpha and the only floor on that is 24 samples, eight cells of 1024, so a small icon
  is graded solely on the fragment it claims. It landed on a patch of door and won it twelve times.
  The asymmetry is the tell: a small candidate can beat a large true item, never the reverse.

`SkinCatalog.DefaultCategories` is the fix that measured well. It is an allow-list, so an unmeasured
category stays out, and it drops `Construction` and `Items` for the first failure and `Attire` for the
second. Across all three corpora at `--confirm 2` that is **0 wrong everywhere**, and the two older
corpora land within one read of carrying no skins at all — 1036 against 1037, and 747 against 747.

`Attire` is the one to revisit: 392 of 1467 cached skins, dropped on the evidence of one item, and no
corpus yet holds a skinned *armour* slot, which is where attire skins would pay. Widen the list and
re-score before assuming it must stay out.

### Rejected: a gate on what a candidate leaves unexplained

The `burlap.headwrap` mechanism suggests an obvious fix, and it does not work. Recorded because it
looks right twice.

The idea is the complement of `PredictsTheStructure`: that asks whether the slot carries the structure
the candidate predicts, so also ask whether it carries structure the candidate does *not* — compare
the residual outside the candidate's alpha mask against the residual inside it. A correct read leaves
the scene behind it and the outside residual is prediction error; a fragment leaves the rest of the
real item.

Measured as a **spread** it is inert. The item a fragment sits on is a large but roughly *constant*
offset from the scene — a painted door is flat — so its variance about its own mean is small. Every
threshold from 1.5 up scored identically to the gate being off; 0.7 and 1.0 only rejected true items,
costing 138 of 236 rifle reads and *raising* wrong reads from 23 to 47.

Measured as a **mean magnitude** it works and is still not worth shipping. The best point, 0.70, takes
the skinned corpus from 12 wrong to 3 — but costs 17 correct reads on corpus 1 and 12 on corpus 2, and
sits on a cliff: 0.75 is back to 11 wrong. Against the category allow-list, which reaches 0 wrong for
one read, it is strictly worse on every axis. Combined with the allow-list it *adds* two wrong reads.

Not in the code. The underlying hole is real — nothing bounds how little of a slot a candidate may
explain — but it is a hole the allow-list closes from the other side, and no threshold on this
statistic pays for itself.

### What is still uncovered

- **Store skins, and they are not a footnote.** `<Rust>\temp` holds another 486 unlabelled 512x512
  RGBA icons — Item Store skins, no manifest, so nothing says which item they paint. This is what the
  Thompson in `belt-corpus-skins` is: 19 Thompson skins sit in the workshop cache and the label maps
  cleanly, yet that slot abstains on all 236 frames both with skins and without, because the one the
  player owns is not among them. One of the five held items, unreadable for this reason alone.
- **`ItemSkin.Redirect`** still means an icon match does not uniquely determine an item id. Untouched.
- **The simulated 97.3% did not survive contact.** It was the icon composited over flat grey, which
  measures ambiguity rather than render noise, and it said skins were free. On real frames they cost
  23 confirmed wrong reads until the catalogue was narrowed. Two of the three things that mattered
  here — the coarse-shortlist regression and this — are things simulation missed. Score a corpus.
- **Silhouette matching is not a fallback.** Alpha-only correlation puts the true item at rank 1 for
  only 29% of skins, median rank 14. Shape is not discriminative at 32x32, so an uncached skin
  abstains rather than degrading gracefully. Do not retry this.
