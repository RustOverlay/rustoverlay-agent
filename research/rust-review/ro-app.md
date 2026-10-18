# ro-app review (2026-10-17)

Scope: `crates/ro-app` against `src/RustOverlay.Agent` and docs/rust-port.md. Read-only; no build run.
Library behaviour was checked in the vendored sources (`winit-0.30.13`, `eframe-0.36.2`, `egui-0.36.2`,
`velopack-1.2.161`) under `~/.cargo/registry/src/index.crates.io-*`.

## Findings, ranked

### 1. Medium: the overlay window shows up in Alt-Tab and Task View
- `interop/window_styles.rs:18-24`, `interop/overlay_input.rs:210-239`, `app.rs:108-117`
- **Mechanism.** winit gives every unowned window `ON_TASKBAR` (`winit window.rs:1339`), and that
  becomes `WS_EX_APPWINDOW` (`window_state.rs:273-275`). egui's `with_taskbar(false)` only calls
  `ITaskbarList::DeleteTab` (`window.rs:1498-1530`), so `WS_EX_APPWINDOW` stays set. `make_click_through`
  only ORs bits in and never clears `WS_EX_APPWINDOW`. It also runs late: `window()` is called only
  from `anchor_to_rust`, which runs only while `wanted` (enabled, has teammates, Rust in front).
- **Scenario.** A solo player, or anyone before their first teammate appears, has an unstyled
  "RustOverlay Overlay" window. It is an Alt-Tab and Win+Tab entry with no `WS_EX_TOOLWINDOW` and no
  `WS_EX_NOACTIVATE`. Picking it moves focus to an invisible 1x1 window. Even after styling,
  `WS_EX_APPWINDOW` takes precedence over `WS_EX_TOOLWINDOW` for shell listing. C# applied
  `MakeClickThrough` in `OnSourceInitialized`, and WPF's `ShowInTaskbar=False` leaves no
  APPWINDOW bit (`OverlayWindow.xaml.cs:121-139`).
- **Fix.** In `make_click_through`, clear `WS_EX_APPWINDOW` (`(style & !APPWINDOW) | added`). Call
  `self.window()` on every tick, before computing `wanted`, so the styles land as soon as egui
  creates the viewport. Latent issue to note: winit's `apply_diff` rewrites `GWL_EXSTYLE` from its
  own flags on any flag change (`window_state.rs:398-408`). Nothing changes overlay flags after
  creation today, but if something starts to (for example a builder field that varies per frame), it
  would silently undo these styles.

### 2. Medium: a panic on the UI thread kills the agent, where C# kept running
- `main.rs:99-113`
- **Mechanism.** The hook logs and shows the box, then the panic unwinds out of the winit callback.
  winit catches it and resumes the panic after `run_app`, and the process ends. C#'s
  `OnDispatcherUnhandledException` set `e.Handled = true` and the app carried on
  (`App.xaml.cs:115-126`). The doc comment ("as the C# dispatcher's unhandled-exception box did")
  claims parity it does not have.
- **Scenario.** A latent indexing or `expect` bug in any view, or an egui assertion, hit mid-raid takes
  down the overlay and the uplink. With C# the user got a box and kept playing.
- **Fix.** Wrap the body of `AgentApp::ui` (and `OverlayView::show`) in
  `catch_unwind(AssertUnwindSafe(..))`. Log, show the box once, and skip the frame. Or, if exiting
  is the deliberate choice, correct the comment and record it as a `TODO(port)`.

### 3. Low-medium: background "repaint the main window" can repaint the overlay instead
- `session.rs:44`, `app.rs:56` (also every `Notify` built from them: team, sign-in, feed, status,
  auth, loadout)
- **Mechanism.** `Context::request_repaint()` is `request_repaint_of(self.viewport_id())`, and
  `viewport_id()` is the top of `viewport_stack` (`egui context.rs:1821-1823, 641-643`). Called from a
  tokio, relay or capture thread while the UI thread is inside the overlay viewport's pass, it requests
  the overlay, not ROOT.
- **Scenario.** A team API call finishes, or the re-sign-in completes, or auth flips to signed-out,
  just as the overlay is painting. The main window keeps showing "busy", the stale modal or no banner
  until the user next moves the mouse over it. One-off notifications are the ones that do not
  self-heal.
- **Fix.** `main_ctx.request_repaint_of(ViewportId::ROOT)` in both closures.

### 4. Low: exit can hang forever if the overlay HWND is still being looked up
- `interop/own_window.rs:41`, `interop/overlay_input.rs:55-60`, `session.rs:104-107`, `app.rs:135-139`
- **Mechanism.** `GetWindowTextW` on a window owned by this process sends `WM_GETTEXT` to the owning
  thread. That is the UI thread, for the main window, the overlay and winit's helper and IME windows.
  `on_exit` runs on the UI thread and calls `OverlayInputThread::shutdown`, which `join()`s with no
  timeout. `WaitForSingleObject` does not dispatch sent messages.
- **Scenario.** The user closes the main window during the overlay thread's first `find`. That
  happens on the first tick where Rust is in front with teammates, before the HWND is cached. The
  input thread waits for the UI thread and the UI thread waits for the join, so the process never
  exits. The window is narrow but has no backstop. Each `find` also stalls the input thread behind a
  UI frame (vsync swap).
- **Fix.** Use `InternalGetWindowText`, which sends no message. Or bound the join (signal plus
  `recv_timeout`, as `Services::shutdown` does for vision).

