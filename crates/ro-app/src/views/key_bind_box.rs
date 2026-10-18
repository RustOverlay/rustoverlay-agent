//! A bind field (KeyBindBox.xaml): click it, press a key, the key is stored.
//!
//! Escape cancels rather than binding. Anything outside the key table is ignored and capture
//! continues, so a bind that would leave the watcher disabled cannot be made from here at all.

use eframe::egui::{self, RichText, Ui};
use ro_core::input::key_names;

use crate::interop::key_capture::{Captured, KeyCapture};

#[derive(Default)]
pub struct KeyBindBox {
    capture: Option<KeyCapture>,
}

impl KeyBindBox {
    /// Draws the field; returns the newly captured key name, if one was pressed this frame.
    pub fn show(&mut self, ui: &mut Ui, key_name: &str) -> Option<&'static str> {
        let text = if self.capture.is_some() { "press a key…".to_owned() } else { key_names::label(key_name) };
        let response = ui.add_sized([130.0, ui.spacing().interact_size.y], egui::Button::new(RichText::new(text).monospace()));

        if response.clicked() {
            self.capture = Some(KeyCapture::begin());
            response.request_focus();
            ui.ctx().request_repaint();
            return None;
        }

        let capture = self.capture.as_mut()?;

        // Swallowed either way: without this, Tab moves focus and arrows walk the form instead of
        // reaching the bind.
        let filter = egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true };
        ui.memory_mut(|m| m.set_focus_lock_filter(response.id, filter));

        let focused = ui.ctx().input(|i| i.focused) && response.has_focus();
        if !focused || response.clicked_elsewhere() {
            self.capture = None;
            return None;
        }

        // Keys are sampled, not delivered as events, so keep frames coming while waiting for one.
        ui.ctx().request_repaint();

        match capture.poll()? {
            Captured::Cancelled => {
                self.capture = None;
                None
            }
            Captured::Key(name) => {
                self.capture = None;
                Some(name)
            }
        }
    }
}
