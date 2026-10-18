//! Steam's cached definitions of Rust's approved skins: which item each repaints, and the workshop id
//! its icon is filed under in the game's own bundles.
//!
//! Steam keeps them per account at `<Steam>\userdata\<account>\inventorymsgcache\<digest>.cachedmsg`
//! as JSON, refreshed whenever Rust starts. The same directory holds that account's inventory; only
//! the file whose body opens as the item schema is read, never the inventory one.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Deserialize;

/// How the schema file's body opens; the inventory files beside it do not.
const SCHEMA_PREFIX: &[u8] = br#"[{ "appid":"252490""#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkinDefinition {
    pub defid: u32,
    pub name: String,
    /// The `assets/skins/<workshop id>/icon.png` key of the skin's icon in `Bundles\textures`.
    pub workshop_id: u64,
}

#[derive(Debug, Default)]
pub struct SkinDefinitions {
    by_item: HashMap<String, Vec<SkinDefinition>>,
}

impl SkinDefinitions {
    /// The newest schema any Steam account on this machine has cached, or None without one.
    pub fn load(steam: &Path) -> Option<Self> {
        let path = newest_schema(&steam.join("userdata"))?;
        match fs::read(&path).map(|bytes| Self::parse(&bytes)) {
            Ok(Ok(definitions)) => Some(definitions),
            Ok(Err(error)) => {
                tracing::warn!(path = %path.display(), %error, "Steam's item schema did not parse");
                None
            }
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "Steam's item schema could not be read");
                None
            }
        }
    }

    /// The leading JSON array only: Steam ends the file with a NUL after it.
    pub fn parse(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        let defs = serde_json::Deserializer::from_slice(bytes).into_iter::<Vec<Dto>>().next().unwrap_or(Ok(Vec::new()))?;
        let mut by_item: HashMap<String, Vec<SkinDefinition>> = HashMap::new();

        // Skins without a workshop id are Facepunch's own, whose icons ship as Bundles/items/*.skin.png.
        for dto in defs {
            let (Some(item), Some(Ok(workshop_id)), Ok(defid)) =
                (dto.itemshortname, dto.workshopdownload.map(|w| w.parse()), dto.itemdefid.parse())
            else {
                continue;
            };
            if item.is_empty() || workshop_id == 0 {
                continue;
            }
            by_item.entry(item.to_ascii_lowercase()).or_default().push(SkinDefinition { defid, name: dto.name, workshop_id });
        }

        Ok(Self { by_item })
    }

    /// Every approved skin of the item with `short_name`, case-insensitively.
    pub fn for_item(&self, short_name: &str) -> &[SkinDefinition] {
        self.by_item.get(&short_name.to_ascii_lowercase()).map_or(&[], Vec::as_slice)
    }

    /// Each item with approved skins, by lower-case short name.
    pub fn items(&self) -> impl Iterator<Item = (&str, &[SkinDefinition])> {
        self.by_item.iter().map(|(item, skins)| (item.as_str(), skins.as_slice()))
    }

    pub fn skin_count(&self) -> usize {
        self.by_item.values().map(Vec::len).sum()
    }
}

#[derive(Deserialize)]
struct Dto {
    #[serde(default)]
    itemdefid: String,
    #[serde(default)]
    name: String,
    itemshortname: Option<String>,
    workshopdownload: Option<String>,
}

fn newest_schema(userdata: &Path) -> Option<PathBuf> {
    let accounts = fs::read_dir(userdata).ok()?;
    let mut newest: Option<(SystemTime, PathBuf)> = None;

    for cache in accounts.flatten().map(|account| account.path().join("inventorymsgcache")) {
        let Ok(files) = fs::read_dir(&cache) else { continue };
        for path in files.flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cachedmsg"))) {
            let Ok(modified) = fs::metadata(&path).and_then(|m| m.modified()) else { continue };
            if newest.as_ref().is_none_or(|(at, _)| modified > *at) && opens_as_schema(&path) {
                newest = Some((modified, path));
            }
        }
    }

    newest.map(|(_, path)| path)
}

fn opens_as_schema(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; SCHEMA_PREFIX.len()];
    fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && head == SCHEMA_PREFIX
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[{ "appid":"252490","itemdefid":"10189","name":"Industrial Door","itemshortname":"door.hinged.metal","workshopdownload":"2306487441","icon_url":"https://files.facepunch.com/rust/icons/inventory/rust/10189_small.png?1891195872"},
        { "appid":"252490","itemdefid":"10190","name":"Some Crate","itemshortname":"","workshopdownload":"1"},
        { "appid":"252490","itemdefid":"10191","name":"Facepunch Skin","itemshortname":"smg.thompson"},
        { "appid":"252490","itemdefid":"56702","name":"Time Travel Thompson","itemshortname":"smg.thompson","workshopdownload":"3137160380"}]"#;

    #[test]
    fn skins_are_indexed_by_the_item_they_repaint_and_the_trailing_nul_is_ignored() {
        let mut bytes = SAMPLE.as_bytes().to_vec();
        bytes.push(0);
        let definitions = SkinDefinitions::parse(&bytes).unwrap();

        assert_eq!(definitions.skin_count(), 2);
        assert_eq!(definitions.for_item("Door.Hinged.Metal")[0].defid, 10189);
        assert_eq!(definitions.for_item("smg.thompson").iter().map(|d| (d.defid, d.workshop_id)).collect::<Vec<_>>(), [(56702, 3_137_160_380)]);
        assert!(definitions.for_item("rifle.ak").is_empty());
    }

    #[test]
    fn only_the_newest_schema_file_is_read_and_an_inventory_file_never_is() {
        let steam = tempfile::tempdir().unwrap();
        let (old, new) = (steam.path().join("userdata/1/inventorymsgcache"), steam.path().join("userdata/2/inventorymsgcache"));
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(old.join("A.cachedmsg"), SAMPLE.replace("Industrial Door", "Old Door")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(new.join("B.cachedmsg"), SAMPLE).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(new.join("C.cachedmsg"), r#"{"inventory":[]}"#).unwrap();

        let definitions = SkinDefinitions::load(steam.path()).unwrap();

        assert_eq!(definitions.for_item("door.hinged.metal")[0].name, "Industrial Door");
    }
}
