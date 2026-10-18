# The capture loop

`VisionService` reads the HUD off the screen for as long as the agent runs and publishes the result
to `LoadoutState`. It is passive throughout: it reads the composited desktop and the player's own
config files, and never touches the Rust process.

## Cadence

| | interval | why |
|---|---|---|
| key poll | 50 ms | one `GetAsyncKeyState` call |
| resting frame read | 1 s | a loadout changes on a human timescale |
| boosted frame read | 100 ms | for 2 s after the inventory key is pressed |
| idle backoff | 2 s | Rust not running, or not focused |

A frame costs ~21 ms cold and ~0.2 ms when nothing has changed, against ~16 ms for the screen grab
itself. At rest that is under 2% of one core.

**The key poll is on its own clock, and has to be.** Tying it to the frame cadence samples it once a
second, and `GetAsyncKeyState` reports the key's *current* state — so a press and release inside that
second leaves nothing to find afterwards. That is exactly the case it exists for: an inventory opened
and shut in under a second is invisible to a 1 Hz reader.

**Why boost rather than read on the press.** The panel animates open, so the moment of the keypress is
the wrong moment to look. Reading repeatedly for two seconds afterwards catches it wherever the
animation lands, and catches the close the same way.

## Reading the player's keybind

Rust writes `cfg/keys.cfg` as `bind <key> <command>[;<command>...]`. `KeyBindings` parses it and
looks up `inventory.toggle`, falling back to Rust's own default of Tab when the file is missing or
the command is unbound. A player who rebound the key would otherwise never be watched at all.

> **Gotcha — match the command, not a prefix.** In a real config, Tab holds `inventory.toggle` while
> Q ends in `inventory.toggle`**`crafting`**. A `StartsWith` match reports Q as the inventory key and
> the agent then watches the skin-cycling key. Commands are split on `;` — respecting quotes, since
> `chat.say "a;b"` is one command — and compared on their first word.

## Two things deliberately not done

**No keyboard hook.** A `WH_KEYBOARD_LL` hook would see every keystroke on the machine. That is a
privacy problem for a tool whose purpose is sharing data, and it is the shape anti-cheat heuristics
look for. `KeyWatcher` queries a single virtual key and intercepts nothing.

**No capture unless Rust is focused.** `CopyFromScreen` reads the composited desktop, so grabbing
while the player has alt-tabbed captures whatever they switched to — a browser, a chat window. The
foreground check is a privacy control first and a saving second.

## What gets read

The belt is read every time; it is drawn in world view too. The main grid, wear row and backpack are
read only when `InventoryPanel.IsOpen` says the panel is drawn — see docs/item-matching.md, where
that detector is load-bearing rather than an optimisation, because the matcher invents items when
pointed at world content.

**Shutting the panel does not clear what it held.** `LoadoutState` keeps the last reading taken
while it was drawn, alongside `HiddenSince`. Dropping it published an empty inventory a second after
a correct one, which reads as the agent having lost track rather than having stopped looking. It is
retained rather than trusted: the belt goes on being read live, but a hidden main grid does go stale
— a reload draws ammo out of it and dying empties it — so it is presented as *as last seen*, never
as current.

## Where frames come from

`IFrameSource` is the seam between obtaining a frame and reading one. Backends differ enormously in
setup and lifecycle but agree on the only thing the readers want — a rectangle of pixels — so
swapping one for another touches nothing in `Core`.

`DuplicationFrameSource` (DXGI Desktop Duplication) is preferred, with `ScreenFrameSource` (GDI
`CopyFromScreen`) as the fallback. Duplication draws no capture indicator, costs a GPU copy rather
than a blit, and survives exclusive fullscreen on DirectX 11.1+ unless the game opts out.
`AgentHost` probes it once at startup and logs which one it picked, because duplication is
unavailable in some remote-desktop sessions and on virtualised display drivers.

**Verified against the real game.** Rust in exclusive fullscreen at 1440x1080 duplicates correctly:
the frame is the game, the slot geometry lands pixel-exact on every container, and the run logged
neither a fallback demotion nor a DPI pause. That was the open question the backend was written to
answer, and it is the one thing GDI could not have done.

Windows Graphics Capture was considered and rejected: going borderless with it needs the
`graphicsCaptureWithoutBorder` capability in a *package manifest*, which means MSIX identity and
conflicts with Velopack distribution. Without it Windows draws a yellow border around the captured
window.

One instance is shared by the capture loop and the debug grab button, so implementations must be
thread-safe — a GPU-backed one holds device state a concurrent call would corrupt.

