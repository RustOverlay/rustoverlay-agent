# Where an owned skin's icon can come from

Research, 2026-10-17. Follow-up to `matcher-accuracy.md` #4. Read-only on the repo; measurements use
a scratch copy of main at `0ed1d06` (coverage gate, shipped `*.skin.png`, 0.70 skin gate, 2/5
shortlist) with one scratch knob, `EXTRA_SKINS=<dir>` (files `<shortname>@<tag>.png` become skin faces
of that item, any category, then go through `retain_covering_skins` like workshop faces). Main
reproduces `score/baseline.txt` exactly.

## Answer

- **The player's Steam inventory is the wrong source.** Its cached copy (below) holds 10 playtime
  drops — boots, caps, beanies, two jackets, a furnace. No door, no Thompson, no AK skin. The belt
  in `belt-corpus-skins` is not drawn from things this account owns: the AK's face is workshop
  `2977171702`, which is in **no** item definition, so it is an unapproved workshop skin the server
  applied. Skinned items on a modded server come from the server, not from the inventory.
- **The Steam item-definition archive is the right catalogue, and it is already on disk.** Steam
  caches it per account at `<Steam>\userdata\<accountid>\inventorymsgcache\<digest>.cachedmsg` as
  plain JSON: 6,660 defs, each with `itemshortname` (the catalogue short name, no label mapping
  needed), `icon_url` / `icon_url_large`, `workshopdownload` and `name`. 298 `door.hinged.metal`
  skins, 87 `smg.thompson`, 345 `rifle.ak`.
- **Its icons are the slot art.** For the 176 approved skins whose workshop `icon.png` (what the
  game draws, and what the matcher already uses) and item-def `icon_url_large` are both on disk, the
  median premultiplied-RGB difference at 128 px is **0.006** and the alpha difference **0.0003**:
  same artwork, same transparent canvas, same framing. Outliers are a handful whose workshop icon
  has no alpha channel (`Cock AR`, `Cock Crossbow`) and three shop fronts. Checked by eye too:
  `scratchpad/inv/compare-ws-vs-itemdef.png` — Easter Bunny Sheet Metal Door, Valkyrie Thompson,
  Endgame Metal Door, workshop icon beside item-def icon on a slot-grey ground, indistinguishable.
- **The icon files themselves are not on disk** for the corpus skins. The item defs point at
  `files.facepunch.com/rust/icons/inventory/rust/<defid>_{small,large}.png`. **`<Rust>\temp` names
  are `md5(icon_url_large)` including the `?` query** — 249 of the 653 temp files resolve to a def
  this way (the rest are news/store banners). That labels the store cache for free, but it holds
  only what the main menu happened to show: 2 doors, 2 Thompsons, none of them the corpus ones.
- **The corpus doors and Thompson were measured later** — see "Measured with the fetched icons".
  At first: Getting their icons needs a network
  fetch. `steamcommunity.com` returned 429 to every request in this session (inventory, market
  search, market listing; ~40 min, spaced checks), and `files.facepunch.com` is outside the hosts
  this research was allowed. So the gain on slots 1-4 is unproven; the evidence that it would work
  is the art-equivalence above plus slot 0, where a face that is in the catalogue reads 235/236.

### Identifying the corpus skins (unresolved)

Candidates from local data only, no art confirmed:

| slot | look | candidates |
|---|---|---|
| 1 | red/black, figure silhouetted in a yellow disc | not `Endgame Metal Door` (compared: `inv/slot1-vs-endgame.png` — different figure, and it was downloaded 2026-08-21, after the corpus) |
| 2 | green ornate face/tech | — |
| 3 | grey panels | — |
| 4 | dark grey Thompson | not `Valkyrie` / `Salvation` (both in temp, neither wins) |

Another local Steam account (`userdata\408506643`, last cached 2025-02) owns `Alexby Metal Door`,
`Molten Visage Metal Door`, `Ghostly Flame Door` (in the workshop cache, already a face — not it) and
`Berne Always Watching Door`. Untested; the corpus account is not established either way.

## Measured

`belt-corpus-skins`, correct / WRONG (per-slot abstain, wrong):

