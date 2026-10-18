use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use eframe::egui::{self, TextureHandle, TextureOptions};

use crate::icons::Icon;

/// One GPU texture per decoded icon, uploaded the first time a view draws it and kept.
#[derive(Default)]
pub struct IconTextures {
    uploaded: Mutex<HashMap<String, TextureHandle>>,
}

impl IconTextures {
    pub fn get(&self, ctx: &egui::Context, icon: &Icon) -> TextureHandle {
        self.uploaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(icon.key.clone())
            .or_insert_with(|| ctx.load_texture(&icon.key, (*icon.image).clone(), TextureOptions::LINEAR))
            .clone()
    }
}
