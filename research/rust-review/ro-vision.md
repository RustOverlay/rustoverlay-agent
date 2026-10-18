# ro-vision review (2026-10-17)

Scope: `crates/ro-vision/src/**` against `src/RustOverlay.Agent.Core/{Hud,Items,Hosting/LoadoutState.cs}`.
Read-only; no cargo runs. Each finding was verified by reading both sides.

## Findings

### 1. Medium: `items/loadout_counts.rs:106`, `SystemTime` overflow panic on wire data
`UNIX_EPOCH + Duration::from_millis(m.read_at_ms as u64)` panics when the sum can't be represented.
- **Scenario:** a teammate's `Loadout.main.read_at_ms` arrives from the relay, so it is not trusted. A bad value (µs or ns sent as ms, or a corrupt or hostile message) panics inside `meds()`/`walls()`/`spare_ammo()` on the consumer/UI path.
- **C#:** `DateTimeOffset.FromUnixTimeMilliseconds` also throws, but only above year 9999.
- **Fix:** `UNIX_EPOCH.checked_add(Duration::from_millis(..))`, and treat `None` as "no timestamp" (not stale).

### 2. Low: `items/loadout_counts.rs:102,124`, i32 overflow on wire counts
`sum(..) + from_main` and `.map(quantity).sum()` add `i32` counts taken from the wire.
- **Debug build:** two slots at `i32::MAX` panic.
- **Release build:** wraps to a negative total, which is the same as C# unchecked `int`, and still a plausible-looking wrong number.
- **Fix:** `fold(0, i32::saturating_add)` / `saturating_add`, or clamp `quantity()` to the item's stack size.

### 3. Low: `items/frame_slot_sampler.rs:58-60` (plus `hud/frame.rs:77`), unclamped pixel reads
`departure_from` reads `frame.pixel(icon.x + column, icon.y + row)` with no bounds check. `Edges::of` (line 164) checks only `icon.size` and that the icon is inset from the slot; it never checks the frame.
- `ArrayFrameBuffer::pixel` computes the index in `i32` and casts it to `usize`. A negative or past-the-end index panics on the slice.
- `slot_crop.rs` `crop` reads `pixel(0,0)` even on a 0x0 frame (it clamps to `max(1, ..)`).
- **Scenario:** a slot whose icon area extends past the frame panics inside `InventoryReader`'s rayon `par_iter`, and the panic propagates to the capture loop. Triggers:
  - a tiny or mid-resize capture,
  - a 0x0 frame from a minimised window,
  - a large `graphics.uiscale` that pushes the main grid's top row above y=0 (needs scale > ~2.1).
- **C#:** has the same reads and throws `IndexOutOfRangeException`. The behaviour matches, but a Rust panic is harder to contain.
- Every other HUD reader clamps: `slot_hash`, `count_reader`, `held_item_reader`, `inventory_panel::mean`, `vitals_reader::measure`, and `frame_slot_sampler` lines 110/198.
- **Fix:**
  - `departure_from` returns `f64::INFINITY` unless `icon` lies entirely inside `0..width` x `0..height`.
  - `read_with_panel` returns an empty reading for `width <= 0 || height <= 0`.
  - Optionally, `debug_assert!` bounds in `ArrayFrameBuffer::pixel`.

## Checked and matching C# (no action)

- **`hosting/loadout_state.rs`:**
  - `last_open_inventory` is retained while the panel is shut, and never reset on close.
  - `hidden_since` is first-shut-frame, `get_or_insert`, the same as `??=`.
  - The reorder (state update before the `last_open` update) has no observable effect: listeners run after both.
- **`hud/slot_history.rs`:**
  - Seed-on-open, hash dedup, intersect ordering (held order, fresh score), confirm/clear counters, count rejections, prune and the 4 s gap reset all match line for line.
  - `duration_since` on a backwards clock doesn't clear the history. C# doesn't clear on a negative gap either.
- **`hud/inventory_reader.rs`:**
  - Cache keyed on `SlotRect` and rebuilt each frame.
  - Visit order, and `over_world` only for the belt with the panel shut.
  - A `None` filter is the same as `!wearOnly || IsWearable`.
  - Same split of the readings.
- **`hud/inventory_panel.rs`:** gap/edge areas, `MINIMUM_STEP`, `f64::MAX` for an off-frame area, and the 0.6 agreement all match.
- **`hud/count_reader.rs`:**
  - Same insets, banker's rounding on the nominal height, same scan order and tie-breaking.
  - `GlyphSet` keeps insertion order. `correlate` returns 0 on a zero-size glyph, so no NaN.
  - `atlases[&anchor.height]` can't miss, because the anchor score is gated first.
- **`hud/vitals_reader.rs`, `hud_geometry.rs`, `inventory_geometry.rs`:** same rounding (`f64::round` is AwayFromZero; `icon_area` uses `round_ties_even`). `clamp` can't panic (fixed bounds). A zero track gives the same result as C#.
- **`hud/held_item_reader.rs`:** first-of-equals and runner-up logic are the same.
- **`dotnet_sort.rs`:** checked against .NET `ArraySortHelper` (IntroSort depth, PickPivotAndPartition, HeapSort/DownHeap, InsertionSort) and `double.CompareTo` NaN ordering. Matches.
- **`items/item_catalog.rs`:**
  - The parallel load keeps directory order, so first-wins indexes are unchanged. IO errors fail the load; malformed JSON drops just that item.
  - Duplicate short names keep the first where C# threw. Already a `TODO(port)`.
- **`items/skin_catalog.rs`:**
  - The parallel rescan applies results in enumeration order, and the Incomplete/Ignored/Unmapped bookkeeping is the same.
  - A non-object manifest root is ignored where C# threw. Already a `TODO(port)`.
- **`items/icon_signature.rs`:** the precomputed composited-cell index list (`a >= 0.1`) is equivalent to C#'s `if (a < 0.1) continue` for all non-NaN alpha, and alpha comes from a decode.
- **`items/slot_crop.rs`:** weight ranges are never empty, so there is no division by zero.
- **`items/icon_matcher.rs`:** the empty ranking returns early, and `ranked[1..]` only runs after `first()`.
