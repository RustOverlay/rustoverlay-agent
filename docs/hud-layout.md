# HUD layout — the vitals cluster

Measurements for the health / hydration / calories cluster the capture pipeline reads. Companion to
`log-format.md`; the capture code points here rather than repeating rationale inline.

Measured from native frames at 1440x1080 and 1920x1080, at `graphics.uiscale` 1.0 and 0.5.

## What the cluster contains

Bottom-right, three rows, each an icon then a value:

| Row | Icon | Max | Notes |
|---|---|---|---|
| Health | cross | 100 | |
| Hydration | cup | 250 | |
| Calories | crossed utensils | 500 | |

Status banners (`STARVING`, `TOO COLD 5°c`, …) stack **upward** above the cluster. They do not move
it.

## Geometry

The cluster is anchored to the bottom-right corner of the client area. Reference values below are at
**1920x1080, `graphics.uiscale 1.0`**:

```
health bar     bottom-113   left edge right-274   height 30
hydration bar  bottom- 71   left edge right-274   height 30
calories row   bottom- 29   (derived: row pitch is 42)
```

Digits are drawn at a fixed offset from the bar's **left** edge, which does not move. The bar grows
rightward from that edge.

### Scaling

```
offset = referenceOffset × (frameWidth / 1920) × uiscale
```

Both factors verified independently:

| Frame | health left | health bottom | height | pitch |
|---|---|---|---|---|
| 1920x1080 @ 1.0 (reference) | right-274 | bottom-113 | 30 | 42 |
| 1920x1080 @ 0.5 | right-136 | bottom-56 | 15 | 21 |
| 1440x1080 @ 1.0 | right-205 | bottom-84 | 23 | 31 |

`uiscale` 0.5 halves every value to within a pixel. The 1440-wide frame lands on ×0.75, which is
`1440/1920` — not `1080/1080`.

> **Gotcha:** the HUD scales with frame **width**, not height. Both measured frames are 1080 tall,
> so a height-driven model would predict identical offsets; every offset moved by 1.333 instead.
> A 4:3 player therefore gets a visibly smaller HUD than a 16:9 player at the same pixel height.

> **Unverified:** only two aspect ratios have been sampled, both at 1080 height, so "scales with
> width" is not yet distinguishable from a Unity `CanvasScaler` match factor blending width and
> height. A frame at a different *height* (e.g. 2560x1440) would settle it. Until then the locator
> must treat the computed position as a search hint and confirm by finding the bar edge, not trust
> the arithmetic outright.

Inventory state does not affect any of this — confirmed across three open/closed pairs at both
resolutions and both uiscales.

## Reading the values: bar width, not the digits

Bar width is proportional to `value / max` against a track that scales with the frame:

```
trackWidth = 247 × scale          (scale = frameWidth / 1920 × uiscale)
value      = barWidth / trackWidth × max
```

| Row | max |
|---|---|
| Health | 100 |
| Hydration | 250 |
| Calories | 500 |

Verified against frames where the digits give ground truth:

| Frame | health | hydration | calories |
|---|---|---|---|
| 1440x1080 @1.0 | 39.0 vs **39** | 62.2 vs **62** | 51.4 vs **51** |
| 1600x900 @1.0 | 35.0 vs **35** | 60.8 vs **61** | 48.7 vs **48** |

Health resolves at 2.47px per point at reference scale, so it is nowhere near ambiguous. Calories is
the coarsest at 0.49px per point, giving roughly ±2.

`VitalsReader` derives `scale` from the **measured** left edge rather than from the frame size, so an
error in `uiscale` or in the scaling model above is absorbed rather than propagated.

### Why not OCR the digits

At low values the digits leave their coloured backing entirely — they sit at a fixed offset while the
bar grows from the same edge, so a short bar means white-on-terrain instead of white-on-green.
Observed directly: at 51/500 calories the `5` is on orange and the `1` is on the row background. A
single glyph can span two backgrounds. That is the worst possible input for character recognition,
and it is the case that matters most — a player at 5 HP is exactly when the read must not degrade.