### 5. Low: topmost is never re-asserted after creation
- `app.rs:107-117`, `interop/overlay_input.rs:222-229`
- **Mechanism.** By design the window is never hidden, only shrunk to 1x1, so nothing re-raises it
  within the topmost band. In WPF, each Collapsed-to-Visible transition called `ShowWindow`, which
  re-raised the topmost window on every return to Rust.
- **Scenario.** Another topmost window that is activated or shown later (an always-on-top utility,
  vendor overlay windows, the taskbar after an alt-tab) stays above the panel where it overlaps,
  until the agent restarts.
- **Fix.** On each hidden-to-visible edge in `OverlayInput::show`, call
  `SetWindowPos(hwnd, HWND_TOPMOST, 0,0,0,0, NOMOVE|NOSIZE|NOACTIVATE|ASYNCWINDOWPOS)`.

### 6. Low: icon decoding runs under the overlay state lock
- `state/overlay.rs:80-90` (reconcile), `:107-113` (select), `state/inspect.rs:29-37`, `icons.rs:44-56`;
  the lock is taken by `views/overlay/mod.rs:43` and by `set_*_visible` on the input thread
- **Mechanism.** `OverlayMemberRow::of` and `InspectView::update` call `ItemIcons::load`, which reads
  and decodes a 512x512 PNG on a cache miss, while holding `OverlayModel.state`.
- **Scenario.** A newly selected teammate with a full inventory (about 50 uncached icons), or a room
  change on the relay's tokio worker, holds the lock for tens to hundreds of ms. Meanwhile the
  15 ms input thread blocks in `set_inspect_visible`, so hold-key edges are late, and the UI thread
  blocks painting the overlay, which also freezes the main window.
- **Fix.** Build `members` and the `InspectView` into locals, then lock only to swap them in. Keep
  `select` cheap by building outside the lock too.

### 7. Low: on the panic path, the process can linger without a window
- `main.rs:78-85`
- **Mechanism.** If `run_native` unwinds, `runtime.shutdown_timeout` is skipped and `Runtime` is
  dropped instead. Drop waits without limit for `spawn_blocking` work, such as Velopack's
  `download_updates` (`background/updates.rs:33`). `on_exit` (and so `Session::shutdown`) is also
  skipped.
- **Scenario.** A UI panic during an update download leaves a windowless `rustoverlay-agent.exe`
  until the download finishes or times out. A relaunch then runs alongside it.
- **Fix.** Run `run_native` under `catch_unwind`, and always call `shutdown_timeout`, then
  `resume_unwind`. Or add `Drop for Session` that calls `shutdown`.

## Checked and found sound

- **`host.rs` `DeferredFrameSource`.** `OnceLock::get_or_init` blocks concurrent callers until the
  in-flight initialiser finishes. If the probe thread's initialiser panics, the cell stays empty and a
  waiter runs `frame_source` itself. If the spawn fails, the closure is dropped and the first `read`
  probes inline. Every `DuplicationFrameSource` and `ScreenFrameSource` constructor is lazy, so
  creating them on the probe thread has no thread-affinity problem. The only UI-side reader, the
  Debug grab, already runs in `spawn_blocking` (`state/agent.rs:86`), so the UI never waits on the
  probe.
- **Velopack.** The argument order of `wait_exit_then_apply_updates(asset, silent=false, restart=true, [])`
  matches the 1.2.161 signature and C#'s defaults. Not-installed returns an `Err` from
  `UpdateManager::new`, which is handled. Check-then-6h cadence and stop-after-staging both match C#.
- **Logging.** Daily file, newest 7 kept, matches `retainedFileCountLimit: 7`. The `WorkerGuard`
  lives to the end of `main`, so lines are flushed on normal exit and on unwind.
- **Lock ordering.** Every `Notify` is called after its model guard is dropped. Room listeners are
  invoked outside `TeamRoomState`'s lock (`team_room_state.rs:125-127`). No guard is held across an
  `.await`. The loadout listener only requests a repaint.
- **Idle repaint.** There is no continuous repaint outside an active `KeyBindBox` capture. Loadout
  notifications follow the vision cadence (1 Hz at rest, 10 Hz for 2 s after the inventory key).
- **Minimised main window.** eframe still runs the root pass while the overlay descendant is visible
  (`glow_integration.rs:1584-1599`), so the overlay keeps being registered.
- **Win32 placement.** `move_to` uses physical pixels with `SWP_ASYNCWINDOWPOS`, matching C#.
  PerMonitorV2 is embedded via `build.rs` and `app.manifest`. winit's `WM_DPICHANGED` keeps the
  suggested top-left, so the anchor holds across monitors.
- **Hotkeys.** Registration, pumping and unregistration all happen on the overlay-input thread
  (`HotkeyBinder` is built inside the spawned closure and dropped there).
- **Settings.** Layering (embedded shipped JSON, then the file beside the exe, then the user's
  `settings.json`, then `RUSTOVERLAY_`) and the Serilog level mapping match `AgentHost`. A malformed
  user file is fatal at start, as C#'s `AddJsonFile` was.
- **Project rules.** Sign-in gate on start (`app.rs:52-58`). Own inventory and grab frame only on the
  Debug tab (`views/debug_tab.rs`). `LoadoutState` keeps the last open inventory across close
  (`loadout_state.rs:83-88`), and nothing resets it.
