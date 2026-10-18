//! Keys the operating system watches for us, for taps that sampling drops.
//!
//! `GetAsyncKeyState` answers whether a key is down at the instant it is asked, so a tap that begins
//! and ends between two polls is never seen at all. Windows timers fire on a ~15.6ms tick, so no
//! poll rate closes the hole completely. A registered key arrives as a message instead: the press
//! is latched here whenever it happens and drained on the next tick, so a late tick delays the
//! response rather than losing it.
//!
//! The cost is that a registered key is swallowed system-wide, which is why [`HotkeyBinder::set_active`]
//! exists and registration follows Rust's foreground state. Left registered, a cycle key of C would
//! stop C reaching every other application on the machine.
//!
//! Not a hook. Nothing is installed into another process, no other key is observed, and the
//! operating system decides what reaches us.
//!
//! Gotcha: keys are registered against the calling thread's message queue (no window), so every
//! method must be called on the one thread that pumps `WM_HOTKEY` into [`HotkeyBinder::handle`].

use std::collections::HashMap;

use ro_core::input::key_names;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
};

/// No modifier at all: the key pressed on its own.
pub const BARE: u32 = 0;

struct Binding {
    name: String,
    code: i32,
    /// Every modifier state the key may be pressed in; see [`modifier_for`].
    modifiers: Vec<u32>,
    ids: Vec<i32>,
    registered: bool,
    pressed: bool,
    complained: bool,
}

#[derive(Default)]
pub struct HotkeyBinder {
    bindings: Vec<Binding>,
    by_hotkey_id: HashMap<i32, usize>,
    next_id: i32,
    active: bool,
}

/// The modifier a key acts as while it is held down, or [`BARE`] for an ordinary key.
///
/// Load-bearing for the inspector. Its cycle key is only ever pressed while the hold key is down,
/// and Windows matches a hotkey's modifiers exactly — with Alt held, a registration for bare C does
/// not fire, only one for Alt+C does. Measured, not assumed.
pub fn modifier_for(key_name: &str) -> u32 {
    match key_names::code_of(key_name) {
        0xA0 | 0xA1 => MOD_SHIFT.0,
        0xA2 | 0xA3 => MOD_CONTROL.0,
        0xA4 | 0xA5 => MOD_ALT.0,
        _ => BARE,
    }
}

impl HotkeyBinder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a key under every modifier combination it may be pressed with, and returns the handle
    /// used to ask about it. Registration comes later.
    pub fn add(&mut self, key_name: &str, modifiers: &[u32]) -> usize {
        let mut distinct = Vec::new();
        for &m in modifiers {
            if !distinct.contains(&m) {
                distinct.push(m);
            }
        }

        self.bindings.push(Binding {
            name: key_name.to_owned(),
            code: key_names::code_of(key_name),
            modifiers: distinct,
            ids: Vec::new(),
            registered: false,
            pressed: false,
            complained: false,
        });
        self.bindings.len() - 1
    }

    /// Forgets every binding, releasing any the operating system is holding.
    pub fn clear(&mut self) {
        self.set_active(false);
        self.bindings.clear();
    }

    /// Whether the operating system is delivering this key, rather than it needing to be sampled.
    pub fn is_watched(&self, handle: usize) -> bool {
        self.bindings[handle].registered
    }

    /// True once per press, clearing the latch — the same contract as polling for one.
    pub fn take(&mut self, handle: usize) -> bool {
        std::mem::take(&mut self.bindings[handle].pressed)
    }

    /// Claims or releases every key, following whether Rust is the window in front.
    pub fn set_active(&mut self, active: bool) {
        if active == self.active {
            return;
        }
        self.active = active;

        for index in 0..self.bindings.len() {
            if active {
                self.claim(index);
            } else {
                self.release(index);
            }
        }
    }

    /// Latches a press. Returns false for a message that was not one of ours.
    pub fn handle(&mut self, hotkey_id: i32) -> bool {
        let Some(&index) = self.by_hotkey_id.get(&hotkey_id) else { return false };
        self.bindings[index].pressed = true;
        true
    }

    /// Claims every combination the key may arrive as, or none of them.
    ///
    /// All or nothing because a partial claim is worse than no claim: the key would look watched,
    /// the sampled path would be discarded, and whichever combination failed to register would go
    /// dead rather than merely miss the occasional press.
    fn claim(&mut self, index: usize) {
        if self.bindings[index].code == 0 {
            return;
        }

        for modifiers in self.bindings[index].modifiers.clone() {
            let id = self.next_id;
            self.next_id += 1;

            let flags = HOT_KEY_MODIFIERS(modifiers) | MOD_NOREPEAT;
            // SAFETY: no window; the message is posted to this thread's queue.
            if unsafe { RegisterHotKey(None, id, flags, self.bindings[index].code as u32) }.is_err() {
                self.release(index);
                self.complain(index);
                return;
            }

            self.by_hotkey_id.insert(id, index);
            self.bindings[index].ids.push(id);
        }

        self.bindings[index].registered = true;
    }

    fn release(&mut self, index: usize) {
        for id in std::mem::take(&mut self.bindings[index].ids) {
            // SAFETY: unregistering an id this thread registered; failure means it is already gone.
            let _ = unsafe { UnregisterHotKey(None, id) };
            self.by_hotkey_id.remove(&id);
        }

        let binding = &mut self.bindings[index];
        binding.registered = false;
        // A press that arrived as we were letting go belongs to the session that just ended.
        binding.pressed = false;
    }

    /// Says so once per key, rather than on every alt-tab back into the game.
    fn complain(&mut self, index: usize) {
        let binding = &mut self.bindings[index];
        if !std::mem::replace(&mut binding.complained, true) {
            tracing::warn!(
                "Another application already owns '{}'; falling back to polling it, which drops the occasional quick tap",
                binding.name
            );
        }
    }
}

impl Drop for HotkeyBinder {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_modifier_keys_map_to_their_hotkey_modifier() {
        assert_eq!(modifier_for("leftalt"), MOD_ALT.0);
        assert_eq!(modifier_for("rightcontrol"), MOD_CONTROL.0);
        assert_eq!(modifier_for("leftshift"), MOD_SHIFT.0);
        assert_eq!(modifier_for("c"), BARE);
        assert_eq!(modifier_for("nonsense"), BARE);
    }

    #[test]
    fn an_unknown_key_is_never_claimed_and_stays_sampled() {
        let mut binder = HotkeyBinder::new();
        let handle = binder.add("nonsense", &[BARE, BARE]);

        binder.set_active(true);

        assert!(!binder.is_watched(handle));
        assert!(!binder.take(handle));
        assert_eq!(binder.bindings[handle].modifiers, [BARE]);
    }

    #[test]
    fn a_handled_press_is_taken_once_and_foreign_ids_are_ignored() {
        let mut binder = HotkeyBinder::new();
        let handle = binder.add("f9", &[BARE]);
        binder.by_hotkey_id.insert(41, handle);

        assert!(!binder.handle(40));
        assert!(binder.handle(41));
        assert!(binder.take(handle));
        assert!(!binder.take(handle));
    }
}