The bar edge has none of these problems. Digit matching is planned only as a cross-check to populate
`Vitals.confidence`; it is not on the critical path.

> **Gotcha:** the digits are drawn *on top of* the bar, so a scanline through them breaks the colour
> into pieces. Measuring the longest unbroken run finds the piece after the digits and reports the
> left edge ~12% too far right, which then corrupts the scale derived from it. Measure the column
> extent over the row's full height instead — the digits are vertically centred and leave colour
> above and below.

## Slots

Measured at 1440x1080 (scale 0.75) by fitting a comb to the edge profile. Colour thresholding does
not work here — the slot panels are translucent over a blurred backdrop, so fill (82,79,69), gap
(85,79,70) and backdrop (72,70,68) are indistinguishable. The grid is defined by its edges.

| Container | slots | pitch @1440 | pitch @1920 ref |
|---|---|---|---|
| Belt | 6 | 72 | 96 |
| Main | 6 x 4 | 72 | 96 |
| Wear | 7 | **61** | **81** |

Slot size is 68 at 1440, i.e. 91 at reference. The belt row occupies y 990–1057, so its bottom sits
29 reference pixels above the frame bottom.

> **Gotcha:** wear slots are a different pitch from the other two containers. Assuming one slot size
> everywhere puts every armour slot progressively further out of position along the row.

### The backpack

One slot, directly above the first wear slot: same left edge, same size, one wear pitch higher. It
needs no constants of its own. Verified at 1440x1080 and 1920x1080 against the same capture, where
the `largebackpack` reads at 0.93-0.97 with a 0.44 margin — comfortably the most confident item in
the corpus.

> **Gotcha:** the number on a backpack is a *fill* count, `0/28`, not a stack count. `CountReader`
> would read the `0` and report a stack of zero. Backpack capacity is a separate reader that does
> not exist yet; until it does, do not point the count reader at this slot.

### Anchoring, confirmed

Everything is anchored to the frame's **bottom-left** and scaled by **width alone**. Height does not
enter the model. Established by capturing the same scene at 1600x900 and 1600x1024 — identical width,
124px of height between them:

| Frame | belt slot 0 x | pitch | belt top (from bottom) | belt bottom |
|---|---|---|---|---|
| 1600x900 | 549 | 80 | 97 | 22 |
| 1600x1024 | 549 | 80 | 97 | 22 |
| 1440x1080 | 495 | 72 | 88 | 20 |

The 1440 frame scales onto the others exactly: 88 × (0.833/0.75) = 97.8, 20 × 1.111 = 22.2.

> Left-anchored and centre-anchored are the *same model* here, not competing ones — screen centre
> scales by the same width ratio, so both predict 495 × (1600/1440) = 550. An apparent 11px offset
> from screen centre is not evidence of anything.

**Bottom-anchored means every container, not just the belt.** The main grid and the wear row were
originally expressed as offsets from the top, which agrees with the bottom-anchored form at 1440x1080
and disagrees at every other width — because the scale changes while the height does not. At
1920x1080 the main grid missed every slot and the wear row was computed past the bottom edge of the
frame. All three containers now derive from `frameHeight - 1 - offset × scale`.

> **Gotcha:** the symptom was one container abstaining on every slot while the belt in the same frame
> read perfectly. That pattern means geometry, not matching — it has now happened twice.

## Held item and ammo

The belt is readable in world view, and it carries both:

- **The held item is the blue-tinted slot.** Measured at 1440x1080: the selected slot averages
  (70,103,134) against (116,113,113) for the same slot unselected — a blue-over-red bias of 64.
- **Ammo is drawn in the slot itself**, so there is no separate readout to locate. Observed going
  24 -> 30 across a reload.
- **Stack counts are prefixed, ammo counts are not**: `x2` and `x3` against `0`, `24`, `30`. That
  prefix is what tells the two numbers apart. Both are right-aligned in the slot's lower edge.
