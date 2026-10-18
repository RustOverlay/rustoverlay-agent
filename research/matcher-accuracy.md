# Matcher accuracy on the belt corpora — error taxonomy and ranked fixes

Research, 2026-10-17. Read-only on the repo; every prototype lives in a scratch copy of
`ro-core`, `ro-vision`, `ro-capture`, `belt-corpus` (scratchpad `ws/`), driven by env-var knobs.
Baseline numbers were taken with the repo's `belt-corpus` release build and reproduced bit-for-bit by
the scratch build with every knob off. Bit-exact C# parity is no longer a constraint, but nothing
below needed to break it except where stated.

## Headline

- **The skins gap is not a matcher problem.** 944 of the 950 confirmed abstentions in `skins-confirm`
  are the three doors and the Thompson, and **none of their four skin icons exists on disk in any
  source we know of** — workshop cache, `<Rust>\temp`, or `Bundles/items/*.skin.png`. No scoring or
  threshold change can read them. Lowering the gate to try only produces *consistent* wrong reads
  that `SlotHistory` cannot catch (46 confirmed wrong at `minimum_score` 0.55).
- **The biggest recoverable loss is `SlotHistory` forgetting the belt on a gap.** 60 of the 67
  `c1-confirm` abstentions and 24 of the 27 `c2-confirm` ones are whole-belt re-confirmations after
  a >4 s gap. Demoting instead of clearing gives **+46 / +24 / +5 reads, 0 wrong**.
- **Most "WRONG" in the unconfirmed c1 number is labelling.** 59 of 63 are a medkit that really was
  in slot 5 (the matcher was right); the other 4 are one frame of a desktop window.
- **A skin coverage gate makes Attire and Construction skins admissible at zero confirmed wrong**
  on all three corpora — the allow-list can widen without reintroducing `burlap.headwrap`.

## Method

Scratch tools (in the session scratchpad, not the repo):

- `diag` example — per slot: departure, decision, top-N `(item, score, category)` from
  `rank_sampler`, with optional extra icon pools: all workshop skins regardless of category,
  `Bundles/items/*.skin.png`, and every `<Rust>\temp\*.png`, each tagged with its source so the
  winning *face* is visible. Run with the coarse shortlist off (full 32x32 over 3,611 faces) to
  rule out shortlist loss.
- Knobs in the scratch `belt-corpus`: `GAP_KEEP` (history demotion), `PRIOR_SCORE/PRIOR_MARGIN`
  (hysteresis towards a confirmed item), `MIN_SCORE/MIN_MARGIN`, `SKIN_COV` (skin alpha mass over
  the item's vanilla alpha mass), `BUILTIN_SKINS` (load `*.skin.png`), `SKIN_MIN_SCORE` (a higher
  score gate when the winning face is a skin).

Configs as in the brief, plus `c1v` (`--from 105408`, unconfirmed) and `c2-confirm`.

## Baseline

| config | correct | abstained | WRONG |
|---|---|---|---|
| c1 | 1409 | 28 | 63 |
| c1v (`--from 105408`) | 1084 | 16 | 4 |
| c1-confirm | 1037 | 67 | 0 |
| c2 | 763 | 6 | 5 |
| c2-flat | 755 | 14 | 5 |
| c2-confirm | 747 | 27 | 0 |
| skins | 462 | 900 | 54 |
| skins-confirm | 466 | 950 | 0 |

## Error taxonomy

### c1 (250 strips x 6)

