//! Settings tab (SettingsTab.xaml). Changes apply and save as they are made; see `settings_form`.

use eframe::egui::{self, RichText, Ui};
use ro_core::hosting::OverlaySettingsStore;

use super::key_bind_box::KeyBindBox;
use crate::format::fixed;
use crate::session::Session;
use crate::state::settings_form::{self as form, Bind};
use crate::ui::widgets::{caption, card, section_title};

const LABEL_WIDTH: f32 = 150.0;

#[derive(Default)]
pub struct SettingsTab {
    binds: [KeyBindBox; 3],
    offset_x: Option<String>,
    offset_y: Option<String>,
}

impl SettingsTab {
    pub fn show(&mut self, ui: &mut Ui, session: &Session) {
        let store = &session.host.overlay_settings;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            section_title(ui, "BINDS");
            card(ui, |ui| {
                for (field, (label, bind)) in self.binds.iter_mut().zip([
                    ("Show and hide", Bind::Toggle),
                    ("Hold to inspect", Bind::Hold),
                    ("Next teammate", Bind::Cycle),
                ]) {
                    row(ui, label, |ui| {
                        let current = bind.get(&store.current()).to_owned();
                        if let Some(key) = field.show(ui, &current) {
                            form::set_bind(store, bind, key);
                        }
                    });
                }
                ui.add_space(4.0);
                caption(
                    ui,
                    "These keys are watched, never intercepted, so each one also keeps doing whatever Rust has it bound to. \
                     Pick keys the game leaves alone, or accept that inspecting a teammate does both things at once.",
                );
            });
            ui.add_space(16.0);

            section_title(ui, "OVERLAY");
            card(ui, |ui| {
                let options = store.current();
                row(ui, "Size", |ui| {
                    if let Some(v) = percent_slider(ui, form::scale_percent(&options)) {
                        form::set_scale_percent(store, v);
                    }
                });
                row(ui, "Inventory size", |ui| {
                    if let Some(v) = percent_slider(ui, form::inventory_scale_percent(&options)) {
                        form::set_inventory_scale_percent(store, v);
                    }
                    caption(ui, "of the size above");
                });
                row(ui, "Position", |ui| {
                    if let Some(x) = int_field(ui, &mut self.offset_x, options.offset_x) {
                        form::set_offset_x(store, x);
                    }
                    if let Some(y) = int_field(ui, &mut self.offset_y, options.offset_y) {
                        form::set_offset_y(store, y);
                    }
                    ui.add_space(4.0);
                    caption(ui, "pixels from the top-left of the game window");
                });

                ui.add_space(8.0);
                if ui.button("Reset to defaults").clicked() {
                    reset(store, self);
                }
            });

            ui.add_space(12.0);
            caption(ui, "Changes apply and save as you make them, to settings.json in your local app data.");
        });
    }
}

fn reset(store: &OverlaySettingsStore, tab: &mut SettingsTab) {
    store.reset();
    tab.offset_x = None;
    tab.offset_y = None;
}

fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized([LABEL_WIDTH, ui.spacing().interact_size.y], egui::Label::new(label).halign(egui::Align::Min));
        add(ui);
    });
    ui.add_space(8.0);
}

/// 50-200 in steps of 5, read out beside it.
fn percent_slider(ui: &mut Ui, percent: f64) -> Option<f64> {
    let mut value = percent;
    ui.spacing_mut().slider_width = 240.0;
    let changed = ui.add(egui::Slider::new(&mut value, 50.0..=200.0).step_by(5.0).show_value(false)).changed();
    ui.add_space(4.0);
    ui.add_sized([50.0, 20.0], egui::Label::new(RichText::new(format!("{}%", fixed(percent, 0))).monospace()));
    changed.then_some(value)
}

/// A whole-number box that commits when focus leaves it. Text that is not a number is dropped.
fn int_field(ui: &mut Ui, buffer: &mut Option<String>, current: i32) -> Option<i32> {
    let text = buffer.get_or_insert_with(|| current.to_string());
    let response = ui.add(egui::TextEdit::singleline(text).desired_width(60.0));

    if response.lost_focus() {
        let parsed = text.trim().parse::<i32>().ok();
        *buffer = None;
        return parsed;
    }
    if !response.has_focus() {
        *buffer = None;
    }
    None
}