- Observed widths so far: ammo counts `0`-`10`, stacks `x5`-`x104`. A reader must handle four
  characters including the prefix, and must not assume a count is present at all.

The text sits in a band from `slot.Bottom - 0.26 * size` to `slot.Bottom - 0.06 * size`, and from
`slot.X + 0.08 * size` to `slot.Right - 0.04 * size`. It is right aligned, every glyph shares a
baseline, and digits stand `0.176 * size` tall — roughly 12px at 1440x1080.

`CountReader` reads it, in Core; `FontGlyphAtlas` supplies the rasterised glyphs, in Capture. It
reads all 24 labelled counts and stays silent on all 74 empty slots. See docs/item-matching.md for
why it slides the font along the strip instead of segmenting it.

> **Gotcha:** the count text is *not* bright. It measures 140-185 luminance against backgrounds that
> reach 180 in daylight over pale rock, so an absolute threshold does not separate it — which is the
> first reason the reader never thresholds at all.

> **Gotcha:** the glyph height cannot be measured from the band. Deriving it from a threshold mask
> gave 11, 12, 13 and 14 pixels for the *same* text across six slots, because whatever background
> blob shares the band is measured along with the digits. The reader searches +/-1px around the
> nominal instead and lets the rightmost glyph choose.
- Condition is a thin vertical bar on the slot's left edge.

> **Gotcha:** in world view the slot panels are transparent, so a player looking at sky or water puts
> blue behind the entire belt. `HeldItemReader` therefore requires the winner to be both blue in
> absolute terms and clearly bluer than its neighbours, rather than testing a threshold per slot.

## Capture constraint

`graphics.resolution` in `client.cfg` is an **index** into Unity's supported-resolution list, not a
resolution — an install running 1440x1080 stores `"4"`. Pixel dimensions must come from the captured
frame or `GetClientRect` on the Rust window. `graphics.uiscale` is a real scalar and is usable.

## Ground truth from gameui.prefab

Rust is an IL2CPP build, so there is no `Assembly-CSharp.dll` to decompile — only
`GameAssembly.dll` plus `il2cpp_data/Metadata/global-metadata.dat`. Two things follow, both
verified 2026-08-09 against Unity 6000.3.15:

- **Il2CppDumper cannot read it.** The metadata is version 39; Il2CppDumper hard-errors. Use
  Il2CppInspectorRedux.
- **There are no layout constants in the binary at all.** `Assembly-CSharp` contains zero
  `const float`/`double`/`string` fields. Every colour, alpha, inset and pitch is a *serialized
  instance field*, so it lives in the asset bundles, not the code. `ItemIcon` has no
  `RectTransform` field either — it holds `iconImage`, `backgroundImage`, `amountText` and
  `conditionFill` and inherits geometry from the prefab hierarchy.

The slot prefab is `assets/bundled/prefabs/ui/gameui.prefab` inside `Bundles/shared/assetscenes.bundle`
(not `content.bundle`, which is world content). Belt and main-inventory slots are byte-identical.

| | serialized value |
|---|---|
| `GridLayoutGroup` | `m_CellSize (64,64)`, `m_Spacing (0,0)`, 6 columns |
| `occupiedContents` | anchors 0/1, `sizeDelta (-4,-4)` → 60x60 |
| icon `image` | anchors 0/1, `sizeDelta (-8,-10)`, `anchoredPosition (0,3)`, `preserveAspect: 1` |
| `condition` | anchors (0,0)/(0,1), `sizeDelta (4,0)`, pivot (0,0.5) — 4 units, flush left |
| `amount` | anchors (1,0), `anchoredPosition (-3,1)`, legacy `Text`, size 14, Bold, LowerRight |
| `CanvasScaler` | `ScaleWithScreenSize`, reference **1280x720**, **`Expand`**, `referencePixelsPerUnit 100` |
| `ItemIcon.selectedBackgroundColor` | `(0.1216, 0.4196, 0.6275, 0.7843)` — composites to ~(30,94,141) |
| `ItemIcon.conditionFillColor` | RGB 115,141,69; refrigerated RGB 31,132,160 |

