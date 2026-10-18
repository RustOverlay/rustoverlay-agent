# ro-capture review

Scope: `crates/ro-capture/src/**`, compared against `src/RustOverlay.Agent.Capture/*.cs`.
Read-only review, 2026-10-17. Nothing was built or run.

**Summary:** no high-severity bugs. The FFI is sound throughout. Every COM object, GDI+ object,
process handle and snapshot handle is released, and the duplication lost-device path rebuilds
the same way the C# does. The findings below are one Rust-only performance regression (the
signature cache) and a few error-path and robustness gaps. Most of those gaps are inherited from
the C#.

## Findings (ranked)

### 1. Medium: the signature cache is rewritten in full on every skin sweep that decodes anything
- `skin_watcher.rs:66-67`, `signature_cache.rs:68-82`
- **What happens:** `sweep` calls `save_cache` every time `build_cached` decodes a new icon.
  `save` re-encodes and rewrites the whole map, about 4.2 KB per entry. With ~1250 catalogue
  icons plus a populated skin cache, that is roughly 10 MB or more per save. The encode and the
  write run on the vision thread while holding the entries lock.
- **Failure scenario:** the player joins a populated server. Steam writes hundreds of skin
  directories over a minute or so, and each one sets `stirred`. Polling is every 50 ms, so every
  poll that finds even one new icon triggers another full rewrite. The result is hundreds of
  ~10 MB writes (GBs of SSD writes). The vision loop stalls on each one, during exactly the
  boosted window where frame cadence matters. The C# had no cache, so this is new to the port.
- **Fix:** debounce saves. Keep `dirty` set, save at most once every 30–60 s from the loop, and
  save once more when the loop exits on stop. Alternatively, use an append-only journal and
  compact it at startup.

### 2. Low: an `Err` from the preferred backend skips the fallback and the health tally
- `fallback_frame_source.rs:41` (`self.preferred.read(use_frame)?`), with errors coming from
  `duplication_frame_source.rs:197-201` (`taken?`)
- **What happens:** errors from `copy_out` propagate straight out of `FallbackFrameSource::read`.
  The failing call sites are `CreateTexture2D` for the staging texture, `Map`, and the
  `IDXGIResource -> ID3D11Texture2D` cast. When that happens:
  - the GDI fallback is not tried for that frame;
  - `record_failure` is never called;
  - the session is not released.
- **Failure scenario:** suppose staging creation or `Map` keeps failing while `AcquireNextFrame`
  keeps succeeding, for example `E_OUTOFMEMORY` at 4K under VRAM or commit pressure. Every read
  then fails and only logs "Frame read failed". The backend is never demoted, so desktop capture
  is never used. This matches the C# (the exception escaped the lock there too), but it
  contradicts the type's own promise that "no frame is lost to a fallback".
- **Fix:** in `FallbackFrameSource`, treat `Err` from `preferred` like `Ok(false)`: log it, record
  the failure, then try the fallback. Optionally, `State::capture` could also `release()` on a
  `copy_out` error so the next read rebuilds the device.

### 3. Low: the signature cache is never pruned
- `signature_cache.rs:34`, `:60-64`, `:85-100`
- **What happens:** entries are only ever inserted or replaced.
- **Failure scenario:** some entries go stale permanently:
  - skins Steam evicts from its workshop cache;
  - every path under an old install location after the user moves their Steam library.

  These stale entries are decoded into memory on every launch and rewritten on every save.
  That makes the cost in finding 1 grow without limit.
- **Fix:** track which keys were touched by `get` or `put` this session. The first sweep rescans
  every skin, so a touched entry means a live file. Write only touched entries.

### 4. Low: the cache write is not flushed to disk before the rename
- `signature_cache.rs:77-79`
- **What happens:** the file is written with `File::create(..)?.write_all(..)` and then renamed,
  without a `sync_all()` in between.
- **Failure scenario:** after a power cut or BSOD, NTFS can persist the rename before the data.
  The result is a zero-filled or truncated `icon-signatures.bin`. `decode` correctly rejects it,
  so nothing is corrupted, but the next launch pays the full multi-second decode.
- **Fix:** keep the `File` handle, call `write_all` and then `sync_all()`, then rename.
  Optionally, use a per-process temp name (e.g. append the PID) so two concurrent writers, such
  as the agent plus a `belt-corpus` example on the same file, cannot interleave writes into one
  `.tmp`.

