//! The overlay's keys, visibility and placement (`OverlayWindow`'s timer half).
//!
//! Shown only while Rust is the foreground window and the player has it toggled on. The foreground
//! half is not a preference: a panel carrying teammates' health has no business drawn over a
//! browser, and less business being visible while a stream or a screen share is running.
//!
//! Runs on a thread of its own rather than in the overlay's paint callback: the overlay repaints
//! only when something changes, while keys must be sampled every tick or a tap between two polls is
//! lost. The same thread pumps `WM_HOTKEY`, since `HotkeyBinder` registers against its queue.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use ro_capture::{KeyWatcher, rust_window};
use ro_core::capture::ScreenRect;
use ro_core::hosting::{OverlayOptions, OverlaySettingsStore};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, MSG, PM_REMOVE, PeekMessageW, WM_HOTKEY};

use super::hotkey_binder::{BARE, HotkeyBinder, modifier_for};
use super::tap_key::TapKey;
use super::{own_window, window_styles};
use crate::state::Notify;
use crate::state::overlay::OverlayModel;

/// The overlay viewport's title, which is how its window is found.
pub const OVERLAY_TITLE: &str = "RustOverlay Overlay";

/// How long exit waits for the thread; it is at most a tick away from noticing the stop flag.
const STOP_TIMEOUT: Duration = Duration::from_secs(1);

/// Where the hidden panel waits: off every display, as Windows parks minimised windows.
const PARKED: (i32, i32) = (-32000, -32000);

pub struct OverlayInputThread {
    stop: Arc<AtomicBool>,
    done: Option<Receiver<()>>,
}

impl OverlayInputThread {
    pub fn spawn(settings: Arc<OverlaySettingsStore>, model: Arc<OverlayModel>, repaint: Notify) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        // A change made before the thread runs needs nothing done: it binds against whatever the
        // settings say by the time it starts.
        let rebind = Arc::new(AtomicBool::new(true));
        let flag = rebind.clone();
        settings.on_changed(move || flag.store(true, Ordering::Relaxed));

        let stopping = stop.clone();
        let (finished, done) = mpsc::channel();
        std::thread::Builder::new()
            .name("overlay-input".into())
            .spawn(move || {
                OverlayInput::new(settings, model, repaint).run(&stopping, &rebind);
                let _ = finished.send(());
            })
            .expect("spawning the overlay input thread");

        Self { stop, done: Some(done) }
    }

    /// Bounded, not a join: this runs on the UI thread, and the input thread can be inside a style
    /// change that waits on the UI thread.
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(done) = self.done.take()
            && let Err(RecvTimeoutError::Timeout) = done.recv_timeout(STOP_TIMEOUT)
        {
            tracing::warn!("The overlay input thread did not stop in time; leaving it to process exit");
        }
    }
}

struct OverlayInput {
    settings: Arc<OverlaySettingsStore>,
    model: Arc<OverlayModel>,
    repaint: Notify,
    hotkeys: HotkeyBinder,
    toggle: Option<TapKey>,
    cycle: Option<TapKey>,
    select: Vec<TapKey>,
    // Sampled rather than registered, because a hold needs the key's release as well and a
    // registered key is only ever announced on the way down.
    hold: KeyWatcher,
    warned: HashSet<String>,
    enabled: bool,
    anchor: Option<ScreenRect>,
    window: Option<HWND>,
}

impl OverlayInput {
    fn new(settings: Arc<OverlaySettingsStore>, model: Arc<OverlayModel>, repaint: Notify) -> Self {
        let enabled = settings.current().enabled_at_launch;
        Self {
            settings,
            model,
            repaint,
            hotkeys: HotkeyBinder::new(),
            toggle: None,
            cycle: None,
            select: Vec::new(),
            hold: KeyWatcher::new(""),
            warned: HashSet::new(),
            enabled,
            anchor: None,
            window: None,
        }
    }

    fn run(mut self, stop: &AtomicBool, rebind: &AtomicBool) {
        let mut interval = Duration::ZERO;

        while !stop.load(Ordering::Relaxed) {
            if rebind.swap(false, Ordering::Relaxed) {
                let options = self.settings.current();
                self.rebind(&options);
                // Offsets are only applied when the anchor moves, so forget where we thought we were.
                self.anchor = None;
                interval = options.poll_interval.to_std().unwrap_or_default();
            }

            self.pump_hotkeys();
            self.poll();
            std::thread::sleep(interval);
        }
    }

    /// Rebuilds the watchers against the current binds. Cheap: a watcher is a virtual key code and
    /// a latch, and dropping the old ones loses only an in-flight press of a key no longer bound.
    /// Registration is not restored here; it is claimed on the next tick that finds Rust in front.
    fn rebind(&mut self, options: &OverlayOptions) {
        self.hotkeys.clear();
        self.hold = self.watch(&options.inspect.hold_key, "the inventory cannot be opened");

        // The inspector's keys act only while the hold key is down, so they are claimed under its
        // modifier alone (Windows matches modifiers exactly): claimed bare too, `C` never reached
        // chat. The toggle is bare only: claimed under Alt it would eat Alt+F4 desktop-wide.
        let while_held = [modifier_for(&options.inspect.hold_key)];

        self.toggle = Some(self.tap(&options.toggle_key, "the panel cannot be toggled", &[BARE]));
        self.cycle = Some(self.tap(&options.inspect.cycle_key, "teammates cannot be cycled", &while_held));
        self.select = options
            .inspect
            .select_keys
            .iter()
            .map(|key| self.tap(key, "that row cannot be selected", &while_held))
            .collect();
    }

