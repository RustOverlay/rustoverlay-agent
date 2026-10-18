use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use indexmap::IndexMap;
use rayon::prelude::*;

use super::item_catalog::read_all_text;
use super::{skin_item_types, IconFace, ItemCatalog};
use crate::text::{eq_ignore_case, fold};

/// Categories whose skins are carried. An allow-list, so a category nobody has measured stays out.
///
/// Over 236 recorded frames, all faces admitted gave 474 correct and 23 wrong; this list gives 466
/// and 0. `Attire` and `Construction` are safe only behind the skin coverage gate, which drops the
/// fragment faces (hinge plates, headwraps) their wrong reads came from. `Items` stays out: `locker`
/// is a full-canvas dark rectangle the gate cannot touch. See research/matcher-accuracy.md.
pub const DEFAULT_CATEGORIES: &[&str] =
    &["Weapon", "Tool", "Medical", "Food", "Ammunition", "Component", "Resources", "Attire", "Construction"];

/// The store thumbnail, `workshop_icon.png`, sits beside it and is not what a slot draws.
const ICON_NAME: &str = "icon.png";

const MANIFEST_NAME: &str = "manifest.txt";

/// The skin icons Steam has cached, resolved to the items they repaint.
///
/// Each workshop directory holds the icon the game draws for that skin plus a manifest naming the
/// item it paints. A skin is a texture swap, so it matches as its own [`IconFace`] of the same
/// item. Unmatched, a skinned slot is not merely missed: the matcher was confidently wrong on 12
/// of 1457 cached skins. `rescan` rather than a constructor because Steam keeps filling the cache
/// while the game runs.
pub struct SkinCatalog {
    items: Arc<ItemCatalog>,
    directory: PathBuf,
    /// None is [`DEFAULT_CATEGORIES`], matched case-insensitively; a caller's own set is exact.
    categories: Option<HashSet<String>>,
    seen: HashSet<PathBuf>,
    /// Folded label to (label as first seen, skins it cost), in first-seen order like C#'s Dictionary.
    unmapped: IndexMap<String, (String, u32)>,
}

enum Read {
    Found(IconFace),
    /// Still being written, or unreadable this moment. Worth another look.
    Incomplete,
    /// Read cleanly and holds nothing for us. Settled.
    Ignored,
    /// Settled, but names an item the catalogue does not know; counted in `unmapped`.
    Unmapped(String),
}

impl SkinCatalog {
    pub fn new(items: Arc<ItemCatalog>, directory: impl Into<PathBuf>, categories: Option<HashSet<String>>) -> Self {
        Self { items, directory: directory.into(), categories, seen: HashSet::new(), unmapped: IndexMap::new() }
    }

    /// Workshop labels no item answers to, and how many skins each cost. Empty is the healthy
    /// state; a renamed item shows up here rather than as a silent gap in the catalogue.
    pub fn unmapped_labels(&self) -> impl Iterator<Item = (&str, u32)> {
        self.unmapped.values().map(|(label, count)| (label.as_str(), *count))
    }

    /// Skins one unmapped label cost, matched case-insensitively.
    pub fn unmapped_count(&self, label: &str) -> Option<u32> {
        self.unmapped.get(&fold(label)).map(|(_, count)| *count)
    }

    /// Reads directories not read before, and returns the faces that were added.
    ///
    /// Steam writes a skin's files into their final directory one by one, so a sweep can arrive
    /// mid-write. Such a directory is left unremembered and picked up next time; one whose
    /// manifest reads but names nothing is remembered, so an unknown label costs one parse rather
    /// than one per sweep.
    pub fn rescan(&mut self) -> Vec<IconFace> {
        let Ok(entries) = fs::read_dir(&self.directory) else { return Vec::new() };
        let fresh: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| !self.seen.contains(p) && p.is_dir()).collect();

        // A first sweep opens ~1500 manifests, each waiting on the disk and the virus scanner, so
        // the reads overlap; bookkeeping is applied afterwards in enumeration order.
        let reads: Vec<Read> = fresh.par_iter().map(|path| self.try_read(path)).collect();
        let mut added = Vec::new();

        for (path, read) in fresh.into_iter().zip(reads) {
            match read {
                Read::Found(face) => added.push(face),
                Read::Incomplete => continue,
                Read::Unmapped(label) => self.unmapped.entry(fold(&label)).or_insert_with(|| (label, 0)).1 += 1,
                Read::Ignored => {}
            }
            self.seen.insert(path);
        }

        added
    }

    /// Drops a face so a later [`Self::rescan`] reconsiders it. For the caller that decodes the
    /// icon, since a truncated PNG is the other half of the mid-write case and only the decode
    /// can see it.
    pub fn forget(&mut self, face: &IconFace) {
        if let Some(directory) = face.icon_path.as_deref().and_then(Path::parent) {
            self.seen.remove(directory);
        }
    }

    fn try_read(&self, path: &Path) -> Read {
        let icon = path.join(ICON_NAME);
        let manifest = path.join(MANIFEST_NAME);

        if !icon.is_file() || !manifest.is_file() {
            return Read::Incomplete;
        }

        let Ok(text) = read_all_text(&manifest) else { return Read::Incomplete };
        let Ok(document) = serde_json::from_str::<serde_json::Value>(&text) else { return Read::Incomplete };

        // TODO(port): C# threw on a non-object root or a non-string ItemType; both are ignored here.
        let label = match document.get("ItemType").and_then(serde_json::Value::as_str) {
            Some(label) if !label.is_empty() => label,
            _ => return Read::Ignored,
        };

        let item = match skin_item_types::short_name_for_label(label) {
            Some(short_name) => self.items.by_short_name(short_name),
            None => self.items.by_name(label),
        };

        let Some(item) = item else { return Read::Unmapped(label.to_owned()) };

        if !self.allows(&item.category) {
            return Read::Ignored;
        }

        Read::Found(IconFace::new(Arc::clone(item), Some(icon)))
    }

    fn allows(&self, category: &str) -> bool {
        match &self.categories {
            None => is_default_skin_category(category),
            Some(categories) => categories.contains(category),
        }
    }
}

/// Whether [`DEFAULT_CATEGORIES`] carries skins of `category`, case-insensitively.
pub fn is_default_skin_category(category: &str) -> bool {
    DEFAULT_CATEGORIES.iter().any(|c| eq_ignore_case(c, category))
}