| class | count | evidence |
|---|---|---|
| WRONG — label artefact | 59 | slot 5 `largemedkit` at 0.83-0.95, frames before 105408. The montage shows a medkit in every one; the truth csv is only valid from 105408 (docs/item-matching.md, *The capture corpus*). Correct reads scored as wrong. |
| WRONG — not a game frame | 4 | `belt-20260809-110258-932`: a terminal window covers the belt (taskbar visible); slots 1-4 read `movembermoustache` 0.72-0.82. The recorder now refuses non-foreground frames (`belt-recorder/src/main.rs:78`) and so does the agent (`vision_service.rs:142`); this frame predates that gate. |
| abstain — not a game frame / geometry off | 6 | `105609-130` (taskbar overlapping the belt, departure 0.009, every score 0) and the rock slot of 110258. Correct behaviour. |
| abstain — truth not rank 1 | 13 (c1v) | AK over busy terrain; leader is an unrelated item at 0.51-0.59 (`flare`, `shotgun.pump`, `skull.human`...). |
| abstain — truth rank 1, score < 0.65 | 3 (c1v) | `rifle.ak` 0.56 / 0.65; one syringe at 0.35 in the taskbar frame. |
| confirm-only abstain — history reset | 60 | 12 whole-belt `-----.` frames in `c1-confirm`: the first frame, plus 10 gaps >4 s in the window (`SlotHistory::max_gap`, `slot_history.rs:87-91`). Count from strip names: 10 gaps in the window, median gap 1.2 s, max 167 s. |
| confirm-only abstain — single slot | 7 | busy-terrain AK frames that never got a second agreeing read. |

### c2 (129 strips x 6)

| class | count | evidence |
|---|---|---|
| WRONG — empty-slot invention | 4 | slot 2 `cannonball` 0.73 / `concretepickaxe` 0.66 / `grenade.molotov` 0.74, slot 5 `water` 0.88; plank, sky and road textures (montage `c2.png`). All single-frame, all removed by `--confirm 2`. |
| WRONG — filled slot | 1 | slot 3 AK behind snowy branches -> `boatbuildingstation` 0.669 (`115433-895`). Single frame; confirm removes it. |
| abstain — AK on busy terrain | 6 | 3 truth not rank 1 (`diving.tank` 0.57, `50cal.mounted` 0.55, `door.hinged.toptier` 0.61), 2 rank 1 at 0.63, 1 rank 1 at 0.66 with margin 0.06 over `flare`. |
| confirm-only abstain — history reset | 24 | 8 gaps >4 s x 3 filled slots. |
| flat vs scene | -8 correct | `c2-flat` loses 8 AK reads to scene compositing; confirms the shipped residual path. |

The vanilla matcher is in good shape on filled slots: the true item is rank 1 in 1,225 of 1,250
filled c1 slot-reads (all frames, including the two non-game ones) and 383 of 387 in c2 (`ana.py` over the diag dumps). What is left is busy
terrain pushing a correct leader under 0.65, which is the same 0.65 gate that keeps
`microphonestand` out (docs, *Why the remaining abstentions...*), and the per-frame inventions
`SlotHistory` already filters.

### skins (236 strips x 6)

| slot | truth | abstained | WRONG (per frame) | after `--confirm 2` |
|---|---|---|---|---|
| 0 | rifle.ak (held, skinned) | 1 | 0 | 6 abstain (5 gap resets + warm-up) |
| 1 | door.hinged.metal (red, figure) | 235 | 1 | all abstain |
| 2 | door.hinged.metal (green ornate) | 218 | 18 | all abstain |
| 3 | door.hinged.metal (grey panels) | 210 | 26 | all abstain |
| 4 | smg.thompson (dark) | 236 | 0 | all abstain |
| 5 | empty | — | 9 | 0 |

Wrong reads by name: slot 3 `medieval.door.hinged.metal` x20 (same door silhouette — it is the
redirect item of the medieval skin), slot 2 `50cal.mounted` x17 (its dark green plates match a
patch of the green door), plus scattered `sign.wooden.*`, `boomerang`, `sail`, `cannonball`. Every
one is single-frame; confirmation removes all 54.

#### Why the skins corpus reads 33%: the icons are missing

Run with every icon source we have — 1,249 vanilla, 1,505 workshop skins *in every category*, 278
`Bundles/items/*.skin.png`, 524 `<Rust>\temp` store icons — and the coarse shortlist off:

| slot | best score, median [max] | margin over next item, median | true item rank 1 | top-1 face |
|---|---|---|---|---|
| 0 AK | 0.867 [0.944] | 0.337 | 235/236 | one workshop face, `2977171702`, 235 times |
| 1 door | 0.665 [0.818] | 0.024 | 13/236 | `frankensteins.monster.01.head` 104, `boatbuildingstation` 59, a store icon 19, `locker` skin 18 ... |
| 2 door | 0.642 [0.815] | 0.002 | 27/236 | a flower-crown store icon 70, `skull.trophy` 24+18, a door "skin" 20, `50cal.mounted` 15 ... |
| 3 door | 0.681 [0.926] | 0.024 | 33/236 | `pants` skin 34, medieval door (skin + redirect item) 31+30, an AK skin 22 ... |
| 4 Thompson | 0.508 [0.601] | 0.026 | 44/236 | a store shotgun 65, `bow.hunting` skin 53, a Thompson skin 44 ... |