    fn tap(&mut self, key: &str, consequence: &str, modifiers: &[u32]) -> TapKey {
        let handle = self.hotkeys.add(key, modifiers);
        TapKey::new(handle, self.watch(key, consequence))
    }

    fn watch(&mut self, key: &str, consequence: &str) -> KeyWatcher {
        let watcher = KeyWatcher::new(key);

        // Only reachable from a hand-edited settings file — the settings screen refuses to bind
        // anything the key table does not know. Once per name, so a bad bind does not fill the log.
        if !watcher.is_watchable() && self.warned.insert(key.to_owned()) {
            tracing::warn!("Overlay key '{key}' is not one we recognise; {consequence}");
        }
        watcher
    }

    fn pump_hotkeys(&mut self) {
        let mut message = MSG::default();
        // SAFETY: a valid out-pointer; drains this thread's own queue.
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            if message.message == WM_HOTKEY {
                self.hotkeys.handle(message.wParam.0 as i32);
            }
        }
    }

    fn poll(&mut self) {
        // Every tick, not only while wanted: an unstyled overlay is an alt-tab entry from the moment egui creates it.
        self.find_and_style_window();
        let foreground = rust_window::is_foreground();

        // Claimed only while Rust is in front: a registered key is swallowed everywhere, and the
        // overlay has no business taking C away from the rest of the desktop.
        self.hotkeys.set_active(foreground);

        if self.toggle.as_mut().is_some_and(|t| t.was_pressed(&mut self.hotkeys)) {
            self.enabled = !self.enabled;
        }

        // An empty panel is a rectangle of nothing over the game, so absence of teammates hides it
        // as surely as alt-tabbing does.
        let wanted = self.enabled && self.model.has_members() && foreground;

        self.poll_inspect(wanted);
        if wanted {
            self.anchor_to_rust();
        }
        self.show(wanted);
    }

    /// Every watcher is read on every tick regardless of whether the inspector is open, or an edge
    /// that happens between two polls is lost and the next press reads as no change at all.
    fn poll_inspect(&mut self, panel_visible: bool) {
        let holding = self.hold.is_down();
        let cycled = self.cycle.as_mut().is_some_and(|c| c.was_pressed(&mut self.hotkeys));

        for i in 0..self.select.len() {
            if self.select[i].was_pressed(&mut self.hotkeys) && holding {
                self.model.select(i);
            }
        }

        if holding && cycled {
            self.model.cycle_selection();
        }

        if self.model.set_inspect_visible(panel_visible && holding) {
            (self.repaint)();
        }
    }

    /// Pins the panel to Rust's client area rather than the screen, so it stays put across
    /// borderless, a non-native resolution, and a move to another display.
    fn anchor_to_rust(&mut self) {
        let Some(bounds) = rust_window::client_bounds() else { return };
        if self.anchor == Some(bounds) {
            return;
        }
        let Some(window) = self.window else { return };

        let options = self.settings.current();
        self.anchor = Some(bounds);
        window_styles::move_to(window, bounds.x + options.offset_x, bounds.y + options.offset_y);
    }

    /// Gotcha: not `ShowWindow`. winit re-applies its own idea of the window's visibility, with
    /// `SW_HIDE` or an activating `SW_SHOW`, whenever any of its window flags change, so a hidden
    /// panel is one that draws nothing and is parked off-screen instead. That also means nothing
    /// re-raises it, so each return to visible re-asserts topmost as WPF's `ShowWindow` did.
    fn show(&mut self, visible: bool) {
        if self.model.set_panel_visible(visible) {
            match (visible, self.window) {
                (true, Some(window)) => window_styles::raise_topmost(window),
                (false, Some(_)) => self.park(),
                _ => {}
            }
            (self.repaint)();
        }
    }

    /// Moves the hidden panel off every display: left in place, its last frame or a one-pixel window
    /// showed as a stray white dot. The next anchor brings it back.
    fn park(&mut self) {
        if let Some(window) = self.window {
            window_styles::move_to(window, PARKED.0, PARKED.1);
            self.anchor = None;
        }
    }

    /// Finds the overlay viewport's window, which stays None until egui has created it, and
    /// re-asserts its styles.
    fn find_and_style_window(&mut self) {
        // SAFETY: `IsWindow` accepts any handle.
        if self.window.is_some_and(|w| !unsafe { IsWindow(Some(w)) }.as_bool()) {
            self.window = None;
            self.anchor = None;
        }
        if self.window.is_none() {
            self.window = own_window::find(OVERLAY_TITLE);
            // Born hidden, so park it now or the pixel shows until the first show.
            if !self.model.state().panel_visible {
                self.park();
            }
        }
        if let Some(window) = self.window {
            window_styles::strip_frame(window);
            window_styles::make_click_through(window);
        }
    }
}
