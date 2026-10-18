use ro_capture::KeyWatcher;

use super::hotkey_binder::HotkeyBinder;

/// A key watched for taps, by the operating system where it can be and by sampling where it cannot.
///
/// Both paths answer the same question — was this key pressed since you last asked — so the caller
/// polls it either way and never learns which is in use. See `HotkeyBinder` for why the registered
/// path exists and why it is not always available.
pub struct TapKey {
    handle: usize,
    fallback: KeyWatcher,
}

impl TapKey {
    pub fn new(handle: usize, fallback: KeyWatcher) -> Self {
        Self { handle, fallback }
    }

    pub fn was_pressed(&mut self, binder: &mut HotkeyBinder) -> bool {
        // The fallback is read even while the key is registered, or its edge latch would go stale
        // and report a press the moment Rust loses focus and sampling takes over again.
        let sampled = self.fallback.was_pressed();

        if binder.is_watched(self.handle) { binder.take(self.handle) } else { sampled }
    }
}