| pool | `--skins` | `--skins --confirm 2` |
|---|---|---|
| main (baseline) | 476 / 27 | 471 / 0 |
| + 2 doors + 2 Thompsons from temp (item-def labelled) | 474 / 27 | 471 / 0 |
| + all 241 temp icons with a short name | 476 / 38 | 471 / **2** |

Per slot, `--skins`:

| slot | main | + doors/Thompsons | + all temp |
|---|---|---|---|
| 1 door | 236 / 0 | 236 / 0 | 231 / 5 |
| 2 door | 221 / 15 | 221 / 15 | 220 / 16 |
| 3 door | 219 / 4 | 221 / 4 | 214 / 9 |
| 4 Thompson | 236 / 0 | 236 / 0 | 236 / 0 |

With `--confirm 2` every slot 1-4 abstains in all three pools except the two new confirmed wrongs:
slot 1 `locker` 0.744 in two consecutive frames — a temp `locker` skin (Items), the full-canvas dark
rectangle `matcher-accuracy.md` already names. c1, c2, c2-flat and their confirm runs are unchanged
by either pool.

Read: labelling and loading item-def icons is safe exactly where the allow-list says (doors and
Thompsons: 0 new wrong), and the `Items` exclusion must apply to them as well. It only helps a slot
whose exact skin is in the pool — the wrong door skin does not read as a door.

## Proposal

The data is local; only the icon files need the network, and only for skins actually on screen.

