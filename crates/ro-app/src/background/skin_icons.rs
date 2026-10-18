//! Supplies the capture loop with approved skins' icons, read from Rust's own asset bundles —
//! nothing is downloaded.
//!
//! Only items the capture loop asks for are extracted: loading every approved skin would triple
//! the faces the matcher scores per slot. Items asked for are remembered and extracted again at the
//! next start, so a skin seen once is recognised from the first frame after.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use ro_capture::{ArrivedIcons, GameSkinIcons, SkinIconSource};
use ro_core::install::steam_root;
use tokio::runtime::Handle;

use crate::host::Host;

const CACHE_DIRECTORY: &str = "game-skin-icons";
/// Items the capture loop has asked for, so the next start loads their sets at once.
const WANTED_FILE: &str = "wanted.json";

pub struct SkinIcons {
    runtime: Handle,
    directory: PathBuf,
    wanted_file: PathBuf,
    game: Arc<OnceLock<GameSkinIcons>>,
    wanted: Mutex<BTreeSet<String>>,
    arrived: Arc<Mutex<ArrivedIcons>>,
}

impl SkinIcons {
    /// Reads Steam's item schema and indexes the game's skin bundles off the caller's thread, then
    /// offers the sets of items wanted before.
    pub fn start(host: &Host, runtime: &Handle) -> Arc<Self> {
        let directory = host.data_directory.join(CACHE_DIRECTORY);
        let wanted_file = directory.join(WANTED_FILE);
        let wanted = std::fs::read(&wanted_file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let icons = Arc::new(Self {
            runtime: runtime.clone(),
            directory,
            wanted_file,
            game: Arc::new(OnceLock::new()),
            wanted: Mutex::new(wanted),
            arrived: Arc::default(),
        });

        let starting = Arc::clone(&icons);
        let locator = host.locator.clone();
        runtime.spawn_blocking(move || {
            let (Some(steam), Some(install)) = (steam_root(), locator.locate()) else { return };
            let Some(game) = GameSkinIcons::load(&steam, &install) else { return };
            let game = starting.game.get_or_init(|| game);
            let wanted: Vec<String> = starting.wanted.lock().unwrap_or_else(PoisonError::into_inner).iter().cloned().collect();
            for item in wanted {
                starting.offer(item.clone(), game.extract(&item, &starting.directory));
            }
        });

        icons
    }

    fn offer(&self, item: String, paths: Vec<PathBuf>) {
        if !paths.is_empty() {
            self.arrived.lock().unwrap_or_else(PoisonError::into_inner).push((item, paths));
        }
    }

    /// False when the item was already wanted, whose set has been offered then.
    fn remember(&self, item: &str) -> bool {
        let mut wanted = self.wanted.lock().unwrap_or_else(PoisonError::into_inner);
        if !wanted.insert(item.to_owned()) {
            return false;
        }
        let written = serde_json::to_vec(&*wanted)
            .map_err(std::io::Error::other)
            .and_then(|json| std::fs::create_dir_all(&self.directory).and_then(|()| std::fs::write(&self.wanted_file, json)));
        if let Err(error) = written {
            tracing::debug!(%error, "Could not record a wanted skin set");
        }
        true
    }
}

impl SkinIconSource for SkinIcons {
    fn has_skins(&self, item_short_name: &str) -> bool {
        self.game.get().is_some_and(|g| g.has_skins(item_short_name))
    }

    fn request(&self, item_short_name: &str) {
        let item = item_short_name.to_ascii_lowercase();
        if !self.has_skins(&item) || !self.remember(&item) {
            return;
        }

        let (game, directory, arrived) = (Arc::clone(&self.game), self.directory.clone(), Arc::clone(&self.arrived));
        self.runtime.spawn_blocking(move || {
            let Some(game) = game.get() else { return };
            let paths = game.extract(&item, &directory);
            tracing::info!(item, icons = paths.len(), "Read approved skins' icons from the game's bundles");
            if !paths.is_empty() {
                arrived.lock().unwrap_or_else(PoisonError::into_inner).push((item, paths));
            }
        });
    }

    fn take_arrived(&self) -> ArrivedIcons {
        std::mem::take(&mut *self.arrived.lock().unwrap_or_else(PoisonError::into_inner))
    }
}
