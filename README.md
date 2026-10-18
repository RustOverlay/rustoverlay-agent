# RustOverlay Agent

Windows desktop agent for RustOverlay. It reads your own Rust client's log, and shares your
position, orientation and combat events with your teammates through the RustOverlay relay.

**This repository is public so you can verify what the agent does.** Everything it reads is listed
below, and everything it sends is defined by [`proto/`](proto/) — vendored here in full, so you can
read the wire format in the same clone as the code that fills it in. It is held byte-identical to
the canonical [wire contract](https://github.com/RustOverlay/rustoverlay-contracts) by CI. The relay
only ever receives what this code chooses to send.

## What it reads

| Source | What for |
|---|---|
| `<Rust>\output_log.txt` | combat events, and your position/orientation when you press a bound key |
| `<Rust>\cfg\client.cfg`, `<Rust>\cfg\keys.cfg` | display mode, UI scale and your inventory key, to calibrate the overlay |
| `<Rust>\Bundles\items` | the game's own item definitions and icons, to recognise what is in a slot |
| `<Rust>\Bundles\textures` | the icons of approved skins, read from the game's own files, so skinned items are recognised too |
| Steam's workshop cache for Rust | skin icons the game has downloaded, so skinned items are recognised too |
| Steam's item schema (`<Steam>\userdata\<account>\inventorymsgcache`) | the list of approved skins and which item each repaints; only the schema file is opened, never the inventory list beside it |
| The screen, while Rust is the window in front | your belt, inventory and vitals; never anything while you are in another window |
| Rust's game audio | gunshots, to place them; nothing else is captured |
| Windows registry (`SOFTWARE\Valve\Steam`) | locating your Rust install |

Nothing is downloaded. Everything above is on your machine already; the only connections the agent
makes are sign-in, the teams API and the relay.

## What it does not do

- **No injection.** Nothing is loaded into the game process. The agent reads a log file the game
  writes on its own, using commands any player can run at auth level 0.
- **No memory reading, no packet interception, no modification of the game.**
- **No credentials.** The agent holds no Steam password and no Rust+ token. It authenticates to the
  relay with a per-install token that grants access to nothing else.
- **No data about players outside your team.**

## Status

The data pipeline, screen capture and the overlay all work: log tailing and parsing, HUD vitals and
inventory reading, the relay client, and a transparent panel that draws teammate health, held item,
ammo, syringes and barricade covers over the game. See [`docs/log-format.md`](docs/log-format.md)
for the log format and the traps in it.

The wall count is the least proven part — wooden barricade covers appear in no matcher corpus, so
that figure has no measured accuracy behind it yet. It abstains rather than guessing, so the failure
shows up as a blank, not a wrong number.

## Install

Grab `RustOverlay-win-Setup.exe` from [Releases](https://github.com/RustOverlay/rustoverlay-agent/releases).
It installs per-user, needs no administrator rights, and updates itself from that same page — new
versions download in the background and apply the next time you close the agent, never mid-session.

> Windows SmartScreen will warn you the first time: the installer is not code-signed yet. Every
> release is built by GitHub Actions from this repository, and you can prove the download came from
> that build rather than from someone else:
>
> ```
> gh attestation verify RustOverlay-win-Setup.exe -R RustOverlay/rustoverlay-agent
> ```

## Build

```
cargo build --release
cargo test
cargo run --release -p ro-app
```

Requires Windows and the MSVC build tools; `rust-toolchain.toml` pins the Rust version, which rustup
installs on first build. A plain clone builds — there is no sibling checkout to arrange and no
submodule to initialise, because `proto/` is vendored. That is deliberate: "read the source and see
what we send" is not a real offer if reproducing the build needs anything you have not been given.
`scripts/check.sh` runs build, tests and clippy the way CI does.

`crates/ro-core/tests/real_log_smoke.rs` runs the pipeline over whatever Rust install is on the
machine and prints a summary. It passes silently when Rust is absent, so it is safe in CI — its job
is to catch log-format drift after a game update, which fixtures cannot do.

Two calibration tools sit beside the agent: `belt-recorder` captures HUD samples from a live session
and `belt-corpus` scores the icon matcher against them. The matcher's thresholds are only as good as
the backgrounds they were measured on, and these are how that gets measured — see
[docs/matcher-tools.md](docs/matcher-tools.md).

## Configuration

The **Settings** tab covers what you are likely to want: the three overlay binds, the overlay's
size, the inventory grids' size on top of that, and where it all sits in the game window. Changes apply as you make them and are written to
`%LOCALAPPDATA%\RustOverlay\settings.json`, so it is worth having Rust running behind the window
while you drag the size slider.

Everything else is configuration. Defaults live in `appsettings.json` next to the executable;
override them per-user in that same `settings.json` — the settings tab only ever rewrites the
`Overlay` block and leaves the rest alone — or with environment variables prefixed `RUSTOVERLAY_`:

```
RUSTOVERLAY_Agent__RustPath=D:\Games\Rust      # skip Steam auto-detection
RUSTOVERLAY_Agent__ReplayFromStart=true        # parse the existing log on launch
RUSTOVERLAY_Overlay__ToggleKey=f4              # key that shows and hides the panel
RUSTOVERLAY_Overlay__Inspect__HoldKey=leftalt  # held to open a teammate's inventory
RUSTOVERLAY_Overlay__Inspect__CycleKey=c       # tapped while held, to change teammate
RUSTOVERLAY_Serilog__MinimumLevel__Default=Debug
```

Logs are written to `%LOCALAPPDATA%\RustOverlay\logs`, rolling daily, seven days retained.

## The overlay

A transparent panel at the top-left of the game window, listing each online teammate's health, held
item, rounds loaded and carried, medical syringes and wooden barricade covers.

It is drawn only while Rust is the window you are actually looking at — not a preference, but so a
panel carrying your team's health is never sitting over a browser or a screen share. `F4` toggles
it within that. Override it under `Overlay` in `settings.json`.

Counts that lean on your teammate's main inventory grid are shown dimmed and italic, because that
container is only legible while their inventory is open and can be minutes old. Belt-sourced counts
are live. An item the matcher cannot identify confidently is left out rather than guessed at, so a
missing number means "not established", never "none".

### Inspecting a teammate

Hold `Left Alt` and the selected teammate's whole inventory — wear row and backpack, main grid, then
belt — unfolds beneath the list. Tap `C` while holding to move down the list; release to close. A
square marked with an amber corner is one the matcher could not narrow past a set of look-alikes, so
the icon is the likeliest of them rather than a certainty.

The wear row closes up around what is actually worn — an empty attire slot is left out rather than
drawn as a blank. The belt and main grid keep theirs, because those are read by position and a
missing square would slide everything after it out of place.

### How the keys are read

Two ways, for one reason: the hold has to know when you let go, and a tap has to not be missed.

The hold key is **sampled** — the agent asks Windows whether it is down, sixty-odd times a second,
and takes nothing. That is the only way to see a release, and it is why the inspector can be a hold
at all.

The taps — the toggle, the cycle key, and anything in `SelectKeys` — are **registered** with Windows
while Rust is the foreground window, so the OS reports the press rather than the agent going looking
for it. Sampling cannot be made reliable here: Windows timers fire on a ~15.6ms tick and a busy
moment stretches that, so a quick tap can begin and end without ever being sampled. Registration has
no such hole.

Registered keys are claimed only while you are actually in Rust, and dropped the instant you leave
it. That matters because a registered key is taken system-wide: left claimed, a cycle key of `C`
would stop `C` reaching every other application on your machine. If another program already owns a
key, the agent says so in the log and falls back to sampling it rather than the bind going dead.

None of this is a keyboard hook. Nothing is installed into another process, and no key other than
the ones you bound is ever observed.

One consequence is untested: a registered key is taken from the message queue, but games reading raw
input may see it regardless. So `Overlay:Inspect:SelectKeys` — which jumps straight to a row rather
than cycling — still ships empty, on the assumption that binding the number row may select the
second teammate *and* put belt slot 2 in your hands. The numpad is the safe option on a keyboard
that has one.

> Exclusive fullscreen cannot be drawn over by any topmost window. The agent reads
> `graphics.screenmode` at startup and warns you if it is set; switch to borderless and restart.

## Setting the keybind

Position and orientation are only written to the log when you run `printpos` and `printeyes`, and
binds do **not** repeat while a key is held. So you get a sample per keypress, not a stream. Bind
them together so the agent can pair them:

```
bind mouse1 "printpos;printeyes"
```

`mouse1` gives one exact transform per shot. Avoid binding movement keys — `W` floods the log.

If `-nolog` is set in your Steam launch options, no log is written at all and the agent will tell
you so.

## Licence

GPL-3.0. See [LICENSE](LICENSE).

Bundled third-party assets:

- **Roboto Condensed Bold**, Apache-2.0 — the typeface Rust draws its HUD counts in, used to
  recognise them. See
  [the licence](crates/ro-capture/assets/RobotoCondensed-Bold.LICENSE.txt).

Rust's item icons and definitions are **not** bundled. They are Facepunch's, and are read from the
player's own install at runtime.
