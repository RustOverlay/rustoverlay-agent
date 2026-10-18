# Review: belt-corpus, belt-recorder, workspace build files

Scope: `crates/belt-corpus` (src, examples, parity/), `crates/belt-recorder`, workspace and crate
`Cargo.toml`s, `scripts/check.sh`, `.gitignore`, `build.rs` files. Compared against
`tools/BeltCorpus`, `tools/BeltRecorder`, `parity/cstrace/Program.cs`. Read-only; nothing was built.

## Findings

### 1. MEDIUM — `--skins` summary line prints in random order, so parity flakes
- `crates/belt-corpus/src/session.rs:53-54`, and the cause in `crates/ro-vision/src/items/skin_catalog.rs:38,58`
- `SkinCatalog.unmapped` is a `std::collections::HashMap` with `RandomState`, and `unmapped_labels()`
  goes through `.values()`. The TODO at session.rs:53 says the order only *differs* from C#. In fact it
  changes **on every run**. C# kept a `Dictionary` with no removals, so it printed labels in the order
  it first saw them.
- What goes wrong: with two or more unmapped workshop labels, the `skins: … unmapped labels a x3, b x1`
  line comes out in a different order on each run. The `skins` and `skins-confirm` configs in
  `parity/run.sh` then report `DIFFERS` at random, or pass by luck.
- Fix: make `unmapped` an `IndexMap<String, (String, u32)>` (ro-vision already depends on `indexmap`)
  so it keeps first-seen order, then remove the TODO.

### 2. MEDIUM — the parity trace check only counts missing lines, not extra ones
- `crates/belt-corpus/parity/run.sh:56-57`
- `mism=$(diff cs rs | grep -c '^<')` counts lines that are in C# but not in Rust. Lines that exist only
  in the Rust trace (`>`) never set `fail`. The tracers' exit codes are also thrown away
  (lines 42-44), and stderr is merged into the trace file.
- What goes wrong:
  - The Rust tracer prints every line C# prints, then panics. The panic text lands in the file as
    `>` lines only, so the result is `PARITY OK`.
  - Rust enumerates a strip C# does not. That is also `>` only, so it passes too.
  - The printed `trace N/N slots exact` is then false.
- Fix: decide pass/fail with `diff -q` on the two trace files, as the report check at line 47
  already does. Count mismatches with `grep -c '^[<>]'`. Append `exit $?` for each tracer, as
  lines 38/40 do for the scorers.

### 3. LOW — parity can test stale Rust binaries
- `crates/belt-corpus/parity/run.sh:9-10,15`
- `RS_BIN` and `RS_TRACE` are hard-coded to `$REPO/target/release/…`. If `CARGO_TARGET_DIR` (or
  `build.target-dir`) is set, line 15 builds into that other directory and succeeds. The script then
  runs whatever older `.exe` is still in `target/release`. No such override exists today; `target/`
  already holds a `release/` from earlier builds.
- Fix: read the directory from
  `cargo metadata --format-version 1 --no-deps --manifest-path … | sed …target_directory…`, or pass
  `--target-dir "$REPO/target"` explicitly on line 15.

### 4. LOW — `--departure` parses with the invariant culture; C# used the current culture
- `crates/belt-corpus/src/args.rs:51-54`, compared with `tools/BeltCorpus/Program.cs:59`
- C# called `double.Parse(departure)`, which uses the current culture and allows thousands
  separators. Under en-US, `--departure 1,5` gives `15`; Rust rejects it. Under a comma-decimal
  culture, `0.05` parses differently again.
- The scores are printed the same way in C# (`{x:0.000}`, current culture), so its report only
  matches the Rust `.`-decimal output under a `.`-decimal culture.
- `run.sh` never passes `--departure`, so parity is unaffected. Using the invariant culture is the
  better behaviour, so this is a divergence to note rather than to fix.
- Fix: add a `// TODO(port):` at args.rs:53 recording the deliberate change, as the porting rules
  require. Optionally pin `CultureInfo.InvariantCulture` in the C# tool for parity runs.

### 5. LOW — `std::env::args()` panics on a non-Unicode argument
- `crates/belt-corpus/src/main.rs:11`, `crates/belt-recorder/src/main.rs:18`,
  `crates/belt-corpus/examples/parity_trace.rs:22`, `examples/profile_phases.rs:41`
- On Windows, a path containing a lone UTF-16 surrogate panics with "invalid unicode in argument"
  before any parsing happens. C# would accept it. Ordinary non-ASCII paths are fine: they are valid
  Unicode, and the path flows through `PathBuf` and `HSTRING::from(&OsStr)` correctly.
- Fix: use `args_os()`. Keep `OsString` for `directory` and `montage`, and convert flag values with
  `to_str()`, returning `ArgsError::Invalid` when that fails.

