//! Reads the next bindable key for a bind field, by virtual key code.
//!
//! egui's key events fold left and right modifiers together and never report a modifier on its own,
//! but Left Alt is the default inspector bind. So capture samples the physical keys instead, across
//! exactly the table the watcher resolves against — a key outside it cannot be captured at all.

use ro_core::input::key_names;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

const ESCAPE: i32 = 0x1B;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Captured {
    /// Escape cancels rather than binding, which is the one key a field cannot be set to.
    Cancelled,
    Key(&'static str),
}

/// Remembers which keys were already down, so only a fresh press is taken.
#[derive(Clone, Debug, Default)]
pub struct KeyCapture {
    down: Vec<i32>,
}

impl KeyCapture {
    /// Starts with whatever is held right now treated as old news.
    pub fn begin() -> Self {
        Self { down: pressed_codes() }
    }

    /// The first key newly pressed since the last call, if any.
    pub fn poll(&mut self) -> Option<Captured> {
        let now = pressed_codes();
        let fresh = now.iter().copied().find(|code| !self.down.contains(code));
        self.down = now;

        match fresh? {
            ESCAPE => Some(Captured::Cancelled),
            code => key_names::name_of(code).map(Captured::Key),
        }
    }
}

fn pressed_codes() -> Vec<i32> {
    (1..=0xFE)
        .filter(|&code| key_names::name_of(code).is_some())
        // SAFETY: no preconditions. Only the high bit (down now) is read; see KeyWatcher.
        .filter(|&code| (unsafe { GetAsyncKeyState(code) } as u16 & 0x8000) != 0)
        .collect()
}
