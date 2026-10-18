//! The matcher as the corpus scores it, and one strip read through it.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use ro_capture::{bitmap_frames, gdiplus::GdiplusError, icon_signatures, skin_signatures, GameSkinIcons};
use ro_core::install::RustInstall;
use ro_vision::hud::{InventoryGeometry, InventoryReading, SlotHistory, SlotReading};
use ro_vision::items::{
    builtin_skin_faces, is_default_skin_category, retain_covering_skins, without_partial_skins, Catalogue, FrameSlotSampler,
    IconFace, IconMatch, IconMatcher, ItemCatalog, ItemDefinition, SkinCatalog,
};

use crate::args::{eq_ignore_case, Options};
use crate::strip_frame::StripFrame;
use crate::strip_name::StripName;

/// What `--skins` added, for the line the C# tool printed.
pub struct SkinsSummary {
    pub icons: usize,
    pub unreadable: usize,
    pub unmapped: Vec<(String, u32)>,
}

impl SkinsSummary {
    pub fn line(&self) -> String {
        let unmapped: Vec<String> = self.unmapped.iter().map(|(label, n)| format!("{label} x{n}")).collect();
        format!("skins: {} icons   unreadable {}   unmapped labels {}", self.icons, self.unreadable, unmapped.join(", "))
    }
}

/// Every catalogue icon, plus the workshop skins when `--skins` asks for them.
pub fn build_signatures(install: &RustInstall, options: &Options) -> std::io::Result<(Catalogue, Option<SkinsSummary>)> {
    let catalog = Arc::new(ItemCatalog::load(&install.items_directory())?);
    // The shipped skins are part of the install the agent reads, so every run carries them as it does.
    let shipped = builtin_skin_faces(&catalog, &install.items_directory(), |i| is_default_skin_category(&i.category));
    let (signatures, _) = icon_signatures::build(catalog.with_icons().cloned().map(IconFace::from).chain(shipped));
    let (mut signatures, _) = without_partial_skins(signatures);

    // Approved skins out of the game's bundles, for the items named: what the agent loads once its
    // skin hunt has asked for them.
    if let Some(items) = options.game_skins.as_deref() {
        let directory = std::env::temp_dir().join("ro-corpus-game-skins");
        let game = ro_core::install::steam_root().and_then(|steam| GameSkinIcons::load(&steam, install));
        let icons = items.iter().map(|item| (item.clone(), game.as_ref().map_or_else(Vec::new, |g| g.extract(item, &directory)))).collect();
        let added = skin_signatures(icons, &catalog, &signatures, None);
        signatures.extend(added);
    }

    // Off by default so a run stays comparable with every score recorded before skins existed.
    if !options.skins {
        return Ok((signatures, None));
    }

    let categories = if options.all_skins {
        Some(catalog.items().iter().map(|i| i.category.clone()).collect())
    } else {
        options.skin_categories.as_deref().map(|only| spellings_of(&catalog, only))
    };

    let mut skins = SkinCatalog::new(Arc::clone(&catalog), install.skins_directory(), categories);
    let (mut skinned, unreadable) = icon_signatures::build(skins.rescan());
    retain_covering_skins(&mut skinned, &signatures);
    let icons = skinned.len();
    for (face, signature) in skinned {
        signatures.insert(face, signature);
    }

    let unmapped = skins.unmapped_labels().map(|(label, n)| (label.to_owned(), n)).collect();
    Ok((signatures, Some(SkinsSummary { icons, unreadable: unreadable.len(), unmapped })))
}

/// SkinCatalog matches a caller's set exactly, where C# handed it an OrdinalIgnoreCase set; adding
/// the catalogue's own spelling of each requested category restores the case-insensitive match.
fn spellings_of(catalog: &ItemCatalog, requested: &[String]) -> HashSet<String> {
    let mut set: HashSet<String> = requested.iter().cloned().collect();
    set.extend(
        catalog
            .items()
            .iter()
            .filter(|i| requested.iter().any(|r| eq_ignore_case(r, &i.category)))
            .map(|i| i.category.clone()),
    );
    set
}

pub fn build_matcher(signatures: Catalogue, options: &Options) -> IconMatcher {
    let mut matcher = IconMatcher::new(signatures);
    matcher.scene_composited = !options.flat;
    if let Some(departure) = options.departure {
        matcher.minimum_departure = departure;
    }
    if let Some(coarse) = options.coarse {
        matcher.coarse_shortlist = coarse;
    }
    matcher
}

/// One strip's belt: each slot as the matcher identified it, and the reading after `history`.
pub struct StripReading {
    pub raw: Vec<Option<IconMatch>>,
    pub confirmed: InventoryReading,
    pub geometry: InventoryGeometry,
}

/// Reads the first `slots` belt slots (at most the belt) of the strip at `path`.
pub fn read_strip(
    path: &Path,
    strip: &StripName,
    matcher: &IconMatcher,
    categories: Option<&[String]>,
    history: Option<&mut SlotHistory>,
    slots: usize,
) -> Result<StripReading, GdiplusError> {
    let frame = StripFrame::new(
        bitmap_frames::read_file(path)?,
        strip.frame_width,
        strip.frame_height,
        strip.origin_x,
        strip.origin_y,
    );
    let geometry = InventoryGeometry::new(strip.frame_width, strip.frame_height, strip.ui_scale);
    let accept = categories.map(|only| move |item: &ItemDefinition| only.iter().any(|c| eq_ignore_case(c, &item.category)));
    let filter = accept.as_ref().map(|f| f as &dyn Fn(&ItemDefinition) -> bool);

    let belt: Vec<SlotReading> = geometry.belt()[..slots.min(geometry.belt().len())]
        .iter()
        .map(|&slot| SlotReading {
            slot,
            // over_world: these are all world-view frames, which is the whole point of the corpus.
            item: matcher.identify_sampler(&FrameSlotSampler::new(&frame, slot), filter, true),
            count: None,
        })
        .collect();

    let raw = belt.iter().map(|s| s.item.clone()).collect();
    let reading = InventoryReading { belt, main: Vec::new(), wear: Vec::new(), backpack: None };
    let confirmed = match history {
        Some(history) => history.confirm(&frame, &reading, false, strip.read_at),
        None => reading,
    };

    Ok(StripReading { raw, confirmed, geometry })
}

/// The answer and its look-alikes by short name, answer first.
pub fn named(found: &IconMatch) -> Vec<String> {
    std::iter::once(&found.item).chain(&found.indistinguishable).map(|i| i.short_name.clone()).collect()
}