**A frame is handed to a callback rather than returned.** Backends reuse one pixel buffer between
grabs, because allocating a display's worth of pixels several times a second is not free. A caller
that kept the buffer past the call would read it while the next frame was being written into it —
half of one frame and half of the next, which is exactly the input that makes the matcher answer
confidently and wrongly. Scoping the frame to the callback puts that beyond a caller's reach, and
lets the backend hold its lock across the read rather than trusting a comment.

### Falling back

`FallbackFrameSource` wraps the pair. The startup probe only tells us duplication works on the
*primary* display, which is not the question — the game may be on another one, on another adapter, or
rotated. So the preferred backend is also watched at runtime: ten consecutive reads that produce
nothing, while a Rust window is actually on screen, demote it in favour of GDI.

No frame is lost to a demotion. A failed attempt is retried on the other backend in the same call;
the demotion only decides which is tried first, and so only saves the wasted attempt.

The demotion expires after a minute rather than lasting the session, because the conditions that
cause it — a rotated display, a window parked across two monitors, a driver reset — are things the
player undoes. A permanent demotion would silently cost exclusive-fullscreen capture for the rest of
the run. Failures are counted as a *consecutive* run, so an intermittent one never accumulates its
way to a demotion over a long session.

A window that is not on screen at all is not counted as a failure. Counting it would demote the
preferred backend before the game had ever started.

### What duplication has to get right

Duplication is per-display, not per-window, so `OutputSelection` picks the display and the crop
within it. That logic is kept out of the D3D code because no test runner has a GPU and it is where
the bugs are.

> **Gotcha — a window straddling two displays is refused, not cropped.** The readers size their
> geometry from the frame they are given, so a short frame puts every HUD offset in the wrong place
> and the matcher answers confidently from the wrong pixels. It only happens mid-drag.

Three lifecycle facts drive the rest of the implementation:

- **`ACCESS_LOST` is routine.** Resolution changes, fullscreen transitions, the secure desktop and
  driver resets all raise it — including every alt-tab out of exclusive fullscreen, the case this
  backend exists for. The session is torn down and rebuilt on the next call. The staging texture is
  additionally size-checked on each copy, so a mode change that somehow did not raise it still
  cannot copy into a texture of the old size.
- **The session is released when Rust closes**, rather than held until the agent exits. It pins a
  D3D device and a display's worth of staging memory.
- **One duplication of an output per process.** A second `DuplicateOutput` on an output this process
  already duplicates fails with `E_INVALIDARG`, which is the other reason the backend is registered
  as a singleton. Other processes duplicating the same output — OBS, a screen share — are unaffected.
- **A timeout is not a failure.** It means the desktop has not changed, so the pixels already held
  are still current.
- **The first frames after `DuplicateOutput` often carry no image.** They report cursor movement,
  with `LastPresentTime` zero, and have to be released and re-acquired.

The staging texture is mapped and unmapped inside the copy rather than handed to the reader. A frame
is read by the whole recognition pass, slot matching included, and holding a mapping open across all
of it stalls the GPU for far longer than the copy costs.

A rotated display duplicates in its *unrotated* orientation, so every coordinate would be transposed.
That is detected and refused rather than read wrongly.

## Display scaling

The agent declares **PerMonitorV2** DPI awareness in `app.manifest`. This is a correctness
requirement, not a cosmetic one. A process that is not per-monitor aware gets lied to: Windows scales
the rectangles from `GetClientRect` and `ClientToScreen` to match what the process expects, while
duplication goes on reporting true physical pixels. At 125% the crop lands a quarter out and every
`InventoryGeometry` offset with it — silently, and plausibly enough to be believed.

`DisplayScaling` is the belt to that braces. The manifest can be bypassed — launching through
`dotnet Agent.dll` uses the host's manifest rather than ours — so the capture loop rechecks before
each read and **pauses instead of reading** when the display is scaled and the process is not aware.
Under the zero-wrong invariant a paused reader beats a confident wrong one. Nothing latches: moving
the window back to an unscaled display resumes reading.

The check is rechecked per read rather than cached because the answer changes when the window moves
between displays. It costs three P/Invokes at 1 Hz.

## Known gaps

- **Display scaling is handled but only lightly exercised.** Both displays on the development
  machine are at 100%, so the paths below are reasoned rather than observed. Worth a look on a
  scaled setup.
- **Catalogue load blocks the service's first read.** ~1233 icons are hashed at startup. It happens
  on the background service rather than the UI thread, so nothing freezes, but the first reading is
  several seconds late. Caching the signatures to disk is the fix.
- **Nothing is published off the machine.** `LoadoutState` is local; the relay is not wired.
