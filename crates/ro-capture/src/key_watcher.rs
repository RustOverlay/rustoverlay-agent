use ro_core::input::key_names;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

/// Watches one key for a press, so a panel that opens and shuts between polls is not missed.
///
/// Queries the key's state; it does not install a hook. A `WH_KEYBOARD_LL` hook would see every
/// keystroke on the machine, which is both a privacy problem for a tool that shares data and
/// exactly the shape anti-cheat heuristics look for. Passive throughout: no injection, no hooks, no
/// reads of the game process.
#[derive(Debug)]
pub struct KeyWatcher {
    key: i32,
    down: bool,
}

impl KeyWatcher {
    pub fn new(key_name: &str) -> Self {
        Self { key: virtual_key(key_name), down: false }
    }

    /// False when the key name is one this does not know, in which case nothing is watched.
    pub fn is_watchable(&self) -> bool {
        self.key != 0
    }

    /// True once per press, on the transition from up to down. Edge-triggered: holding the key
    /// would otherwise report a press on every poll and pin the capture at its boosted rate.
    pub fn was_pressed(&mut self) -> bool {
        if self.key == 0 {
            return false;
        }

        let down = physically_down(self.key);
        self.latch(down)
    }

    /// Whether the key is down right now, for a hold rather than a press. Leaves the edge latch
    /// alone, so one watcher can serve both without either eating the other's transitions.
    pub fn is_down(&self) -> bool {
        self.key != 0 && physically_down(self.key)
    }

    fn latch(&mut self, down: bool) -> bool {
        let pressed = down && !self.down;
        self.down = down;
        pressed
    }
}

/// Maps a Rust key name to a Windows virtual key code, or 0 when unrecognised.
pub fn virtual_key(name: &str) -> i32 {
    key_names::code_of(name)
}

/// Gotcha: only the high bit (current physical state) is read. The low bit — "pressed since last
/// call" — is consumed by whichever process reads it first.
fn physically_down(key: i32) -> bool {
    // SAFETY: no preconditions.
    (unsafe { GetAsyncKeyState(key) } as u16 & 0x8000) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_is_reported_once_however_long_it_is_held() {
        let mut watcher = KeyWatcher { key: 9, down: false };

        assert!(watcher.latch(true));
        assert!(!watcher.latch(true));
        assert!(!watcher.latch(false));
        assert!(watcher.latch(true));
    }

    #[test]
    fn an_unknown_key_is_not_watched() {
        let mut watcher = KeyWatcher::new("no-such-key");

        assert!(!watcher.is_watchable());
        assert!(!watcher.was_pressed());
        assert!(!watcher.is_down());
    }

    #[test]
    fn a_known_key_is_watched() {
        assert!(KeyWatcher::new("tab").is_watchable());
    }
}
