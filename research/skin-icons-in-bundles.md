# Approved-skin icons in Rust's own asset bundles

Research, 2026-10-18. Follow-up to `skin-icon-source.md`. Read-only on the game install (shared-read
opens; the small serialized files were copied to the scratchpad, the big `.resS` payloads were read
in place by offset). Scripts: `scratchpad/bundles/*.py` (`blocks.py` header/directory parser,
`listtex.py`, `containers.py`, `extract.py`, `coverage.py`, `memtest.py`).

## Answer

**Yes.** Every approved workshop skin baked into the build ships its inventory icon in
`<Rust>\Bundles\textures\textures.{0..9}.bundle` at the container path
**`assets/skins/<workshopid>/icon.png`** (a `Texture2D` plus a `Sprite` over it), next to the skin's
material maps and a `manifest.txt` (JSON: `ItemType` display name, author, publish date, material
groups). The workshop id is the item def's `workshopdownload` (also `workshopid`), so
`itemdefid -> workshopdownload -> assets/skins/<id>/icon.png` needs no name matching.

- The two belt skins: `57903` Shadowborn Sheet Metal Door -> `assets/skins/3208884292/icon.png` in
  `textures.2.bundle`; `56702` Trust in Rust Time Travel Thompson -> `assets/skins/3137160380/icon.png`
  in `textures.1.bundle`. The other corpus doors `63508` / `63507` are in `textures.4.bundle`.
- **Same art as the CDN icon.** Bundle icon (256x256) vs `files.facepunch.com/.../<defid>_small.png`
  (200x200), both resized to 128 px, premultiplied RGB: median diff **0.0000**, mean 0.0044 / 0.0023,
  alpha mean 0.0006. Side by side indistinguishable (`scratchpad/bundles/out2/compare.png`).
- This corrects `skin-icon-source.md`: these skins *do* have `workshopdownload` in the current
  schema, and the game never needed `<Rust>\temp` or the workshop cache for them because the art is in
  the install.

## Format

- All bundles: `UnityFS` v8, Unity `6000.3.15x1`, flags `0x243` = LZ4HC-compressed block/directory
  info only, 16-byte padding (`0x200`, not the encryption bit). **Data blocks are uncompressed**
  (block flags 0) in every bundle, including the 7 GB ones. No encryption, no obfuscation.
- Each `textures\textures.N.bundle` = one SerializedFile (`CAB-<hash>`, 1.7-3.3 MB, format 22,
  **type trees embedded**, platform 19) + one `.resS` (0.7-1.4 GB) holding the pixel data.
- Icons: all 5,888 are **DXT5 (BC3), 256x256, 9 mips**, 87,408 bytes each in the `.resS`
  (`m_StreamData` offset/size). Mip 3 is a 32x32 BC3 block of 1 KB — the matcher's working size — but
  Unity's mip filter is not the agent's downscale; use mip 0 and the existing resize unless a
  corpus run shows mip 3 scores the same.
