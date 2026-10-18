use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Map, Value};

use super::OverlayOptions;

type Listener = Arc<dyn Fn() + Send + Sync>;

/// The live overlay settings: what the settings screen edits and the overlay reads.
///
/// Seeded from configuration, so the file layering still decides what the player starts with.
/// Only what they change afterwards is written back, and only the `Overlay` node of the file: the
/// rest of settings.json is theirs and survives untouched.
pub struct OverlaySettingsStore {
    path: PathBuf,
    current: Mutex<OverlayOptions>,
    listeners: Mutex<Vec<Listener>>,
}

impl OverlaySettingsStore {
    pub fn new(directory: impl AsRef<Path>, seed: OverlayOptions) -> Self {
        Self {
            path: directory.as_ref().join("settings.json"),
            current: Mutex::new(seed),
            listeners: Mutex::default(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The settings in force. Read it per use rather than holding on to it: a reset replaces them.
    pub fn current(&self) -> OverlayOptions {
        self.current.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Raised after a change has been applied and saved.
    pub fn on_changed(&self, listener: impl Fn() + Send + Sync + 'static) {
        self.listeners.lock().unwrap_or_else(PoisonError::into_inner).push(Arc::new(listener));
    }

    /// Applies an edit, persists it, and tells everyone drawing from it.
    /// The edit runs under the store's lock: it must not call back into the store.
    pub fn update(&self, edit: impl FnOnce(&mut OverlayOptions)) {
        let snapshot = {
            let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
            edit(&mut current);
            current.clone()
        };
        self.commit(&snapshot);
    }

    /// Back to the values the agent ships with, not to whatever launched this session.
    pub fn reset(&self) {
        let defaults = OverlayOptions::default();
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = defaults.clone();
        self.commit(&defaults);
    }

    fn commit(&self, options: &OverlayOptions) {
        self.save(options);

        let listeners = self.listeners.lock().unwrap_or_else(PoisonError::into_inner).clone();
        for listener in listeners {
            listener();
        }
    }

    fn save(&self, options: &OverlayOptions) {
        if let Err(e) = self.try_save(options) {
            // The change is already live; losing it at exit beats taking the overlay down with us.
            tracing::error!(error = %e, path = %self.path.display(), "Could not write overlay settings");
        }
    }

    fn try_save(&self, options: &OverlayOptions) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut root = self.read()?.unwrap_or_default();
        root.insert(OverlayOptions::SECTION.to_owned(), serde_json::to_value(options).map_err(io::Error::other)?);

        let text = serde_json::to_string_pretty(&Value::Object(root)).map_err(io::Error::other)?;
        write_atomically(&self.path, text.as_bytes())
    }

    fn read(&self) -> io::Result<Option<Map<String, Value>>> {
        if !self.path.is_file() {
            return Ok(None);
        }

        let bytes = std::fs::read(&self.path)?;

        // The same dialect startup reads, or a comment or trailing comma would wipe the other sections.
        match serde_json::from_slice::<Value>(&super::json_text::relax(&bytes)) {
            Ok(Value::Object(root)) => Ok(Some(root)),
            Ok(_) => Ok(None),
            Err(e) => {
                // Hand-edited into invalid JSON. Configuration already failed to load it, so
                // replacing it loses nothing that was in effect.
                tracing::warn!(error = %e, path = %self.path.display(), "Overlay settings are not valid JSON; replacing them");
                Ok(None)
            }
        }
    }
}

/// A torn settings.json stops the next launch, so write beside it and rename over it.
fn write_atomically(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);

    let written = (|| {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}