A slot whose face is in the catalogue looks like slot 0: one face, every frame, high score, wide
margin. The other four look like a missing face: the leader changes identity frame to frame, the
margin is noise, and the "true item at rank 1" cases are other door skins winning by silhouette.
The 40 cached `Sheet Metal Door` and 20 `Thompson` workshop skins are not the ones the player owns,
and the workshop directories written just before the session (07:49-07:50 on 2026-08-14) are a
backpack, hoodie, pants, two helmets, an axe, a BBQ, a turret, a door of another type and an AK.
`<Rust>\temp` is the main-menu store/news image cache (the `.jpg`s are devblog banners).

So: **missing skin signatures, not rejection, not thresholds, not labelling.** The truth csv is
right (the strip shows the AK, three different painted sheet metal doors and a Thompson).

Two further facts from the same run that bear on the fixes:

- **Some cached skins are not pictures of the item.** Workshop `3380345508`, labelled
  `Sheet Metal Door`, is two hinge plates on a transparent canvas. Faces like it and the small
  `burlap.headwrap` skins are what wins fragments of a real item. Coverage ratio (skin alpha mass
  over the vanilla icon's) is < 0.6 for 107 of 1,505 cached skins.
- **Workshop icons are not framed like the vanilla icon**: alpha IoU against the vanilla icon is
  0.5-0.7 for most skins, so a silhouette-IoU gate would reject good faces. Coverage ratio is the
  usable statistic.

## Experiments

All at the shipped gates unless stated. "skins on" = `--skins` with the shipped allow-list.

### SlotHistory: demote on a gap instead of forgetting

`GAP_KEEP`: on `read_at - last > max_gap`, a confirmed identity becomes *pending* one agreement short
of `confirm_after`, and `seen` is cleared so the next frame counts. A fresh read that agrees
re-confirms immediately; anything else needs the normal two agreements.

| | c1-confirm | c2-confirm | skins-confirm |
|---|---|---|---|
| shipped | 1037 / 67 / 0 | 747 / 27 / 0 | 466 / 950 / 0 |
| `GAP_KEEP` | **1083 / 21 / 0** | **771 / 3 / 0** | **471 / 945 / 0** |
| + prior hysteresis 0.50 / 0.05 | 1085 / 19 / 0 | 771 / 3 / 0 | 471 / 945 / 0 |
| + prior hysteresis 0.45 / 0.03 | 1085 / 19 / 0 | 771 / 3 / 0 | 471 / 945 / 0 |
| prior hysteresis alone | 1039 / 65 / 0 | 747 / 27 / 0 | 466 / 950 / 0 |

c2-confirm with the fix (771) beats unconfirmed c2 (763) with zero wrong against five. The gain is
exactly the gap count: 10 gaps x ~5 filled slots in c1, 8 x 3 in c2, 5 x 1 in skins.

Part of this is a replay artefact: `BeltRecorder` skips strips whose backdrop has not changed, so the
corpus has 4 s+ holes the live agent would fill with cached-unchanged frames. In production the gap
is focus loss — alt-tab or a menu — and today that costs two silent frames per slot on return.
Both are worth fixing; only the second changes the product.

Risk: the only new path to a wrong read is a slot that changed during the gap *and* whose first
fresh read invents exactly the item that was there before — an empty-slot invention matching the
departed item. Inventions over 129 + 250 strips never repeated the same name in adjacent frames, and
naming the previously held item specifically is far narrower than that. Measured: 0 new wrong.

Prior hysteresis (accept the nominal leader at 0.50 score / 0.05 margin when it names the confirmed
item) buys 2 reads. Not worth the extra rule.

### Gates: lowering `minimum_score` is still wrong — and now for a new reason

| `minimum_score` | c1v | c2 | skins | c1-confirm | c2-confirm | skins-confirm |
|---|---|---|---|---|---|---|
| 0.65 (shipped) | 1084/16/4 | 763/6/5 | 462/900/54 | 1037/67/0 | 747/27/0 | 466/950/0 |
| 0.60 | 1084/15/5 | 762/6/6 | 454/878/84 | 1037/67/0 | 747/27/0 | 466/950/0 |
| 0.55 | 1082/12/10 | 754/6/14 | 447/816/153 | 1039/65/0 | 747/27/0 | 466/904/**46** |

0.60 buys nothing after confirmation. At 0.55 the unread Thompson becomes a **confirmed**
`bow.hunting` (42) — a workshop bow skin that beats every other face on that slot frame after frame.
That is the failure `SlotHistory` cannot catch: it filters *inconsistent* errors, and a missing skin
face produces a consistent one. The Thompson's best face tops out at 0.601; the gate's headroom on it
is 0.05.

### Skin admission

| skins catalogue | c2-confirm | skins | skins-confirm |
|---|---|---|---|
| shipped allow-list | 747/27/0 | 462/900/54 | 466/950/0 |
| every category | 743/27/**4** | 467/834/115 | 474/919/**23** |
| every category, coverage >= 0.6 | 747/27/0 | 459/878/79 | 466/939/**11** (all `locker`) |
| every category, coverage >= 0.8 | 747/27/0 | — | 466/938/**12** (`locker`) |
| + Attire | 747/27/0 | 460/883/73 | 466/938/**12** (`burlap.headwrap`) |
| + Attire, coverage >= 0.6 | 747/27/0 | 461/904/51 | 466/950/0 |
| + Construction | 743/27/**4** | — | 478/936/**2** |
| + Attire + Construction, coverage >= 0.6 | 747/27/0 | 461/905/50 | 466/950/0 |

(c1-confirm is 1036-1037 / 0 wrong in every row.) The every-category row reproduces the docs' 474 /
23 exactly. Read across:

- `burlap.headwrap` (Attire) and the door wrongs on empty c2 slots (Construction, `door.hinged.*`
  over flat ground) are both small-coverage faces, and the coverage gate removes both.
- `locker` (Items) is the honest look-alike the docs describe — a full-canvas dark rectangle — and
  coverage cannot touch it. Items stays out.
- Construction's +12 in the unfiltered row is fragment faces landing on the right *item* by luck
  (the hinge-plates face above); it does not survive the coverage gate, and should not.

### Built-in skin icons (`Bundles/items/*.skin.png`)

279 Facepunch skin icons ship beside the item icons (`rifle.ak.ak47u_diver.skin.png`,
`door.hinged.metal.medieval.skin.png`, ...). `ItemCatalog::read` only looks for
`<shortname>.png` (`item_catalog.rs:128`), so they are never loaded. Loaded as faces of the
longest-prefix owner (278 resolve; `lr300.item.lr330.space` does not):

| | c1-confirm | c2-confirm | skins | skins-confirm |
|---|---|---|---|---|
| shipped skins | 1036/68/0 | 747/27/0 | 462/900/54 | 466/950/0 |
| + built-in skins | 1036/68/0 | 747/27/0 | 479/903/34 | 466/950/0 |

Neutral after confirmation. The per-frame gain is slot 3's `medieval.door.hinged.metal` wrongs
turning into `door.hinged.metal` reads through the medieval skin face — still a silhouette match on
a door that is not that skin, so it never confirms. Worth loading for coverage (these are skins the
game really draws), not for this corpus. Per-frame it adds `wallpaper.*` inventions on c2, which
confirmation removes.

### A higher gate for skin faces

`SKIN_MIN_SCORE = 0.70` (vanilla faces stay at 0.65): **no read lost anywhere** — 462 / 466 on
skins, 1036-1037 on c1-confirm, 747 on c2-confirm — because a real skin read scores 0.82-0.94. It
doubles the headroom on the consistent-wrong hazard above. Stress test, vanilla gate dropped to
0.55 with the skin gate at 0.70: skins-confirm goes from 46 wrong to **4** — every `bow.hunting`
skin read gone — and the 4 left are the vanilla `medieval.door.hinged.metal` face, which the
vanilla 0.65 gate already stops. The skin gate does exactly what it is for and nothing else.

### Combined

`GAP_KEEP` + coverage >= 0.6 + allow-list `+ Attire + Construction` + built-in skins:

| config | correct | abstained | WRONG |
|---|---|---|---|
| c1-confirm | 1082 | 22 | 0 |
| c2-confirm | 771 | 3 | 0 |
| skins-confirm | 471 | 945 | 0 |
| c2 (per frame) | 763 | 6 | 5 |
| skins (per frame) | 474 | 908 | 34 |

Zero confirmed wrong with 3.5x the skin faces (1,378 admitted against the shipped 397; 91 dropped
by the coverage gate); the gains are the gap fix's.

## Ranked proposals

| # | change | expected gain | WRONG risk | effort |
|---|---|---|---|---|
| 1 | `SlotHistory`: on a gap, demote confirmed to pending one agreement short instead of `slots.clear()` (`slot_history.rs:87-91`) | +46 c1-confirm, +24 c2-confirm, +5 skins-confirm (measured); in play, every slot readable on the first frame after alt-tab | very low; 0 measured | ~15 lines + a test |
| 2 | Skin-face score gate 0.70 (vanilla stays 0.65) in `IconMatcher::decide_at` | 0 reads; doubles headroom against consistent wrong reads from uncached skins | removes risk | ~10 lines |
| 3 | Admit a skin face only if its alpha mass is >= 0.6x the vanilla icon's; then widen `DEFAULT_CATEGORIES` with Attire and Construction | 0 on these corpora (their skins are not cached); unlocks the Attire and Construction skins (~700 of the cache, now admissible) for armour and door slots | 0 measured across all three; Items stays out for `locker` | small, in `SkinCatalog`/signature build |
| 4 | Find a source for owned-skin icons the cache lacks | up to +944 skins-confirm (all four unread items); the only change that moves the skins number materially | each new face must be re-scored; adds candidates | large; research first (below) |
| 5 | Load `Bundles/items/*.skin.png` as faces | 0 after confirm; real coverage of Facepunch skins | 0 measured | small, `ItemCatalog::load` |
| 6 | Corpus hygiene: drop or mark `belt-corpus` frames `105609-130` and `110258-932`; make the 105408 window the default for that corpus; add a scorer `--max-gap` so recorder holes do not read as focus loss | makes c1 "WRONG" mean something (63 -> 0 real per-frame errors are 4, all in the desktop frame) | none | trivial |
| — | Not recommended: lower `minimum_score` (0.55 -> 46 confirmed wrong); prior hysteresis (+2); a frame-chrome check for the desktop frame (the foreground gate already covers it); GDI+/sort/scoring rewrites — the remaining vanilla misses are terrain pushing a rank-1 truth under 0.65, not resampling, and nothing measured points at the downscale | | | |

### On #4 — where an owned skin's icon could come from

Unverified leads, in order of how directly they answer "what can this player's belt show":

1. **The player's Steam community inventory** for app 252490 — owned items with an icon URL and
   item-type tags, i.e. exactly the skins this player can put in a slot. Needs a public inventory
   and a network fetch; check that the icon is the transparent in-game icon, not a store render.
2. **The Steam inventory item definitions** (`IInventoryService/GetItemDefMeta` /
   `IGameInventory/GetItemDefArchive`): every approved skin with `icon_url` and an item short name
   tag. The public endpoint refuses without a key (probed: 403). If `<Rust>\temp` names are the MD5
   of those URLs, the same data labels the 524 temp icons for free.
3. **Game bundles.** `items.preload.bundle` holds `ItemDefinition`/`ItemMod*` behaviours and no
   `ItemSkin`; `skinnables.preload.bundle` holds models. Approved-skin icons would be in the large
   content/texture bundles — expensive to survey and not attempted.

Whatever the source, the matcher change is nil: faces are already the mechanism. The risk is the
one the docs measured — more faces, more fragment and look-alike wins — so re-run all three corpora
at `--confirm 2` with #2 and #3 in place first.

## Do first

**#1, the `SlotHistory` gap demotion.** It is the largest measured gain on every confirmed config,
zero wrong, a few lines in one function, and it fixes a live behaviour (silent belt after alt-tab),
not just the replay. #2 goes in the same change set as cheap insurance before anyone touches skins.