- `shared\*.bundle` hold world/model textures (and the Facepunch-made skins' icons, see below);
  `ui\store.bundle` is store UI; `skinnables.preload` is models; `items.preload` is item defs.

## Coverage (Steam item schema, `userdata\1402693347\...\30AC72...cachedmsg`, 6,660 defs, refreshed 2026-10-18)

| set | defs | icon in bundles |
|---|---|---|
| defs with `itemshortname` + `workshopdownload` (approved workshop skins) | 5,991 | **5,890** (98.3%) |
| `door.hinged.metal` | 295 | 290 |
| `smg.thompson` | 87 | 84 |
| `rifle.ak` | 334 | 332 |

- Bundled icon dirs: 5,888 (4 workshop ids are shared by two defs; 2 dirs have no def — retired).
- **The 101 not bundled are all the newest drops** (37 created 2026-09, 64 created 2026-10), i.e.
  accepted after this build's skins were baked. 13 of them are already in
  `steamapps\workshop\content\252490\` — the game downloads not-yet-baked skins from the workshop when
  it meets them, which is the path the agent already reads. The next game update should bake them.
- 610 defs have a short name but no `workshopdownload`: Facepunch-made / DLC / legacy skins (red
  t-shirt, tarp, ...). Their icons are prefab-local (`assets/prefabs/**/skins/**/*icon*.png`: 258
  textures in `shared\textures.1/2.bundle`) and mostly already covered by the shipped
  `Bundles/items/*.skin.png`. Not needed for this question.
- Every extracted icon has a transparent canvas (none near-empty, none fully opaque); random sample
  checked by eye (`scratchpad/bundles/out2/sheet.png`).
- Note: the largest `.cachedmsg` is not always the live one — `userdata\1387733362` holds a stale
  2025 copy. Pick by mtime among files whose body starts with the Rust appid.

## Cost (measured, Python + UnityPy, game running)

- **Index** (read 10 CABs ~27 MB straight from the bundles, parse `AssetBundle.m_Container`, read each
  icon's `m_StreamData`): **0.8 s, peak working set 160 MB** (Python+UnityPy baseline 89 MB). A
  native reader only needs the container table and ~6k small Texture2D records; it would be well
  under that.
- **Per icon:** one seek + 87 KB read + BC3 decode, ~1.5 ms warm. All 5,888 to PNG: 31 s (mostly PNG
  encoding), 515 MB read, 248 MB of PNG. No copy of the bundles is needed — offsets are absolute file
  offsets in an uncompressed archive, so a shared-read handle plus seek works while Rust runs.
- Compared with the CDN path: zero network, zero rate limits, zero privacy disclosure, and the
  per-item-set fetch trigger in `skin-icon-source.md` becomes unnecessary for baked skins — the agent
  can load an item's whole skin set on demand (or every skin of skinnable items) for disk-read cost.

## Fragility

- **Which `textures.N` holds a skin, and every offset, changes per build.** Build the index at
  start-up (or cache it keyed by each bundle's size+mtime) — never persist offsets across updates.
- **Container path convention** `assets/skins/<workshopid>/icon.png` is the stable key; it is the
  layout Facepunch's skin importer uses (same files as a workshop item: `icon.png`, `manifest.txt`,
  `_MainTex0.png`...). If it changed, the index would come back empty — detect and fall back.
- **Unity version bumps** can change the SerializedFile format (now 22) or Texture2D fields. Type trees
  are embedded, so a type-tree-driven reader survives field additions; a hard-coded field layout would
  not. Texture format could change (e.g. BC7) — handle BC3 and BC7, abstain otherwise.
- If Facepunch ever compresses the bundles (LZ4 data blocks), random access needs block-wise
  decompression (block table is already parsed for the header); encryption would end this route.
- Brand-new skins lag one game update; the workshop cache covers those the game has displayed.

## Recommendation

**Replace the download** for approved skins: index `Bundles\textures\textures.*.bundle` at start-up,
map `itemdefid -> workshopdownload -> icon`, decode BC3 on demand, keep feeding faces through the
existing `SkinCatalog` gates (coverage gate, allow-list, `Items` exclusion, 0.70). Keep the workshop
cache as the source for not-yet-baked and unapproved skins. Drop the `files.facepunch.com` fetch
unless a measured gap remains (the ~100 newest skins the game has not downloaded — rare in a belt,
since the game itself must download them to draw them, and then they are in the workshop cache).

Before shipping:

1. Re-run the `skin-icon-source.md` measurement with bundle icons instead of fetched ones (door +
   Thompson sets via `EXTRA_SKINS`) — expect the same 1326 / 0 since the art is pixel-equivalent, but
   BC3 artefacts are unmeasured in the matcher.
2. Implementation in Rust needs a small UnityFS/SerializedFile v22 reader (header, LZ4 block info,
   object table, type-tree walk for `AssetBundle` and `Texture2D`) plus a BC3 decoder crate. No
   mature Rust UnityPy equivalent was assessed here.
3. README: add `<Rust>\Bundles\textures\*.bundle` (read-only) to what the agent reads.