### 6. LOW — montage font has no fallback when the family is missing
- `crates/belt-corpus/src/gdiplus.rs:167-180`, called from `montage.rs:39`
- `new Font("Consolas", 10)` in System.Drawing catches GDI+ `FontFamilyNotFound` and falls back to
  `GenericSansSerif`. `Font::system` returns the error instead.
- What goes wrong: on a machine without Consolas, `--montage` fails with `montage: …` and exit code 1
  after the score sheet has printed. C# writes the sheet and returns 0 or 2.
- Fix: when `GdipCreateFontFamilyFromName` returns `FontFamilyNotFound`, retry with
  `GdipGetGenericFontFamilySansSerif`.

### 7. LOW — recorder count arguments are stricter than `int.Parse`
- `crates/belt-recorder/src/main.rs:140-142`
- `int.Parse` accepts surrounding whitespace, a leading `+`, and negative numbers; a negative
  frame count made the loop a no-op. `u32::from_str` rejects all of these and exits 1.
- This is a dev tool, so the impact is cosmetic.
- Fix, if parity of the error behaviour matters: `ro_core::invariant::parse_i32(text).map(|n| n.max(0) as u32)`.

## Checked and found correct

- **PNG24 writer** (`belt-recorder/src/png24.rs`): it does not hand-roll CRC or zlib. It wraps a BGR
  buffer in `GdipCreateBitmapFromScan0(PixelFormat24bppRGB)` and saves with the built-in PNG encoder
  CLSID, as the C# `LockBits` and `Save` did. The stride `(w*3+3)&!3` matches GDI+ for 24bpp. Padding
  bytes are zeroed, matching the reused `byte[Stride]` in C#. The buffer outlives the bitmap. On the
  error path, a failed create leaves no handle to dispose.
- **belt-corpus argument parsing** (`args.rs`) matches C#'s quirks:
  - first occurrence wins;
  - a flag's value may be the next flag;
  - `args[1]` is treated as truth unless it starts with `--`;
  - empty truth becomes six `-` entries;
  - `--confirm` falls back to the `TryParse` value of 2;
  - a negative `--confirm` or `--coarse` is clamped in a way that behaves the same as C#'s `>= n` and `> 0` tests;
  - the strip-name regexes and the inclusive `--from`/`--to` ordinal comparison match;
  - unscored slots keep NUL marks;
  - the `{,-18}` and `{,4}` padding and `0.0` and `0.000` formatting match
    (`dotnet_format::fixed`, which uses 15 significant digits and rounds half away from zero);
  - exit codes are 0, 1 and 2.
- **Montage**: the GDI+ call sequence matches System.Drawing. That covers the 32bpp ARGB sheet,
  `Clear(24,24,28)`, nearest-neighbour with half-pixel offset, `DrawImageRectRectI` in UnitPixel,
  `DrawString` with a zero-size layout rectangle and null format, and the PNG encoder.
- **run.sh** in other respects:
  - stale `cs/`, `rs/` and `trace/` outputs are deleted before every run;
  - scorer exit codes are part of the diffed text;
  - `wrote <path>` is normalised before the diff;
  - a montage present on only one side fails;
  - `slots == 0` fails, so a crashed C# tracer or an empty corpus cannot pass;
  - build failures exit 1;
  - `set -u` without `-e` is fine because every check sets `fail` explicitly.
- **check.sh**: a build failure exits with cargo's code. `exit $(( t || c ))` returns 1 when either
  test or clippy fails. The awk field positions match `test result:` lines.
- **Release profile**: `lto="thin"`, `codegen-units=1`, and the default unwinding panic strategy.
  Nothing needs changing, and there is no `panic=abort`.
- **Dependencies**: every version requirement resolves in `Cargo.lock`. Some examples are chrono
  0.4.45, regex 1.13.1, windows 0.62.2 (a single copy), tokio 1.53.3, eframe 0.36.2 and velopack
  1.2.161. No duplicate `windows` major versions.
- **Feature unification**: every crate declares the chrono `clock` feature it uses for
  `Local::now`/`Utc::now`, and the serde `derive` it uses. Each crate's `windows` features cover
  the `Win32::*` modules it imports; belt-corpus's `Win32_System_Threading` is for
  `profile_phases`'s `GetProcessTimes`. Every crate also declares the tokio features it needs, so
  no crate compiles only because a sibling turned a feature on.
- **.gitignore**: `bin/` and `obj/` correctly ignore `parity/cstrace/{bin,obj}` (verified with
  `git status --ignored`). No Rust `src/bin/` directory exists that the same rule would wrongly hide.
- **build.rs**: ro-app's manifest embed is gated on `CARGO_CFG_WINDOWS`. ro-net's
  `rerun-if-changed` covers the only `.proto` file.
