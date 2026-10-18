//! Icons of approved skins that arrive from outside the install, as faces for the matcher.

use std::path::PathBuf;
use std::sync::Arc;

use ro_vision::items::{is_default_skin_category, retain_covering_skins, Catalogue, IconFace, ItemCatalog};

use crate::{icon_signatures, SignatureCache};

/// Icon files by the short name of the item they are skins of.
pub type ArrivedIcons = Vec<(String, Vec<PathBuf>)>;

/// Where icons of approved skins come from. The vision loop says which items it is failing to
/// read; the source decides whether and when to fetch, and hands back what it has.
pub trait SkinIconSource: Send + Sync {
    /// Whether any approved skin of the item exists, so the loop never asks for one that has none.
    fn has_skins(&self, item_short_name: &str) -> bool;
    /// Asks for every approved skin of the item. Cheap and repeatable: the source keeps its own
    /// record of what it fetched and when it last failed.
    fn request(&self, item_short_name: &str);
    /// Icons that became available since the last call, by item short name.
    fn take_arrived(&self) -> ArrivedIcons;
}

/// Turns arrived icons into faces through the same gates every skin passes: the category
/// allow-list (which keeps `locker` out), the coverage gate, and the signature cache.
pub(crate) struct FetchedSkins {
    source: Arc<dyn SkinIconSource>,
    catalog: Arc<ItemCatalog>,
    cache: Arc<SignatureCache>,
}

impl FetchedSkins {
    pub fn new(source: Arc<dyn SkinIconSource>, catalog: Arc<ItemCatalog>, cache: Arc<SignatureCache>) -> Self {
        Self { source, catalog, cache }
    }

    pub fn source(&self) -> &dyn SkinIconSource {
        self.source.as_ref()
    }

    /// Signatures for every icon that arrived, against `vanilla` for the coverage gate. Empty when
    /// nothing did, which is the common case and costs one lock.
    pub fn take(&self, vanilla: &Catalogue) -> Catalogue {
        skin_signatures(self.source.take_arrived(), &self.catalog, vanilla, Some(&self.cache))
    }
}

/// Signatures for skin icons by item, through the gates every skin passes: the category allow-list
/// (which keeps `locker` out), the coverage gate against `vanilla`, and `cache` when given.
pub fn skin_signatures(icons: ArrivedIcons, catalog: &ItemCatalog, vanilla: &Catalogue, cache: Option<&SignatureCache>) -> Catalogue {
    let faces: Vec<IconFace> = icons
        .into_iter()
        .filter_map(|(item, paths)| Some((catalog.by_short_name(&item)?.clone(), paths)))
        .filter(|(item, _)| is_default_skin_category(&item.category))
        .flat_map(|(item, paths)| paths.into_iter().map(move |path| IconFace::new(Arc::clone(&item), Some(path))))
        .collect();
    if faces.is_empty() {
        return Catalogue::new();
    }

    let (mut built, unreadable) = match cache {
        Some(cache) => icon_signatures::build_cached(faces, cache),
        None => icon_signatures::build(faces),
    };
    let dropped = retain_covering_skins(&mut built, vanilla);
    tracing::info!(added = built.len(), unreadable = unreadable.len(), partial = dropped, "Approved skins' icons joined the catalogue");
    built
}
