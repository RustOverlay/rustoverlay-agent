# Configuration

The **Settings** tab covers what most players want: the three overlay binds, the overlay's size, the
inventory grids' size on top of that, and where it all sits in the game window. Changes apply as you
make them, so it is worth having Rust running behind the window while you drag the size slider.

## Files and environment variables

Defaults live in `appsettings.json` next to the executable. Override them per-user in
`%LOCALAPPDATA%\RustOverlayData\settings.json` — the Settings tab only ever rewrites its `Overlay`
block and leaves the rest alone — or with environment variables prefixed `RUSTOVERLAY_`:

```
RUSTOVERLAY_Agent__RustPath=D:\Games\Rust          # if Steam auto-detection can't find Rust
RUSTOVERLAY_Serilog__MinimumLevel__Default=Debug   # verbose logs, for bug reports
```

Keybinds and overlay size belong in the Settings tab rather than here.

Logs are written to `%LOCALAPPDATA%\RustOverlayData\logs`, rolling daily, seven days retained.

## Reading the overlay

Counts that lean on a teammate's main inventory grid are shown dimmed and italic, because that
container is only legible while their inventory is open and can be minutes old. Belt-sourced counts
are live. An item the matcher cannot identify confidently is left out rather than guessed at, so a
missing number means "not established", never "none".

In the inspector, a square marked with an amber corner is one the matcher could not narrow past a set
of look-alikes; the icon is the likeliest of them rather than a certainty. The wear row closes up
around what is actually worn. The belt and main grid keep their empty squares, because those are
read by position and a missing square would slide everything after it out of place.

The panel is drawn only while Rust is the window you are actually looking at, so a panel carrying
your team's health is never sitting over a browser or a screen share.

## How keys are read

Two ways, for one reason: the hold has to know when you let go, and a tap has to not be missed.

The hold key is **sampled** — the agent asks Windows whether it is down, sixty-odd times a second,
and takes nothing. That is the only way to see a release, and it is why the inspector can be a hold
at all.

The taps — the toggle, the cycle key, and anything in `SelectKeys` — are **registered** with Windows
while Rust is the foreground window, so the OS reports the press rather than the agent going looking
for it. Sampling cannot be made reliable here: Windows timers fire on a ~15.6ms tick and a busy
moment stretches that, so a quick tap can begin and end without ever being sampled.

Registered keys are claimed only while you are actually in Rust, and dropped the instant you leave
it. A registered key is taken system-wide: left claimed, a cycle key of `C` would stop `C` reaching
every other application. If another program already owns a key, the agent logs it and falls back to
sampling rather than the bind going dead.

None of this is a keyboard hook. Nothing is installed into another process, and no key other than
the ones you bound is ever observed.

One consequence is untested: a registered key is taken from the message queue, but games reading raw
input may see it regardless. So `Overlay:Inspect:SelectKeys` — which jumps straight to a row rather
than cycling — ships empty, on the assumption that binding the number row may select the second
teammate *and* put belt slot 2 in your hands. The numpad is the safe option on a keyboard that has
one.