So a square icon draws 50x50 in a 64-unit cell, two units below the slot top.

### What this changed, and what it did not

**Pitch is confirmed exactly.** 64 x (1920/1280) = 96, matching the fitted constant.

**The scale formula was wrong for ultrawide.** `Expand` mode means
`min(W/1280, H/720)` — the *smaller* axis ratio. Scaling by width alone is correct at 16:9 and
narrower, which is why 1600x900 and 1600x1024 measured identical, but at 2560x1080 it gives a 128px
pitch where the game draws 96. Now `min(W/1920, H/1080)`, which is the same quantity at this file's
reference.

### The icon rect: what the sweep settled

Substituting the prefab's rect naively — 50/60 of slot size, 2/60 below its top, anchors untouched —
takes belt-corpus from **1062 correct / 35 abstained** to **1048 / 50**. That looks like the prefab
being wrong. It is not; it is an anchor artefact, and the fair test says so.

`BeltLeftAt1920`, `BeltBottomFromBottom` and `SlotSize` were fitted *together with* the icon
fractions, so each absorbs some of the others' residual. Swapping one member for its true value
while the rest stay fitted breaks the compensation. Re-tuning anchors per icon model, exhaustively
over S 86..96 x L 654..666 x B 21..33:

| icon model | at shipped anchors | anchors re-tuned | resulting icon rect |
|---|---|---|---|
| fitted 0.80 / 0.06 | 0.8581 | 0.8779 at S=92 L=660 B=26 | (502,995,55) |
| prefab 0.833 / 0.033 | 0.7961 | **0.8790** at S=88 L=662 B=28 | (502,995,55) |

*(mean similarity of the truth item, 920 non-empty labelled slots, 1440x1080)*

**The two families are not competing geometries — they are two parameterisations of the same
pixels.** A free search over 16,170 rects confirms it: all 25 top scorers share the icon rect
**(502, 995, 55)**, differing only in the slot ring around it. The 0.062 gap at shared anchors
collapses to 0.0011 once each is given its own anchors.

So the drawn icon is **55px at 0.75 scale = 73.3px at the 1920 reference**. The prefab's Image rect
predicts 75px. The optimum is ~2% smaller, and edge by edge it is inset from the prefab's rect on
three sides while the right edge agrees exactly — consistent with the 512px artwork not reaching its
canvas edges, though one resolution and four items cannot establish that.

**What shipped:** `IconScale` 0.806 and `IconTop` 0.044, taken directly rather than as
`Size - 2 * inset` (which can only change size by even amounts and cannot reach 55 from 68 at all).
Anchors are unchanged. Belt-corpus **1062 → 1071 correct, 35 → 28 abstained, 7 → 5 wrong**.

> **Caveats, because this was tuned and scored on the same corpus.** Configs that move the anchors
> reach 1073 correct but at 6 wrong; the shipped choice keeps anchors fixed and takes the lowest
> wrong count instead, since zero-wrong is the property being protected. Only **14 of 1104 slots**
> differ from the old constants. The corpus is one continuous session, one resolution, no UI-scale
> variation, 6 slots, 4 distinct items — two of them the same AK. At 0.75 scale `SlotSize` and
> `IconScale` collapse onto the same few integers, so **nothing here constrains the
> fraction-of-slot-size form that has to hold at other resolutions.** Re-run this against a corpus
> recorded at 1920x1080 and at a non-1.0 uiscale before trusting the third decimal place.
>
> The slot ring is worth ~7 slots on its own (all eight swept configs sharing one icon rect still
> spanned 1066-1073) because it feeds `BackgroundOf`, `SceneFrom` and `Departure`. It was not tuned
> here and deserves its own study.
