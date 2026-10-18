use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use rayon::prelude::*;
use serde::Deserialize;

use super::ItemDefinition;
use crate::text::fold;

/// The item definitions shipped in `<Rust>\Bundles\items`, indexed for slot matching.
///
/// The install carries a definition JSON and a 512x512 ARGB icon per item; the icons' alpha
/// channel doubles as the matching mask. Not every PNG has a JSON — some are variants such as
/// `50cal.mounted.left.png` — so the catalogue is driven from the definitions and treats a missing
/// icon as unmatched rather than an error.
#[derive(Debug, Default)]
pub struct ItemCatalog {
    items: Vec<Arc<ItemDefinition>>,
    by_short_name: HashMap<String, Arc<ItemDefinition>>,
    by_id: HashMap<i32, Arc<ItemDefinition>>,
    by_name: HashMap<String, Arc<ItemDefinition>>,
}

impl ItemCatalog {
    /// From definitions already in hand, rather than from an install. For tests.
    pub fn from_items(items: Vec<ItemDefinition>) -> Self {
        let items: Vec<Arc<ItemDefinition>> = items.into_iter().map(Arc::new).collect();
        let mut catalog = Self::default();

        // First wins on every index. Display names are not unique — deployable variants share one.
        // TODO(port): C# threw on a duplicate short name (ToDictionary); this keeps the first.
        for item in &items {
            catalog.by_short_name.entry(fold(&item.short_name)).or_insert_with(|| Arc::clone(item));
            catalog.by_id.entry(item.item_id).or_insert_with(|| Arc::clone(item));
            catalog.by_name.entry(fold(&item.name)).or_insert_with(|| Arc::clone(item));
        }

        catalog.items = items;
        catalog
    }

    /// Reads every `*.json` definition in the directory, in directory enumeration order. A
    /// malformed definition costs one item, not the whole catalogue.
    pub fn load(items_directory: &Path) -> std::io::Result<Self> {
        let mut paths = Vec::new();
        for entry in fs::read_dir(items_directory)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
                paths.push(path);
            }
        }

        // Reads overlap because each open waits on the disk and the virus scanner, not the CPU;
        // errors are still taken in enumeration order so the first unreadable file is the one reported.
        let read: Vec<_> = paths.par_iter().map(|path| if path.is_file() { read(path) } else { Ok(None) }).collect();
        let mut items = Vec::with_capacity(read.len());
        for item in read {
            items.extend(item?);
        }

        Ok(Self::from_items(items))
    }

    pub fn items(&self) -> &[Arc<ItemDefinition>] {
        &self.items
    }

    pub fn with_icons(&self) -> impl Iterator<Item = &Arc<ItemDefinition>> {
        self.items.iter().filter(|i| i.icon_path.is_some())
    }

    pub fn wearable(&self) -> impl Iterator<Item = &Arc<ItemDefinition>> {
        self.with_icons().filter(|i| i.is_wearable)
    }

    pub fn holdable(&self) -> impl Iterator<Item = &Arc<ItemDefinition>> {
        self.with_icons().filter(|i| i.is_holdable)
    }

    /// Case-insensitive, as every short-name lookup in the agent is.
    pub fn by_short_name(&self, short_name: &str) -> Option<&Arc<ItemDefinition>> {
        self.by_short_name.get(&fold(short_name))
    }

    pub fn by_id(&self, item_id: i32) -> Option<&Arc<ItemDefinition>> {
        self.by_id.get(&item_id)
    }

    /// By display name, case-insensitive. How a skin manifest names the item it paints.
    pub fn by_name(&self, name: &str) -> Option<&Arc<ItemDefinition>> {
        self.by_name.get(&fold(name))
    }
}

#[derive(Deserialize)]
struct Dto {
    #[serde(rename = "itemid", default)]
    item_id: i32,
    #[serde(rename = "shortname", default)]
    short_name: Option<String>,
    #[serde(rename = "Name", default)]
    name: Option<String>,
    #[serde(rename = "Category", default)]
    category: Option<String>,
    #[serde(rename = "stackable", default)]
    stackable: i32,
    #[serde(rename = "isWearable", default)]
    is_wearable: bool,
    #[serde(rename = "isHoldable", default)]
    is_holdable: bool,
    #[serde(rename = "condition", default)]
    condition: Option<ConditionDto>,
}

#[derive(Deserialize)]
struct ConditionDto {
    #[serde(default)]
    enabled: bool,
}

/// An unreadable file fails the whole load, as File.ReadAllText threw; malformed JSON costs one item.
fn read(path: &Path) -> std::io::Result<Option<ItemDefinition>> {
    let text = read_all_text(path)?;
    let Some(dto) = serde_json::from_str::<Option<Dto>>(&text).ok().flatten() else { return Ok(None) };
    let Some(short_name) = dto.short_name.filter(|s| !s.is_empty()) else { return Ok(None) };
    let icon = path.with_extension("png");

    Ok(Some(ItemDefinition {
        item_id: dto.item_id,
        name: dto.name.unwrap_or_else(|| short_name.clone()),
        short_name,
        category: dto.category.unwrap_or_else(|| "Unknown".to_owned()),
        is_wearable: dto.is_wearable,
        is_holdable: dto.is_holdable,
        has_condition_bar: dto.condition.is_some_and(|c| c.enabled),
        stack_size: 1.max(dto.stackable),
        icon_path: icon.is_file().then_some(icon),
    }))
}

/// `File.ReadAllText`: invalid UTF-8 decodes to U+FFFD rather than failing, and a BOM is dropped.
pub(crate) fn read_all_text(path: &Path) -> std::io::Result<String> {
    let bytes = fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);

    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned())
}
