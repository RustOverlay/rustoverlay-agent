use std::path::{Component, Path, PathBuf};

/// The Rust client install and the files the agent reads from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustInstall {
    root: PathBuf,
}

impl RustInstall {
    pub const STEAM_APP_ID: &str = "252490";

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn log_path(&self) -> PathBuf {
        self.root.join("output_log.txt")
    }

    pub fn client_config_path(&self) -> PathBuf {
        self.root.join("cfg").join("client.cfg")
    }

    pub fn keys_config_path(&self) -> PathBuf {
        self.root.join("cfg").join("keys.cfg")
    }

    pub fn maps_directory(&self) -> PathBuf {
        self.root.join("maps")
    }

    /// Item definitions and 512x512 ARGB icons, one pair per item.
    pub fn items_directory(&self) -> PathBuf {
        self.root.join("Bundles").join("items")
    }

    /// Steam's workshop cache for Rust: one directory per skin, holding the icon the game draws.
    ///
    /// Outside the install — `steamapps/common/Rust` and `steamapps/workshop` are siblings — but
    /// reached from it rather than located separately, since a second locator could disagree with
    /// the first about which library the game lives in. Steam fills it on demand as the player
    /// meets skins, so it is never complete. See `SkinCatalog`.
    pub fn skins_directory(&self) -> PathBuf {
        full_path(
            &self.root.join("..").join("..").join("workshop").join("content").join(Self::STEAM_APP_ID),
        )
    }

    pub fn has_item_bundles(&self) -> bool {
        self.items_directory().is_dir()
    }

    pub fn has_skins(&self) -> bool {
        self.skins_directory().is_dir()
    }

    /// False when the user has `-nolog` in their Steam launch options.
    pub fn has_log(&self) -> bool {
        self.log_path().is_file()
    }

    pub fn app_manifest_name() -> String {
        format!("appmanifest_{}.acf", Self::STEAM_APP_ID)
    }
}

/// `Path.GetFullPath`: absolute, with `.` and `..` resolved lexically.
pub(crate) fn full_path(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut out = PathBuf::new();

    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !matches!(out.components().next_back(), Some(Component::RootDir | Component::Prefix(_)) | None) {
                    out.pop();
                }
            }
            other => out.push(other),
        }
    }

    out
}