### 5. Low: a failed glyph render is not cached
- `font_glyph_atlas.rs:58-65`
- **What happens:** on a GDI+ error, `at()` returns a fresh empty `GlyphSet` without inserting it
  into the cache. The ported TODO covers abstaining instead of throwing, not this.
- **Failure scenario:** a persistent GDI+ failure (e.g. after `GdipPrivateAddMemoryFont`
  problems, or under resource exhaustion) means every count read does the following:
  - re-runs the whole render (65k `GdipBitmapGetPixel` calls on the 256² canvas) while holding
    the atlas mutex, which serialises the parallel slot readers;
  - logs `error!`.

  That repeats for every slot × 3 heights per frame, up to 10 fps while boosted.
- **Fix:** cache the empty set for that height, or at least log once per height.

### 6. Low: the stop request is ignored during startup
- `vision_service.rs:66-109`
- **What happens:** `ItemCatalog::load`, `build_cached` (seconds cold) and `FontGlyphAtlas::new`
  all run before the loop's first `is_stopped()` check.
- **Failure scenario:** the user quits during the first launch's decode. `Services::shutdown`
  waits 5 s, gives up, and the process exits with rayon workers mid-decode. `save_cache` never
  runs, so the next launch decodes everything again. Nothing is corrupted, because the write
  goes through a temp file.
- **Fix:** check `stop.is_stopped()` after each startup phase. Make `save_cache` cheap enough to
  call on the way out; finding 1's debounce gives you that.

### 7. Low (inherited from C#): frame and sweep scheduling use the wall clock
- `vision_service.rs:120-150` (`next_read`, `boosted_until`), `skin_watcher.rs:80-85`
- **What happens:** both use `SystemTime`, as the C# did with `DateTimeOffset.UtcNow`.
- **Failure scenario:** a backward clock step (manual change, or a large time-sync correction)
  of N minutes stops frame reads for N minutes, because `now < next_read`. The backstop sweep
  stalls the same way.
- **Fix (does not change output):** schedule with `Instant` and keep `SystemTime` only for the
  `now` passed to `LoadoutState::set` and `SlotHistory::confirm`.

## Checked, no issue found

- **Duplication COM lifetimes** (`duplication_session.rs`, `duplication_frame_source.rs`):
  - Session field order releases staging, then duplication, then context, then device.
  - The frame resource is dropped inside `take` before `ReleaseFrame`, and every successful
    `AcquireNextFrame` is paired with a `ReleaseFrame`.
  - `Map` is always matched by `Unmap`; the early `?` from `Map` needs none.
  - The copy length `RowPitch * Height` matches the C#.
- **ACCESS_LOST and mode changes:** every acquire error other than timeout releases the session.
  The next read rebuilds it, and `has_pixels` is cleared so stale pixels are not served. The
  staging size is rechecked, as in the C#.
- **Threading:** the device context and pixel buffer are serialised by `State`'s mutex across the
  capture loop and the debug-grab thread. Poisoning is recovered, and the state stays consistent
  if the callback panics. Lock order is always frames before loadout, and `ro-app`'s grab
  (`state/agent.rs:88`) takes no other lock, so there is no deadlock.
- **`rust_window.rs`:**
  - `OpenProcess` handles are closed before the result is checked; the Toolhelp snapshot is
    always closed.
  - The PID→answer cache lives for one enumeration only, so PID reuse cannot poison it.
  - The cached HWND is revalidated with `IsWindow`, the same guarantee the C# gave.
  - Edge-case semantics match .NET's `MainWindowHandle`: an unowned, visible window, with
    `GetWindow` returning `Err` meaning no owner.
- **GDI+ wrappers:**
  - Every `Gp*` object has a `Drop` (dispose, delete).
  - Failure inside `from_file` after creation is disposed through `Drop`.
  - The `GdiplusStartupInputEx` layout matches the header on x64.
  - Font families are borrowed from the collection, as System.Drawing's clones are.
  - The font bytes are `'static`.
  - `Graphics<'a>` borrowing the bitmap makes the lifetime of `LockBits` vs. a live `Graphics`
    compile-time checked.
- **`signature_cache::decode`:** every length is checked through `get` or `read_exact`, so it
  cannot panic. Trailing bytes are rejected, and a version mismatch means an empty cache.
- **DPI:** `ro-app/app.manifest` declares PerMonitorV2, and `display_scaling` matches the C#
  check.
- **Pixel parity:** the glyph atlas rounding (`round_ties_even`) and the icon downscale call
  sequence match the C# exactly.
