# UI design — what to take from Rust+

The agent's window is currently a debug harness: raw `DataGrid`, hardcoded hex, no reusable styles.
This records what the Rust+ ecosystem actually looks like under the hood and which parts are worth
adopting, so the visual decisions are made once here rather than re-argued per window.

## What was surveyed

| Source | What it is | Verdict |
|---|---|---|
| [`Pronwan/rustplus-desktop`](https://github.com/Pronwan/rustplus-desktop) | Unofficial Rust+ desktop client. **WPF, .NET, same stack as us** | Primary source. Cloned and read |
| [`nerif-tafu/rustplusplus`](https://github.com/nerif-tafu/rustplusplus) | Web Rust+ interface | Rejected as a design source — `font-family: helvetica`, no tokens, no dark theme. Protocol reference only |
| [rust.facepunch.com/companion](https://rust.facepunch.com/companion) | The official app's marketing page | Information architecture only; no published palette or design system |
| [`liamcottle/rustplus.js`](https://github.com/liamcottle/rustplus.js) | The library everything else is built on | Protocol, not UI |

Facepunch publishes no design system for Rust+. The closest thing to a canonical brand colour is the
Rust logo red-orange **`#CE422B`** against near-black `#1E2020`, which is a game-branding palette, not
an app one. So the practical reference is `rustplus-desktop`, and the honest description of its look
is **WinUI 3 Fluent dark, not Rust-branded** — it merges `Wpf.Ui`'s dark theme and layers a custom
palette on top.

That is worth knowing before committing: copying Rust+ *the desktop app* means adopting Fluent, not
adopting Rust's visual identity.

## The palette it actually uses

Two ladders, because the app defines the same keys twice (see "what not to copy"). The
`SharedResources.xaml` set is the newer and better one:

```
AppBg        #0F1013     window background
Surface      #181B1F     card background
SurfaceAlt   #1F2229     input / nested surface
SurfaceHover #252A33     hover
CardBorder   #28FFFFFF   1px border — WHITE AT 16% ALPHA, not a grey
Accent       #60CDFF     Fluent light blue
AccentGreen  #4EB887
AccentRed    #F04747
TextPrimary  #F0F3F7
TextSubtle   #8C96A8
TextCaption  #5A6475
```

Radius is `8` for controls and cards, `14` for the tab content shell, `10` for buttons.

### The single most transferable detail

Borders are **`#28FFFFFF` — white at alpha**, not an opaque grey. Our `#232833` is an opaque
approximation of the same idea, which means it only looks right on exactly one background. An alpha
border composites correctly over `AppBg`, `Surface` and `SurfaceAlt` alike, so one brush serves the
whole surface ladder. That is why they can nest cards three deep without the borders drifting.

## Patterns worth adopting

**Three-step surface ladder.** Window → card → input, each one step lighter, each separated by an
alpha border. Depth comes from stacked surfaces, never from drop shadows.

**Everything lives in a card.** `Border` with `Surface` background, alpha border, radius 8, padding
12. Their `Card` style is four setters and it is applied everywhere.

**Semantic colour + chip, keyed by data.** `DeathStatsWindow` colours each row by *where* the death
happened — monument `#EF6C33`, base `#4ADE80`, open `#9D9D9D` — and renders the same three colours
as both a 9px dot in the table and a rounded "chip" (`CornerRadius=14`, background = the same colour
at 20% alpha, border = the colour at full). Driven by `DataTrigger` on the row's `Type`. This is
directly applicable to our combat events, which have exactly this shape.

**Presence as a corner badge.** Team avatars are circular (`EllipseGeometry` clip), with a 12px
online/offline dot pinned bottom-right and a gold star for the leader top-left. Reads at a glance
without a legend.

**Hairline scrollbars.** 4px wide, no arrow buttons (`RepeatButton` with `Opacity=0`), thumb only,
three-state colour. Dense data views stop looking like a spreadsheet.

**Tabs are an accent underline.** A 3px `Accent` bar at the bottom of the selected tab, plus
`FontWeight=SemiBold` and white text. No boxes, no borders.

**Overlays are content-sized transparent windows.** The minimap is `WindowStyle=None`,
`AllowsTransparency=True`, `Background=Transparent`, `Topmost=True`, `ShowInTaskbar=False`, and its
chrome (settings button) fades in on hover via a `Storyboard` — 0.1s in, 0.2s out. Ours will need
the same shape, and the asymmetric fade is worth copying: fast to appear, slow to leave.

**Filters stay visible when they match nothing.** Commented in their source as deliberate: if the
filter row hid itself on an empty result you could never widen the search.

## What not to copy

**`MainWindow.xaml` is 699,784 bytes.** One file, every view, templates inlined at eight levels of
indentation. Our 300-line ceiling exists for this. Views go in their own files under
`Views/`, as their *newer* code already does (`Views/MainWindow/Team/TeamTabContent.xaml`).

**The palette is defined twice and has drifted.** `App.xaml` and `SharedResources.xaml` both declare
`AppBg`, `Surface`, `Accent`, `PrimaryBrush`, `CardBorder` — with different values (`AppBg` is
`#1A1D21` in one and `#0F1013` in the other; `PrimaryBrush` is `#60CDFF` vs `#3B82F6`). Whichever
loads last wins, per key. Tokens belong in exactly one `ResourceDictionary`.

**Styles redefined inline per view.** `TeamTabContent.xaml` re-declares a `CheckBox` template that
`App.xaml` already styles. Every such copy is a place the design can drift.

**`Wpf.Ui` as a dependency** is a real decision, not a freebie: it brings `ui:Card`, `ui:SymbolIcon`
(Fluent icons) and a maintained dark theme, at the cost of a third-party package that owns our
control templates. Recommendation is to skip it — we need perhaps six styles, and hand-writing them
keeps the overlay's dependency surface at zero UI packages.

## Tokens — implemented

In `Theme/Tokens.xaml` (colours, radii, fonts) and `Theme/Controls.xaml` (styles), merged by
`App.xaml`. Tokens is the only file in the project that names a colour; Controls resolves every one
of them by `DynamicResource` and adds none of its own.

Our previous values were already close to Rust+'s ladder, so this was a normalisation rather than a
repaint. The one genuine change is alpha borders.

```
AppBg        #0F1114     (was #12141A)
Surface      #171A20
SurfaceAlt   #1E222A
SurfaceHover #262B35
CardBorder   #28FFFFFF   (was opaque #232833)
TextPrimary  #E6E8EC     unchanged
TextSubtle   #7C8598     unchanged
TextCaption  #5A6475
Radius       8
```

Semantic colours, kept deliberately separate from the accent:

```
Damage/death  #F04747
Kill/positive #4EB887
Warning       #D9A441   unchanged
Monument      #EF6C33
```

### Accent colour — settled

Rust's brand accent is `#CE422B`, a red-orange. **Our domain already spends the warm end of the
spectrum on meaning** — deaths, damage, kills, monuments. An accent that close to the damage colour
means selection state and "you took damage" compete for the same read.

Settled on **a cool accent** (`#60CDFF`, as Rust+ desktop does), reserving warm colours entirely for
combat semantics. The cost is that the app looks less overtly Rust-branded; the benefit is that the
one colour a player must never misread stays unambiguous. Revisiting this means moving the damage
colour, not just swapping `Accent`.

## Window shape

Rust+'s IA is Hub / Map / Team / Devices — four tabs, each a single concern. Ours maps to:

| Tab | Content | Status |
|---|---|---|
| **Session** | Identity, nickname warning | Built. Thin until there is more non-debug state to show |
| **Team** | Create/join, roster, per-member vitals and inventory | Built |
| **History** | Past sessions, death stats by location — the `DeathStatsWindow` pattern | Needs history endpoints |
| **Settings** | Install path, capture mode, sign-out | Not built |
| **Debug** | Loadout dump, HUD frame grab, tail status | Built. Not part of the four-tab IA — it is scaffolding |

The Team tab renders each container against its own staleness stamp and each slot as every candidate
the matcher could not separate. Both are load-bearing rather than cosmetic: see `item-matching.md` on
why a wrong item shown to a teammate is worse than a blank slot.

Sign-out belongs in Settings and is the missing half of the startup gate — see the revoked-token gap
in the backend's `open-questions.md`.

## Not yet decided

- Whether the in-game overlay is a separate transparent window (Rust+'s minimap approach) or a mode
  of the main window. The former is almost certainly right, but it interacts with capture and has not
  been designed.
- Icons. Rust+ desktop leans on `ui:SymbolIcon`; without `Wpf.Ui` we need Segoe MDL2 glyphs (already
  used for their minimap overlays) or vector `Path` data.
- Whether `Session` should be readable at a glance mid-game or is a post-match review surface. This
  changes the density substantially and should be settled before the feed is restyled.