1. **Item definitions — local, no fetch.** New `ro-core::steam::item_defs`: find the newest
   `*.cachedmsg` under `<Steam>\userdata\*\inventorymsgcache\` whose body starts with
   `[{ "appid":"252490"` (largest file; the digest name changes with each schema). Parse once at
   start-up into `defid → (shortname, icon_url_large, workshopdownload)`. Re-read when its mtime
   changes (Steam refreshes it when Rust starts). ~7 MB JSON, ~100 ms — off the capture thread.
2. **Free faces — local, no fetch.** Label `<Rust>\temp\*.png` by `md5(icon_url_large)` and add those
   as skin faces through the existing `SkinCatalog` path (coverage gate, category allow-list, 0.70
   gate). Zero network; small gain (store icons the player looked at).
3. **On-demand fetch — the only new network call.** When a belt slot abstains for N consecutive
   frames with a high departure (something is there), the shortlist leader's item is a skinnable
   item, and the slot's top candidates are all skins of that item, fetch the `_large.png` of every
   approved skin of that **item** (≤ 345 files, ~30-60 KB each) once, then re-score. Per item, not
   per frame. In practice the first time a player holds a given item type with an unknown skin.
   - Host: `files.facepunch.com` (Facepunch's CDN, the URL in Steam's own item defs). Plain GET, no
     cookies, no Steam ID, no key. Alternative with no Facepunch dependency does not exist without
     a Web API key.
   - Cache: `%LOCALAPPDATA%\RustOverlay\skin-icons\<defid>.png` plus `index.json`
     (`defid, shortname, url, etag, fetched_at`). Keyed by defid; the URL's `?<n>` cache-buster
     changes when Facepunch replaces an icon, so a changed URL means re-fetch. Never delete on
     failure. Cap the directory (e.g. 200 MB, LRU).
   - Cadence: on demand as above; at most one item batch in flight, ≤ 4 requests/s, and a per-item
     back-off (1 h after a failure, 24 h after a batch that did not change the slot's reading).
   - Failure handling: any HTTP error, timeout or bad PNG leaves the slot abstaining exactly as
     today — the fetch can add faces, never remove them. Offline is the normal case, not an error.
   - These faces go through the same gates as workshop faces; keep `Items` out (the `locker` result
     above).
4. **Unapproved workshop skins** (slot 0's kind) are not in the item defs at all. They already
   arrive in the workshop cache when the game downloads them; nothing to add.
5. **Verify before shipping**: fetch the door and Thompson sets once by hand, run
   `score/run.sh` with them, confirm slots 1-4 read and WRONG does not rise; then identify the three
   doors and the Thompson by their winning defid.

### Privacy / README

The README says "Everything it reads is listed below" and lists only `output_log.txt`,
`client.cfg` and the registry. The agent already reads more than that table shows
(`Bundles/items`, the workshop cache, screen capture) — the table needs those rows regardless.
This proposal adds:

- **Read:** `<Steam>\userdata\<id>\inventorymsgcache\` (Steam's item schema; the same directory
  holds the account's inventory list — read only the schema file, never the inventory one, and say
  so) and `<Rust>\temp\`.
- **Network, new:** GET `files.facepunch.com/rust/icons/inventory/rust/*.png`. Leaks to Facepunch's
  CDN the client IP and *which item types the player is holding skinned items of* (not which skin,
  since the whole per-item set is fetched; not the Steam ID). This is the first outbound connection
  other than the relay and must be called out in README under its own heading ("What it fetches"),
  with a setting to turn it off (default on or off is the user's call; off costs only skinned-slot
  reads).
- Do **not** use the Steam community inventory endpoint: it identifies the player by Steam ID,
  needs a public inventory, is aggressively rate-limited (429 for the whole session here), and does
  not describe what a modded server puts in the belt anyway.

## Measured with the fetched icons

One-off fetch (authorised): `icon_url_large` of every approved `door.hinged.metal` (298) and
`smg.thompson` (87) skin from the item-definition cache — 385 anonymous GETs to
`files.facepunch.com` at <= 4/s, 385 OK, no 429, no failure. 512x512 palette PNGs with transparency,
**32 MB total (~83 KB each)**. Loaded through `EXTRA_SKINS` into the main pipeline (coverage gate,
0.70 skin gate, 2/5 shortlist); all 385 passed the coverage gate.

| config | main (baseline) | + fetched icons |
|---|---|---|
| c1 | 1084 / 0 | 1084 / 0 |
| c1-confirm | 1082 / 0 | 1082 / 0 |
| c2 | 765 / 4 | 765 / 4 |
| c2-flat | 757 / 4 | 757 / 4 |
| skins | 476 / 27 | **1316 / 9** |
| skins-confirm | 471 / 0 | **1323 / 0** |

(correct / WRONG; c1/c2 bit-identical to baseline.)

Per slot, correct reads out of 236:

| slot | truth | winning skin (defid) | per frame: main -> fetched | `--confirm 2`: main -> fetched |
|---|---|---|---|---|
| 0 | rifle.ak | (unchanged, workshop `2977171702`) | 235 -> 235 | 235 -> 235 |
| 1 | door | Shadowborn Sheet Metal Door (57903) | 0 -> **234** | 0 -> **235** |
| 2 | door | Jungle Guardian Door (63508) | 0 -> **234** | 0 -> **235** |
| 3 | door | Aztec Jungle Door (63507) | 13 -> **150** | 0 -> **147** |
| 4 | smg.thompson | Trust in Rust Time Travel Thompson (56702) | 0 -> **235** | 0 -> **235** |

- **New WRONG: none.** Per frame the 9 left are the 8 pre-existing empty-slot-5 inventions (`shovel`,
  `cannonball`, `sail`, ...) plus one slot-3 `sign.wooden.*` frame; the 15 slot-2 `50cal.mounted`
  and 4 slot-3 wrongs of main are gone. Confirmed: 0.
- **Right skin, checked by eye** (`scratchpad/inv/slots-vs-fetched.png`, slot crop above the winning
  icon): slot 1 the red door with the silhouette in a yellow disc, slot 2 the green glowing mask,
  slot 3 the carved stone maze panels, slot 4 the silver-and-wood Thompson — each the same artwork,
  and the diag (old scratch build, shortlist off) has the same face at rank 1 in 234-235 of 236
  frames for slots 1, 2, 4 with a median margin of 0.20-0.34.
- **Slot 3 is the weak one:** median score 0.825, margin 0.146; the next faces are other carved
  doors (Inquisitor Metal Door 57002, Hypnotic Steel Door 42603), and ~89 frames abstain on margin.
  Its slot sits over the red-brown background and the door reads darker than the icon. Still 0
  wrong.
- None of the four is in the workshop cache (`workshopdownload` absent) or in either local Steam
  inventory; three are marketable, the Thompson is not. Consistent with the server applying them.
  So these approved skins are drawn from data the game never left on disk where we look — the item
  definitions plus Facepunch's icon CDN are the only source found for them.

### Verdict on the on-demand fetch

**Worth building.** It is the only change that moves the skins corpus: 33% -> 93% confirmed, with
zero wrong in all six configs. Adjustments to the proposal above from the measurement:

- Budget ~83 KB per icon, not 30-60: one full item set is up to ~29 MB (`rifle.ak`, 345 skins).
  Fetch `_small.png` instead if it scores the same — the matcher works at 32x32 — but that is
  unmeasured; measure it before choosing.
- Fetching per item type is the right granularity: the winning face was found among 298 doors
  with no hint of which one, and look-alikes did not cause wrong reads.
- Only skinnable items the agent actually sees unread trigger a fetch, so the typical cost is a few
  item sets per player, once.

## Choosing the trigger

Main's matcher (`--skins`, **no** fetched icons — the state the trigger runs in), per slot-frame
dumps from a scratch `trigger` example: decision, departure, and the distinct-item ranking both
shortlisted (what the agent computes) and full (shortlist off). "Abstaining" is the per-frame
`identify` result, before `SlotHistory`. Corpora: `belt-corpus-skins`, c1 (`--from 105408 --exclude
105609-130,110258-932`), `belt-corpus-2`. "Skinnable" = has approved skins in the item-def cache
(256 short names). Analysis scripts: `scratchpad/inv/trigger_ana.py`, `trigger_ana2.py`,
`trigger_seq.py`.

### 1. Where the true item ranks on abstaining, non-empty slots

Departure >= `minimum_departure`, truth not `-`, truth skinnable:

| corpus | n | top 1 | top 3 | top 5 | top 10 | top 20 | top 40 | > 40 |
|---|---|---|---|---|---|---|---|---|
| skins (full) | 894 | 202 | 368 | 479 | 612 | 756 | 795 | 99 |
| skins (shortlisted) | 894 | 202 | 362 | 474 | 606 | 762 | 798 | 96 |
| c1 (full) | 10 | 6 | 6 | 6 | 7 | 10 | 10 | 0 |
| c2 (full) | 5 | 4 | 4 | 4 | 4 | 5 | 5 | 0 |

Per skins slot (full ranking): slot 1 door median rank 4 (233/235 in top 10); slot 2 door median 13
(94/220 in top 10); slot 3 door median 10 (104/204); slot 4 Thompson median 1 (181/235). The
shortlist costs almost nothing here. Per frame the true item is often not near the top — but it
only has to be there in *some* frames, since doors are fetched once for all three door slots.

But the trigger's real enemy is the other class: **empty slots that pass the departure gate**.
Abstaining with departure >= min: skins 127 empty vs 894 item slot-frames, c1 110 vs 10, c2 222 vs
5. Departure and top-1 score overlap too much to separate them (empty: departure median 0.056-0.063,
top-1 median 0.49-0.55; item: 0.08-0.14, 0.60). What does separate them is **stability**: an item
keeps the same candidates frame after frame, landscape behind an empty slot does not.

### 2. Fetch cost of candidate triggers

**As specified** — fetch every skinnable item in the top K of a slot after M consecutive
abstaining non-empty frames. Item types fetched per corpus (skins / c1 / c2), with skins and MB for
the skins corpus (large @ 83 KB, small @ 19 KB):

| K | M | skins corpus | c1 | c2 | true item fetched, skins slots 1-4 | types triggered by empty slots (skins/c1/c2) |
|---|---|---|---|---|---|---|
| 3 | 2 | 68 types, 4000 skins, 324 / 78 MB | 54 | 86 | Y Y Y Y | 36 / 52 / 84 |
| 3 | 3 | 61, 3891, 315 / 76 | 41 | 75 | Y Y Y Y | 24 / 40 / 74 |
| 5 | 2 | 87, 4382, 355 / 86 | 68 | 105 | Y Y Y Y | 54 / 66 / 103 |
| 5 | 3 | 81, 4286, 347 / 84 | 52 | 96 | Y Y Y Y | 43 / 50 / 95 |
| 10 | 2 | 110, 4843, 393 / 95 | 102 | 144 | Y Y Y Y | 77 / 100 / 142 |
| 10 | 3 | 103, 4745, 385 / 93 | 88 | 137 | Y Y Y Y | 63 / 86 / 135 |
| 20 | 2 | 135, 5129, 416 / 100 | 144 | 180 | Y Y Y Y | 101 / 140 / 178 |
| 20 | 3 | 126, 5057, 410 / 99 | 128 | 168 | Y Y Y Y | 87 / 126 / 166 |

**Every one of these is unacceptable.** It finds the right items but fetches 40-180 item types per
session, almost all of them because of empty slots — c2, where no slot holds a skin, fetches 75-180
types. Already-read slots trigger nothing by construction (a read is not an abstention), and wrong
reads don't either, so the whole cost is empty slots plus noise from the unread items' neighbours.

**Recommended rule — stable and one at a time:** after M consecutive abstaining non-empty frames,
take the skinnable items that are in the top K of **every** one of those M frames, fetch the
**single** best-ranked one not yet fetched, re-score, and re-arm (the M-frame count starts over).

| K | M | skins corpus | c1 | c2 | order fetched (skins) | types from empty slots |
|---|---|---|---|---|---|---|
| 5 | 2 | 10 types, 1440 skins (27 / 117 MB) | 4 | 5 | — | 8 / 3 / 5 |
| 5 | 3 | 4 types, 915 skins (17 / 74 MB) | 0 | 0 | door, rifle.ak, Thompson, pants | 1 / 0 / 0 |
| **5** | **5** | **3 types, 730 skins (14 / 59 MB)** | **0** | **0** | **door (1st), rifle.ak, Thompson (3rd)** | **0 / 0 / 0** |
| 10 | 3 | 4 types, 915 skins | 0 | 1 (`roadsign.kilt`) | door, rifle.ak, Thompson, pants | 1 / 0 / 1 |
| 10 | 5 | 3 types, 730 skins | 0 | 0 | door, rifle.ak, Thompson | 0 / 0 / 0 |
| 20 | 3 | 9 types, 1505 skins | 1 | 4 | — | 7 / 1 / 4 |

(Simulation assumes a slot stops abstaining once its true item's skins are fetched — what the
fetched-icon measurement above showed. A top-1-score floor of 0.50-0.55 was also tried; it only
hurts — the Thompson's top-1 sits around 0.50, so a floor delays or prevents its fetch.)

`rifle.ak` is the one wasted set: slot 0's AK reads, but on the frames where a door slot's stable
top 5 contains it. 345 skins, unavoidable without knowing the answer; still 0 fetches on c1/c2.

### 3. `_small.png` against `_large.png`

`icon_url` (`<defid>_small.png`) for the same 385 skins: 385 GETs, no failure, **200x200 palette
PNGs, 7 MB total (~19 KB each)** against 32 MB.

| config | large | small |
|---|---|---|
| skins | 1316 / 9 | 1317 / 9 |
| skins-confirm | 1323 / 0 | **1326 / 0** |
| c1, c1-confirm, c2, c2-flat | = baseline | = baseline |

Per slot `--confirm 2` with small: 235 / 235 / 235 / 150 / 235 / 236 (slots 0-5; slot 3 abstains 86).
Small is as good — the matcher works at 32x32 — at under a quarter of the bytes.

### Recommendation

- **Trigger: K = 5, M = 5, intersection, one item type per trigger.** The two rules asked about
  (union over top K, M = 2/3) cost 40-180 item types; this one costs 3 on the skins corpus, 0 on c1
  and c2, and fetches the door set first and the Thompson third. M = 3 is the fallback if 5 is too
  slow (4 types, one stray `pants` set from an empty slot, still 0 on c1/c2).
- **M is in recorder frames (~1.3 s apart), so M = 5 is ~6 s.** If the agent reads the belt faster,
  express it as time (>= 5 reads spanning >= 5 s), not raw frames, or empty slots will look stable.
- **Icon size: `_small.png`.** Same reads (1326 vs 1323 confirmed), 0 wrong, ~19 KB per icon: the
  whole recommended-rule cost on the skins corpus is ~14 MB; the largest single set (`rifle.ak`,
  345) is ~6.5 MB; every approved skin in the game is ~125 MB.
